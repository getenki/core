use crate::tooling::types::{ToolContext, ToolParams, ToolRegistry, ToolRegistryBuilder};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;
use tokio::process::Command;

#[derive(Deserialize)]
pub struct ReadFileParams {
    pub path: String,
}

impl ToolParams for ReadFileParams {
    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }
}

#[derive(Deserialize)]
pub struct WriteFileParams {
    pub path: String,
    pub content: String,
}

impl ToolParams for WriteFileParams {
    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }
}

#[derive(Deserialize)]
pub struct ExecParams {
    pub cmd: String,
}

impl ToolParams for ExecParams {
    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string" }
            },
            "required": ["cmd"]
        })
    }
}

pub async fn read_file_tool(ctx: ToolContext, params: ReadFileParams) -> String {
    let resolved = ctx.workspace_dir.join(params.path);
    fs::read_to_string(&resolved)
        .await
        .unwrap_or_else(|_| "File not found.".to_string())
}

pub async fn write_file_tool(ctx: ToolContext, params: WriteFileParams) -> String {
    let resolved = ctx.workspace_dir.join(params.path);
    if let Some(parent) = resolved.parent() {
        let _ = fs::create_dir_all(parent).await;
    }

    match fs::write(&resolved, params.content).await {
        Ok(_) => "File written.".to_string(),
        Err(error) => format!("Error: {error}"),
    }
}

pub async fn exec_tool(ctx: ToolContext, params: ExecParams) -> String {
    #[cfg(windows)]
    let (shell, flag) = ("cmd", "/C");
    #[cfg(not(windows))]
    let (shell, flag) = ("sh", "-c");

    match Command::new(shell)
        .arg(flag)
        .arg(params.cmd)
        .current_dir(&ctx.workspace_dir)
        .output()
        .await
    {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            let rc = result.status.code().unwrap_or(-1);

            format!("stdout: {stdout}\nstderr: {stderr}\nrc: {rc}")
        }
        Err(error) => format!("Exec error: {error}"),
    }
}

pub fn default_registry() -> ToolRegistry {
    ToolRegistryBuilder::new()
        .register_typed_async_fn::<ReadFileParams, _>(
            "read_file",
            "Read file content.",
            read_file_tool,
        )
        .register_typed_async_fn::<WriteFileParams, _>(
            "write_file",
            "Write content to file.",
            write_file_tool,
        )
        .register_typed_async_fn::<ExecParams, _>(
            "exec",
            "Execute shell command safely.",
            exec_tool,
        )
        .build()
}
