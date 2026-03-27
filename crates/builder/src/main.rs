mod cli;
mod commands;
mod manifest;
mod project_runtime;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    print_logo();

    let cli = Cli::parse();

    let result = match cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Build(args) => commands::build::run(args).await,
        Command::Run(args) => commands::run::run(args).await,
        Command::Test(args) => commands::test::run(args).await,
        Command::Monitor(args) => commands::monitor::run(args).await,
        Command::Join(args) => commands::join::run(args).await,
    };

    if let Err(e) = result {
        eprintln!("\x1b[1;31merror:\x1b[0m {e}");
        std::process::exit(1);
    }
}

fn print_logo() {
    println!("\x1b[1;36m");
    println!(r#"  ███████╗███╗   ██╗██╗  ██╗██╗"#);
    println!(r#"  ██╔════╝████╗  ██║██║ ██╔╝██║"#);
    println!(r#"  █████╗  ██╔██╗ ██║█████╔╝ ██║"#);
    println!(r#"  ██╔══╝  ██║╚██╗██║██╔═██╗ ██║"#);
    println!(r#"  ███████╗██║ ╚████║██║  ██╗██║"#);
    println!(r#"  ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═╝╚═╝"#);
    println!("\x1b[0m");
}
