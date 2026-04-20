use enki_next::agent::{Agent, AgentDefinition};
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

fn usage(program: &str) {
    eprintln!(
        "Usage: {program} <prompt>\n\
         Optional env vars:\n\
         - ENKI_MODEL=model-id\n\
         - ENKI_WORKSPACE=path\n\
         - ENKI_SESSION=session-id"
    );
}

#[tokio::main]
async fn main() {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "simple_agent".to_string());
    let prompt = args.collect::<Vec<_>>().join(" ");

    if prompt.trim().is_empty() {
        usage(&program);
        std::process::exit(1);
    }

    let workspace_home = env::var("ENKI_WORKSPACE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("crates/core/examples/.agent-workspace")));
    let session_id = env::var("ENKI_SESSION").unwrap_or_else(|_| "demo-session".to_string());

    let definition = AgentDefinition {
        name: "Capabilities Demo".to_string(),
        system_prompt_preamble: "You are a concise demo agent. Prefer using tools when they help demonstrate your capabilities such as reading files, writing files, and running commands in the task workspace.".to_string(),
        model: env::var("ENKI_MODEL").unwrap_or_else(|_| "ollama::qwen3.5".to_string()),
        max_iterations: 12,
    };

    let agent = match Agent::with_definition_executor_and_workspace(
        definition,
        Box::new(enki_next::tooling::tool_calling::RegistryToolExecutor),
        workspace_home,
    )
    .await
    {
        Ok(agent) => agent,
        Err(err) => {
            eprintln!("Failed to initialize agent: {err}");
            std::process::exit(1);
        }
    };

    println!("Session: {session_id}");
    println!("Prompt: {prompt}");
    print!("Running agent...");
    let _ = io::stdout().flush();

    let response = agent.run(&session_id, &prompt).await;

    println!("\n\nResponse:\n{response}");
}
