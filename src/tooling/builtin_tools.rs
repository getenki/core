use crate::tooling::types::*;
use serde_json::{Value, json};
use tokio::fs;
use tokio::process::Command;

pub struct ReadFileTool;
pub struct WriteFileTool;
pub struct ExecTool;

define_tool!(
    ReadFileTool,
    name: "read_file",
    description: "Read file content.",
    parameters: json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" }
        },
        "required": ["path"]
    }),
    |args, ctx| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let resolved = ctx.workspace_dir.join(path);
        match fs::read_to_string(&resolved).await {
            Ok(content) => content,
            Err(_) => "File not found.".to_string(),
        }
    }
);

define_tool!(
    WriteFileTool,
    name: "write_file",
    description: "Write content to file.",
    parameters: json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" },
            "content": { "type": "string" }
        },
        "required": ["path", "content"]
    }),
    |args, ctx| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let content = args
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let resolved = ctx.workspace_dir.join(path);
        if let Some(parent) = resolved.parent() {
            let _ = fs::create_dir_all(parent).await;
        }
        match fs::write(&resolved, content).await {
            Ok(_) => "File written.".to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }
);

define_tool!(
    ExecTool,
    name: "exec",
    description: "Execute shell command safely.",
    parameters: json!({
        "type": "object",
        "properties": {
            "cmd": { "type": "string" }
        },
        "required": ["cmd"]
    }),
    |args, ctx| {
        let cmd = args
            .get("cmd")
            .and_then(Value::as_str)
            .unwrap_or_default();

        #[cfg(windows)]
        let (shell, flag) = ("cmd", "/C");
        #[cfg(not(windows))]
        let (shell, flag) = ("sh", "-c");

        match Command::new(shell)
            .arg(flag)
            .arg(cmd)
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
            Err(e) => format!("Exec error: {e}"),
        }
    }
);
