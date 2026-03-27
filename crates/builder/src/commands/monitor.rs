use crate::cli::MonitorArgs;
use crate::manifest::Manifest;

pub async fn run(args: MonitorArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;

    println!(
        "\x1b[1;36mAgent Monitor\x1b[0m - {}",
        manifest.project.name
    );
    println!();
    println!(
        "  {:<20} {:<24} {:<12} {}",
        "ID", "NAME", "STATUS", "CAPABILITIES"
    );
    println!("  {}", "-".repeat(72));

    for agent_cfg in &manifest.agents {
        println!(
            "  {:<20} {:<24} \x1b[32m{:<12}\x1b[0m {}",
            agent_cfg.id,
            agent_cfg.name,
            "configured",
            agent_cfg.capabilities.join(", ")
        );
    }

    println!();
    println!(
        "  \x1b[2mTotal:\x1b[0m {} agent(s) configured",
        manifest.agents.len()
    );
    println!();
    println!("  \x1b[1mModel Configuration:\x1b[0m");
    for agent_cfg in &manifest.agents {
        println!("    {} -> {}", agent_cfg.id, agent_cfg.model);
    }
    println!();

    Ok(())
}
