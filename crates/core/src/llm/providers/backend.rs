use crate::llm::providers::error::{LlmError, Result};
use llm::builder::LLMBackend;

/// Get environment variable name for a provider's API key.
pub fn get_api_key_env_var(provider: &str) -> Option<&'static str> {
    match provider.to_lowercase().as_str() {
        "ollama" => None,
        "anthropic" | "claude" => Some("ANTHROPIC_API_KEY"),
        "openai" | "gpt" => Some("OPENAI_API_KEY"),
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "xai" | "x.ai" => Some("XAI_API_KEY"),
        "phind" => Some("PHIND_API_KEY"),
        "google" | "gemini" => Some("GOOGLE_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "azure" | "azureopenai" | "azure-openai" => Some("AZURE_OPENAI_API_KEY"),
        "elevenlabs" | "11labs" => Some("ELEVENLABS_API_KEY"),
        "cohere" => Some("COHERE_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        _ => None,
    }
}

/// Get API key from environment variable for the provider.
pub fn get_api_key_from_env(provider: &str) -> Result<Option<String>> {
    match get_api_key_env_var(provider) {
        None => Ok(None),
        Some(env_var) => match std::env::var(env_var) {
            Ok(key) => Ok(Some(key)),
            Err(_) => Err(LlmError::Config(format!(
                "API key required for provider '{}'. Please set {} environment variable.",
                provider, env_var
            ))),
        },
    }
}

/// Parse provider string to LLMBackend.
pub fn parse_backend(provider: &str) -> Result<LLMBackend> {
    match provider.to_lowercase().as_str() {
        "ollama" => Ok(LLMBackend::Ollama),
        "anthropic" | "claude" => Ok(LLMBackend::Anthropic),
        "openai" | "gpt" => Ok(LLMBackend::OpenAI),
        "deepseek" => Ok(LLMBackend::DeepSeek),
        "xai" | "x.ai" => Ok(LLMBackend::XAI),
        "phind" => Ok(LLMBackend::Phind),
        "google" | "gemini" => Ok(LLMBackend::Google),
        "groq" => Ok(LLMBackend::Groq),
        "azure" | "azureopenai" => Ok(LLMBackend::AzureOpenAI),
        "elevenlabs" | "11labs" => Ok(LLMBackend::ElevenLabs),
        "cohere" => Ok(LLMBackend::Cohere),
        "mistral" => Ok(LLMBackend::Mistral),
        "openrouter" => Ok(LLMBackend::OpenRouter),
        _ => Err(LlmError::Config(format!(
            "Unknown LLM provider: '{}'. Supported: ollama, openai, anthropic, google, deepseek, xai, groq, mistral, cohere, openrouter",
            provider
        ))),
    }
}
