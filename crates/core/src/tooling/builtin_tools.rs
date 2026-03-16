use crate::tooling::types::{
    Tool, ToolContext, ToolRegistry, ToolRegistryBuilder, parse_tool_args,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;
use tokio::process::Command;

#[derive(Deserialize)]
struct ReadFileParams {
    path: String,
}

pub struct ReadFileTool;

#[async_trait(?Send)]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read file content."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let params: ReadFileParams = match parse_tool_args(args) {
            Ok(params) => params,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

        let resolved = ctx.workspace_dir.join(params.path);
        fs::read_to_string(&resolved)
            .await
            .unwrap_or_else(|_| "File not found.".to_string())
    }
}

#[derive(Deserialize)]
struct WriteFileParams {
    path: String,
    content: String,
}

pub struct WriteFileTool;

#[async_trait(?Send)]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to file."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let params: WriteFileParams = match parse_tool_args(args) {
            Ok(params) => params,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

        let resolved = ctx.workspace_dir.join(params.path);
        if let Some(parent) = resolved.parent() {
            let _ = fs::create_dir_all(parent).await;
        }

        match fs::write(&resolved, params.content).await {
            Ok(_) => "File written.".to_string(),
            Err(error) => format!("Error: {error}"),
        }
    }
}

#[derive(Deserialize)]
struct ExecParams {
    cmd: String,
}

pub struct ExecTool;

#[async_trait(?Send)]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute shell command safely."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cmd": { "type": "string" }
            },
            "required": ["cmd"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let params: ExecParams = match parse_tool_args(args) {
            Ok(params) => params,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

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
}

pub fn default_registry() -> ToolRegistry {
    ToolRegistryBuilder::new()
        .register(ReadFileTool)
        .register(WriteFileTool)
        .register(ExecTool)
        .build()
}
