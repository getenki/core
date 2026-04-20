//! Multi-agent example - two agents share a registry and can discover each
//! other. The "coordinator" agent receives the user prompt and the
//! "researcher" agent is available as a peer.
//!
//! ```powershell
//! $env:ENKI_MODEL="ollama::qwen3.5"   # or any supported provider::model
//! cargo run -p enki-next --example multi_agent -- "Summarize the repository structure"
//! ```
//!
//! Optional env vars:
//!   ENKI_MODEL      - model string (default: ollama::qwen3.5)
//!   ENKI_WORKSPACE  - workspace root (default: crates/core/examples/.agent-workspace)

use enki_next::agent::AgentDefinition;
use enki_next::runtime::MultiAgentRuntime;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

fn usage(program: &str) {
    eprintln!(
        "Usage: {program} <prompt>\n\
         Optional env vars:\n\
         - ENKI_MODEL=provider::model\n\
         - ENKI_WORKSPACE=path"
    );
}

#[tokio::main]
async fn main() {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "multi_agent".to_string());
    let prompt = args.collect::<Vec<_>>().join(" ");

    if prompt.trim().is_empty() {
        usage(&program);
        std::process::exit(1);
    }

    let workspace_home = env::var("ENKI_WORKSPACE")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/core/examples/.agent-workspace"));
    let model = env::var("ENKI_MODEL").unwrap_or_else(|_| "ollama::qwen3.5".to_string());

    // Agent definitions

    let coordinator_def = AgentDefinition {
        name: "Coordinator".to_string(),
        system_prompt_preamble: "\
            You are a coordinator agent. You can use `discover_agents` to find \
            peer agents and `delegate_task` to send work to them. \
            Prefer delegating research tasks to a peer with the 'research' \
            capability whenever appropriate."
            .to_string(),
        model: model.clone(),
        max_iterations: 50,
    };

    let researcher_def = AgentDefinition {
        name: "Researcher".to_string(),
        system_prompt_preamble: "\
            You are a research agent. You read files, run commands, and \
            synthesize concise answers. Focus on factual, well-structured \
            responses."
            .to_string(),
        model: model.clone(),
        max_iterations: 50,
    };

    // Build the multi-agent runtime

    let runtime = match MultiAgentRuntime::builder()
        .add_agent(
            "coordinator",
            coordinator_def,
            vec!["orchestration".into(), "planning".into()],
        )
        .add_agent(
            "researcher",
            researcher_def,
            vec!["research".into(), "analysis".into()],
        )
        .with_workspace_home(workspace_home)
        .build()
        .await
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("Failed to build multi-agent runtime: {err}");
            std::process::exit(1);
        }
    };

    // Show registered agents

    println!("=== Registered Agents ===");
    for card in runtime.registry().list_all().await {
        println!(
            "  - {} (id={}, capabilities={:?}, status={})",
            card.name, card.agent_id, card.capabilities, card.status
        );
    }
    println!();

    // Send the prompt to the coordinator

    println!("Prompt: {prompt}");
    print!("Running coordinator agent...");
    let _ = io::stdout().flush();

    let response = runtime
        .process("coordinator", "demo-session", &prompt)
        .await;

    match response {
        Ok(content) => println!("\n\nResponse:\n{content}"),
        Err(err) => eprintln!("\n\nError: {err}"),
    }
}
