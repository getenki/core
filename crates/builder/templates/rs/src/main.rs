//! Enki Multi-Agent Starter — Rust
//!
//! Run with:   cargo run
//! Or via CLI: enki run --message "Hello!"

use core_next::llm::UniversalLLMClient;
use core_next::runtime::RuntimeBuilder;
use std::env;

#[tokio::main]
async fn main() {
    let model = env::var("ENKI_MODEL").unwrap_or_else(|_| "ollama::qwen3.5".to_string());

    println!("⚡ Enki Multi-Agent Runtime");
    println!();

    let llm = UniversalLLMClient::new(&model).expect("Failed to create LLM client");

    let runtime = RuntimeBuilder::for_default_agent()
        .with_llm(Box::new(llm))
        .with_workspace_home("./.enki")
        .build()
        .await
        .expect("Failed to build runtime");

    let message = env::args()
        .nth(1)
        .unwrap_or_else(|| "Hello! What can you help me with?".to_string());

    println!("> {message}");
    println!();

    let request = core_next::runtime::RuntimeRequest::new("session-1", "cli", &message);
    match runtime.process(request).await {
        Ok(response) => println!("{}", response.content),
        Err(e) => eprintln!("Error: {e}"),
    }
}
