use crate::cli::MonitorArgs;
use crate::manifest::Manifest;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;

pub async fn run(args: MonitorArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;

    let manifest_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let workspace_home = manifest_dir
        .join(&manifest.workspace.home)
        .to_string_lossy()
        .to_string();

    // Build the runtime to populate the registry
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

    println!(
        "\x1b[1;36m⚡ Agent Monitor\x1b[0m — {}",
        manifest.project.name
    );
    println!();

    // Print header
    println!(
        "  {:<20} {:<24} {:<12} {}",
        "ID", "NAME", "STATUS", "CAPABILITIES"
    );
    println!("  {}", "─".repeat(72));

    let cards = runtime.registry().list_all().await;
    for card in &cards {
        let caps = card.capabilities.join(", ");
        println!(
            "  {:<20} {:<24} \x1b[32m{:<12}\x1b[0m {}",
            card.agent_id,
            card.name,
            format!("{:?}", card.status),
            caps
        );
    }

    println!();
    println!(
        "  \x1b[2mTotal:\x1b[0m {} agent(s) registered",
        cards.len()
    );
    println!();

    // Show model info from the manifest
    println!("  \x1b[1mModel Configuration:\x1b[0m");
    for agent_cfg in &manifest.agents {
        println!("    {} → {}", agent_cfg.id, agent_cfg.model);
    }
    println!();

    Ok(())
}
