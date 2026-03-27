use crate::cli::RunArgs;
use crate::manifest::Manifest;
use crate::project_runtime;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;

pub async fn run(args: RunArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let project_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let workspace_home = resolve_workspace_home(&args, &manifest);

    println!(
        "\x1b[1;36m⚡ Running agents\x1b[0m from '{}'",
        manifest.project.name
    );
    println!();

    if let Some(agent_id) = &args.agent {
        // Run a single agent
        let agent_cfg = manifest
            .agents
            .iter()
            .find(|a| a.id == *agent_id)
            .ok_or_else(|| {
                format!(
                    "Agent '{}' not found in manifest. Available: {}",
                    agent_id,
                    manifest
                        .agents
                        .iter()
                        .map(|a| a.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;

        println!(
            "  \x1b[2mAgent:\x1b[0m {} ({})",
            agent_cfg.name, agent_cfg.model
        );
        println!("  \x1b[2mMessage:\x1b[0m {}", args.message);
        println!();

        if project_runtime::is_python_project(project_dir) {
            let response = project_runtime::run_python_agent(
                &manifest,
                project_dir,
                &workspace_home,
                &agent_cfg.id,
                "cli-session",
                &args.message,
            )
            .await?;

            println!("\x1b[1;33m{}:\x1b[0m {}", agent_cfg.name, response);
            println!();
            return Ok(());
        }

        let mut builder = MultiAgentRuntimeBuilder::new().with_workspace_home(&workspace_home);

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

        let runtime = builder.build().await?;
        let response = runtime.process(agent_id, "cli-session", &args.message).await?;

        println!("\x1b[1;33m{}:\x1b[0m {}", agent_cfg.name, response);
    } else {
        if project_runtime::is_python_project(project_dir) {
            let first_agent = &manifest.agents[0];
            println!(
                "  \x1b[2mRouting to:\x1b[0m {} (first agent)",
                first_agent.name
            );
            println!("  \x1b[2mMessage:\x1b[0m {}", args.message);
            println!();

            let response = project_runtime::run_python_agent(
                &manifest,
                project_dir,
                &workspace_home,
                &first_agent.id,
                "cli-session",
                &args.message,
            )
            .await?;

            println!("\x1b[1;33m{}:\x1b[0m {}", first_agent.name, response);
            println!();
            return Ok(());
        }

        // Run all agents in a multi-agent runtime
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

        // Send the message to the first agent (orchestrator pattern)
        let first_agent = &manifest.agents[0];
        println!(
            "  \x1b[2mRouting to:\x1b[0m {} (first agent)",
            first_agent.name
        );
        println!("  \x1b[2mMessage:\x1b[0m {}", args.message);
        println!();

        let response = runtime
            .process(&first_agent.id, "cli-session", &args.message)
            .await?;

        println!("\x1b[1;33m{}:\x1b[0m {}", first_agent.name, response);
    }

    println!();
    Ok(())
}

fn resolve_workspace_home(args: &RunArgs, manifest: &Manifest) -> String {
    let manifest_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));
    manifest_dir
        .join(&manifest.workspace.home)
        .to_string_lossy()
        .to_string()
}
