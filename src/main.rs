#[macro_use]
mod macros;
pub mod tooling;
pub mod agent;
pub mod message;
pub mod runtime;

use crate::tooling::builtin_tools::{ExecTool, ReadFileTool, WriteFileTool};
use crate::tooling::types::*;
use crate::runtime::{CliChannel, RuntimeBuilder};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;

fn api_key() -> Option<String> {
    None
    // env::var("ANTHROPIC_API_KEY").ok()
}

fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn build_context() -> ToolContext {
    let workspace_dir = home_dir().join(".atomiagent");
    let sessions_dir = workspace_dir.join("sessions");

    ToolContext {
        workspace_dir,
        sessions_dir,
    }
}

async fn ensure_dirs(ctx: &ToolContext) -> Result<(), String> {
    fs::create_dir_all(&ctx.sessions_dir)
        .await
        .map_err(|e| format!("Failed to create dirs: {e}"))
}

fn build_tools() -> ToolRegistry {
    register_tools![ReadFileTool, WriteFileTool, ExecTool]
}

async fn post_json(url: &str, payload: &Value, timeout_secs: u64) -> Result<Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Client error: {e}"))?;

    let mut req = client.post(url).json(payload);

    if let Some(key) = api_key() {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await.map_err(|e| format!("Connection error: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("Read error: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), body));
    }

    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON response: {e}"))
}


#[tokio::main]
async fn main() {
    let runtime = match RuntimeBuilder::for_default_agent().build().await {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut channel = match CliChannel::from_args(env::args().collect()) {
        Ok(channel) => channel,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = runtime.serve_channel(&mut channel).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
