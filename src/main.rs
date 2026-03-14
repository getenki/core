#[macro_use]
mod macros;
pub mod tooling;
pub mod agent;
pub mod message;

use crate::tooling::builtin_tools::{ExecTool, ReadFileTool, WriteFileTool};
use crate::tooling::types::*;

use crate::agent::Agent;
use reqwest::blocking::Client;
use serde_json::Value;
use std::env;
use std::fs::{self};
use std::path::PathBuf;
use std::time::Duration;

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

fn ensure_dirs(ctx: &ToolContext) -> Result<(), String> {
    fs::create_dir_all(&ctx.sessions_dir).map_err(|e| format!("Failed to create dirs: {e}"))
}

fn build_tools() -> ToolRegistry {
    register_tools![ReadFileTool, WriteFileTool, ExecTool]
}

fn post_json(url: &str, payload: &Value, timeout_secs: u64) -> Result<Value, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Client error: {e}"))?;

    let mut req = client.post(url).json(payload);

    if let Some(key) = api_key() {
        req = req.bearer_auth(key);
    }

    let resp = req.send().map_err(|e| format!("Connection error: {e}"))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Read error: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), body));
    }

    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON response: {e}"))
}


fn main() {
    let agent = match Agent::new() {
        Ok(agent) => agent,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <session_id> '<message>'", args[0]);
        std::process::exit(1);
    }

    let session_id = &args[1];
    let message = args[2..].join(" ");

    let response = agent.run(session_id, &message);
    println!("{response}");
}
