use crate::cli::JoinArgs;
use crate::interactive_cli::{ClientEvent, InteractiveCliClient};
use crate::manifest::Manifest;
use crate::project_runtime;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;
use std::io;

pub async fn run(args: JoinArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let manifest_dir = args.manifest.parent().unwrap_or(std::path::Path::new("."));
    project_runtime::validate_python_tools(&manifest, manifest_dir)?;
    let workspace_home = manifest_dir
        .join(&manifest.workspace.home)
        .to_string_lossy()
        .to_string();
    let is_python_project = project_runtime::is_python_project(manifest_dir);

    let runtime = if is_python_project {
        None
    } else {
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

        Some(builder.build().await?)
    };

    let default_agent_id = args.agent.as_deref().unwrap_or(&manifest.agents[0].id);
    if manifest
        .agents
        .iter()
        .all(|agent| agent.id != default_agent_id)
    {
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

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut client = InteractiveCliClient::new(default_agent_id, &manifest.agents);
    client.print_welcome(&manifest.project.name);

    loop {
        let event = match client.next_event(&mut stdin) {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(err) => {
                println!("  \x1b[31mError:\x1b[0m {}", err);
                println!();
                continue;
            }
        };

        match event {
            ClientEvent::Quit => {
                println!("\n  \x1b[2mGoodbye!\x1b[0m\n");
                break;
            }
            ClientEvent::ShowAgents => {
                client.print_agents(&manifest.agents);
                continue;
            }
            ClientEvent::ShowHelp => {
                client.print_help();
                continue;
            }
            ClientEvent::SendMessage { agent_id, message } => {
                let session_id = client.next_session_id(&agent_id);

                println!();
                println!("  \x1b[2m[{}]\x1b[0m", agent_id);

                let response = if is_python_project {
                    project_runtime::run_python_agent(
                        &manifest,
                        manifest_dir,
                        &workspace_home,
                        &agent_id,
                        &session_id,
                        &message,
                        false,
                    )
                    .await
                } else {
                    runtime
                        .as_ref()
                        .expect("runtime available for non-Python projects")
                        .process(&agent_id, &session_id, &message)
                        .await
                };

                match response {
                    Ok(response) => {
                        let agent_name = manifest
                            .agents
                            .iter()
                            .find(|a| a.id == agent_id)
                            .map(|a| a.name.as_str())
                            .unwrap_or(&agent_id);

                        println!("  \x1b[1;33m{}:\x1b[0m {}", agent_name, response);
                    }
                    Err(e) => {
                        println!("  \x1b[31mError:\x1b[0m {}", e);
                    }
                }

                println!();
            }
        }
    }

    Ok(())
}
