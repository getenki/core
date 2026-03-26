#[cfg(feature = "universal-llm-provider")]
use core_next::llm::UniversalLLMClient;
use core_next::runtime::{CliChannel, RuntimeBuilder};
use std::env;

#[cfg(feature = "universal-llm-provider")]
#[tokio::main]
async fn main() {
    let runtime = match RuntimeBuilder::for_default_agent()
        .with_llm(Box::new(match UniversalLLMClient::new("ollama::qwen3.5") {
            Ok(llm) => llm,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }))
        .with_workspace_home("./ps")
        .build()
        .await
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut channel = match CliChannel::from_args(env::args().collect()) {
        Ok(channel) => channel,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = runtime.serve_channel(&mut channel).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "universal-llm-provider"))]
fn main() {
    eprintln!(
        "The built-in universal LLM provider is disabled. Rebuild with the `universal-llm-provider` feature or inject a custom LlmProvider."
    );
    std::process::exit(1);
}
