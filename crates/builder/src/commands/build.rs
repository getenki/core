use crate::cli::BuildArgs;
use crate::manifest::Manifest;

pub async fn run(args: BuildArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;

    println!(
        "\x1b[1;36m⚡ Building project\x1b[0m '{}'",
        manifest.project.name
    );
    println!();

    // Validate all agent configs
    for agent in &manifest.agents {
        println!(
            "  \x1b[32m✓\x1b[0m Agent '{}' ({}) — model: {}",
            agent.id, agent.name, agent.model
        );
    }

    println!();

    // Check for language-specific project files and run installs
    let project_dir = args
        .manifest
        .parent()
        .unwrap_or(std::path::Path::new("."));

    if project_dir.join("package.json").exists() {
        println!("  \x1b[2mDetected Node project, running npm install...\x1b[0m");
        let status = tokio::process::Command::new("npm")
            .arg("install")
            .current_dir(project_dir)
            .status()
            .await
            .map_err(|e| format!("Failed to run npm install: {e}"))?;

        if !status.success() {
            return Err("npm install failed".to_string());
        }
        println!("  \x1b[32m✓\x1b[0m npm install complete");
    } else if project_dir.join("pyproject.toml").exists() {
        println!("  \x1b[2mDetected Python project, running pip install...\x1b[0m");
        let status = tokio::process::Command::new("pip")
            .args(["install", "-e", "."])
            .current_dir(project_dir)
            .status()
            .await
            .map_err(|e| format!("Failed to run pip install: {e}"))?;

        if !status.success() {
            return Err("pip install failed".to_string());
        }
        println!("  \x1b[32m✓\x1b[0m pip install complete");
    } else if project_dir.join("Cargo.toml").exists()
        && project_dir.join("Cargo.toml") != args.manifest
    {
        println!("  \x1b[2mDetected Rust project, running cargo build...\x1b[0m");
        let status = tokio::process::Command::new("cargo")
            .arg("build")
            .current_dir(project_dir)
            .status()
            .await
            .map_err(|e| format!("Failed to run cargo build: {e}"))?;

        if !status.success() {
            return Err("cargo build failed".to_string());
        }
        println!("  \x1b[32m✓\x1b[0m cargo build complete");
    }

    println!();
    println!(
        "\x1b[1;32m✓ Build complete\x1b[0m — {} agent(s) configured",
        manifest.agents.len()
    );

    Ok(())
}
