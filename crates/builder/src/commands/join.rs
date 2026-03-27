use crate::cli::JoinArgs;
use crate::manifest::Manifest;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;
use std::io::{self, BufRead, Write};

pub async fn run(args: JoinArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;

    let manifest_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let workspace_home = manifest_dir
        .join(&manifest.workspace.home)
        .to_string_lossy()
        .to_string();

    // Build the multi-agent runtime
    let mut builder = MultiAgentRuntimeBuilder::new().with_workspace_home(&workspace_home);
    for agent_cfg in &manifest.agents {
        builder = builder.add_agent(
            &agent_cfg.id,
            AgentDefinition {
                name: agent_cfg.name.clone(),
                system_prompt_preamble: agent_cfg.system_prompt.clone(),
                model: agent_cfg.model.clone(),
                max_iterations: agent_cfg.max_iterations,
            },
            agent_cfg.capabilities.clone(),
        );
    }

    let runtime = builder.build().await?;

    // Determine default agent
    let default_agent_id = args
        .agent
        .as_deref()
        .unwrap_or(&manifest.agents[0].id);

    // Verify default agent exists
    if runtime.registry().get(default_agent_id).await.is_none() {
        return Err(format!(
            "Agent '{}' not found. Available: {}",
            default_agent_id,
            manifest
                .agents
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let agent_ids: Vec<String> = manifest.agents.iter().map(|a| a.id.clone()).collect();

    // Print welcome banner
    println!();
    println!("\x1b[1;36m╔══════════════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m  \x1b[1mEnki Interactive Session\x1b[0m                                \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m  Project: {:<47}\x1b[1;36m║\x1b[0m", manifest.project.name);
    println!("\x1b[1;36m╠══════════════════════════════════════════════════════════╣\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m                                                          \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m  \x1b[2mSend messages to agents:\x1b[0m                                \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m    \x1b[33m>\x1b[0m hello                    → default ({:<14})\x1b[1;36m║\x1b[0m", default_agent_id);
    println!("\x1b[1;36m║\x1b[0m    \x1b[33m>\x1b[0m @agent-id hello          → specific agent       \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m    \x1b[33m>\x1b[0m /agents                  → list all agents       \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m    \x1b[33m>\x1b[0m quit                     → exit                  \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m                                                          \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m╚══════════════════════════════════════════════════════════╝\x1b[0m");
    println!();

    let stdin = io::stdin();
    let mut session_counter = 0u64;

    loop {
        print!("\x1b[1;33m❯\x1b[0m ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input.to_lowercase().as_str() {
            "quit" | "exit" | "q" => {
                println!("\n  \x1b[2mGoodbye!\x1b[0m\n");
                break;
            }
            "/agents" => {
                println!();
                let cards = runtime.registry().list_all().await;
                for card in &cards {
                    let marker = if card.agent_id == default_agent_id {
                        " \x1b[33m(default)\x1b[0m"
                    } else {
                        ""
                    };
                    println!(
                        "  \x1b[36m•\x1b[0m {} ({}) — [{}]{}",
                        card.agent_id,
                        card.name,
                        card.capabilities.join(", "),
                        marker
                    );
                }
                println!();
                continue;
            }
            _ => {}
        }

        // Parse @agent-id prefix
        let (target_agent, message) = if input.starts_with('@') {
            if let Some(space_idx) = input.find(' ') {
                let agent_id = &input[1..space_idx];
                let msg = input[space_idx + 1..].trim();
                if agent_ids.iter().any(|id| id == agent_id) {
                    (agent_id.to_string(), msg.to_string())
                } else {
                    println!(
                        "\n  \x1b[31m✗\x1b[0m Unknown agent '{}'. Type /agents to see available agents.\n",
                        agent_id
                    );
                    continue;
                }
            } else {
                println!("\n  \x1b[31m✗\x1b[0m Usage: @agent-id <message>\n");
                continue;
            }
        } else {
            (default_agent_id.to_string(), input.to_string())
        };

        session_counter += 1;
        let session_id = format!("join-{}-{}", target_agent, session_counter);

        println!();
        println!("  \x1b[2m[{}]\x1b[0m", target_agent);

        match runtime
            .process(&target_agent, &session_id, &message)
            .await
        {
            Ok(response) => {
                // Find agent name
                let agent_name = manifest
                    .agents
                    .iter()
                    .find(|a| a.id == target_agent)
                    .map(|a| a.name.as_str())
                    .unwrap_or(&target_agent);

                println!("  \x1b[1;33m{}:\x1b[0m {}", agent_name, response);
            }
            Err(e) => {
                println!("  \x1b[31m✗ Error:\x1b[0m {}", e);
            }
        }

        println!();
    }

    Ok(())
}
