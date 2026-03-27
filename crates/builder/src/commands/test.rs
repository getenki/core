use crate::cli::TestArgs;
use crate::manifest::Manifest;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;

pub async fn run(args: TestArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;

    let manifest_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let workspace_home = manifest_dir
        .join(&manifest.workspace.home)
        .to_string_lossy()
        .to_string();

    println!(
        "\x1b[1;36m⚡ Testing agents\x1b[0m from '{}'",
        manifest.project.name
    );
    println!();

    let mut all_ok = true;

    for agent_cfg in &manifest.agents {
        print!(
            "  Testing '{}' ({})... ",
            agent_cfg.id, agent_cfg.model
        );

        let builder = MultiAgentRuntimeBuilder::new()
            .with_workspace_home(&workspace_home)
            .add_agent(
                &agent_cfg.id,
                AgentDefinition {
                    name: agent_cfg.name.clone(),
                    system_prompt_preamble: agent_cfg.system_prompt.clone(),
                    model: agent_cfg.model.clone(),
                    max_iterations: 2,
                },
                agent_cfg.capabilities.clone(),
            );

        match builder.build().await {
            Ok(runtime) => {
                // Send a simple ping
                match runtime
                    .process(&agent_cfg.id, "test-session", "Respond with OK")
                    .await
                {
                    Ok(response) => {
                        let preview: String = response.chars().take(80).collect();
                        println!("\x1b[32m✓\x1b[0m  {}", preview);
                    }
                    Err(e) => {
                        println!("\x1b[31m✗\x1b[0m  {}", e);
                        all_ok = false;
                    }
                }
            }
            Err(e) => {
                println!("\x1b[31m✗\x1b[0m  Build failed: {}", e);
                all_ok = false;
            }
        }
    }

    println!();
    if all_ok {
        println!(
            "\x1b[1;32m✓ All {} agent(s) passed\x1b[0m",
            manifest.agents.len()
        );
    } else {
        return Err("Some agents failed connectivity tests".to_string());
    }

    Ok(())
}
