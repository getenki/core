use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "enki",
    about = "Enki — multi-agent framework CLI",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new Enki project
    Init(InitArgs),
    /// Install project dependencies
    Build(BuildArgs),
    /// Run agents defined in enki.toml
    Run(RunArgs),
    /// Test agent connectivity and configuration
    Test(TestArgs),
    /// Display agent registry status
    Monitor(MonitorArgs),
    /// Interactive human-in-the-loop REPL
    Join(JoinArgs),
}

#[derive(Clone, ValueEnum)]
pub enum Template {
    Ts,
    Py,
    Rs,
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Template::Ts => write!(f, "ts"),
            Template::Py => write!(f, "py"),
            Template::Rs => write!(f, "rs"),
        }
    }
}

#[derive(Parser)]
pub struct InitArgs {
    /// Project name (also used as directory name)
    #[arg(long)]
    pub name: String,

    /// Language template to scaffold
    #[arg(long, default_value = "ts")]
    pub template: Template,
}

#[derive(Parser)]
pub struct BuildArgs {
    /// Path to enki.toml manifest
    #[arg(long, default_value = "./enki.toml")]
    pub manifest: PathBuf,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Path to enki.toml manifest
    #[arg(long, default_value = "./enki.toml")]
    pub manifest: PathBuf,

    /// Specific agent ID to run (runs all if omitted)
    #[arg(long)]
    pub agent: Option<String>,

    /// Message to send to the agent(s)
    #[arg(long)]
    pub message: String,
}

#[derive(Parser)]
pub struct TestArgs {
    /// Path to enki.toml manifest
    #[arg(long, default_value = "./enki.toml")]
    pub manifest: PathBuf,
}

#[derive(Parser)]
pub struct MonitorArgs {
    /// Path to enki.toml manifest
    #[arg(long, default_value = "./enki.toml")]
    pub manifest: PathBuf,
}

#[derive(Parser)]
pub struct JoinArgs {
    /// Path to enki.toml manifest
    #[arg(long, default_value = "./enki.toml")]
    pub manifest: PathBuf,

    /// Agent ID to interact with (defaults to the first agent)
    #[arg(long)]
    pub agent: Option<String>,
}
