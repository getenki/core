#[macro_use]
mod macros;
pub mod tooling;

use crate::tooling::types::*;

use reqwest::blocking::Client;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// Config
const LLM_URL: &str = "http://localhost:11434/api/chat";
const MODEL: &str = "qwen3.5:latest";
const MAX_ITERATIONS: usize = 20;

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
    |args, _ctx| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match fs::read_to_string(Path::new(path)) {
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
    |args, _ctx| {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let content = args
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match fs::write(Path::new(path), content) {
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

        match Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&ctx.workspace_dir)
            .output()
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

fn tool_catalog_json(tools: &ToolRegistry) -> Value {
    let mut map = Map::new();

    for tool in tools.values() {
        map.insert(tool.name().to_string(), tool.as_catalog_entry());
    }

    Value::Object(map)
}

fn tools_payload(tools: &ToolRegistry) -> Vec<Value> {
    tools.values().map(|tool| tool.as_tool_payload()).collect()
}

fn system_prompt(tools: &ToolRegistry, ctx: &ToolContext) -> String {
    format!(
        r#"You are a helpful Personal Assistant agent. Use tools via JSON calls when needed.
- To use a tool: respond with ONLY {{"tool": "tool_name", "args": {{...}}}}
- When done, respond with plain text answer.
Available tools: {}
Workspace: {}"#,
        tool_catalog_json(tools),
        ctx.workspace_dir.display()
    )
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

fn load_session(session_id: &str, system_prompt: &str, ctx: &ToolContext) -> Vec<Value> {
    let session_file = ctx.sessions_dir.join(format!("{session_id}.jsonl"));

    if session_file.exists() {
        if let Ok(file) = File::open(&session_file) {
            let reader = BufReader::new(file);
            let messages: Vec<Value> = reader
                .lines()
                .filter_map(Result::ok)
                .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
                .collect();

            if !messages.is_empty() {
                return messages;
            }
        }
    }

    vec![json!({
        "role": "system",
        "content": system_prompt
    })]
}

fn save_messages(session_id: &str, messages: &[Value], ctx: &ToolContext) -> Result<(), String> {
    let session_file = ctx.sessions_dir.join(format!("{session_id}.jsonl"));
    let start = messages.len().saturating_sub(10);

    let mut file = File::create(&session_file)
        .map_err(|e| format!("Failed to open session file for writing: {e}"))?;

    for msg in &messages[start..] {
        let line =
            serde_json::to_string(msg).map_err(|e| format!("Failed to serialize message: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("Failed to write session file: {e}"))?;
    }

    Ok(())
}

fn call_llm(messages: &[Value], tools: &ToolRegistry) -> String {
    let payload = json!({
        "model": MODEL,
        "messages": messages,
        "stream": false,
        "tools": tools_payload(tools),
    });

    match post_json(LLM_URL, &payload, 60) {
        Ok(data) => data
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        Err(e) => format!("LLM error: {e}"),
    }
}

fn execute_tool(tool_call: &Value, tools: &ToolRegistry, ctx: &ToolContext) -> String {
    let tool_name = tool_call
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let args = tool_call.get("args").unwrap_or(&Value::Null);

    match tools.get(tool_name) {
        Some(tool) => tool.execute(args, ctx),
        None => "Unknown tool.".to_string(),
    }
}

fn agent_loop(
    session_id: &str,
    user_message: &str,
    tools: &ToolRegistry,
    ctx: &ToolContext,
) -> String {
    let prompt = system_prompt(tools, ctx);
    let mut messages = load_session(session_id, &prompt, ctx);

    messages.push(json!({
        "role": "user",
        "content": user_message
    }));

    for _ in 0..MAX_ITERATIONS {
        let llm_response = call_llm(&messages, tools);

        messages.push(json!({
            "role": "assistant",
            "content": llm_response
        }));

        if let Ok(tool_call) = serde_json::from_str::<Value>(&llm_response) {
            if tool_call.is_object() && tool_call.get("tool").is_some() {
                let result = execute_tool(&tool_call, tools, ctx);

                messages.push(json!({
                    "role": "tool",
                    "content": result,
                    "tool_call_id": "1"
                }));

                let _ = save_messages(session_id, &messages, ctx);
                continue;
            }
        }

        let _ = save_messages(session_id, &messages, ctx);
        return llm_response;
    }

    "Max iterations reached.".to_string()
}

fn main() {
    let ctx = build_context();

    if let Err(e) = ensure_dirs(&ctx) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    let tools = build_tools();
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <session_id> '<message>'", args[0]);
        std::process::exit(1);
    }

    let session_id = &args[1];
    let message = args[2..].join(" ");

    let response = agent_loop(session_id, &message, &tools, &ctx);
    println!("{response}");
}
