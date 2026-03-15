//! Universal LLM Client using graniet/llm library.
//!
//! Provides a unified interface for 13+ LLM providers using the `provider::model` format.
//!
//! # Supported Providers
//!
//! - **OpenAI** - `openai::gpt-4o`, `openai::gpt-3.5-turbo`
//! - **Anthropic** - `anthropic::claude-3-opus-20240229`, `anthropic::claude-3-sonnet-20240229`
//! - **Ollama** - `ollama::llama3.2`, `ollama::gemma3:latest`, `ollama::mistral`
//! - **Google** - `google::gemini-pro`
//! - **DeepSeek** - `deepseek::deepseek-chat`
//! - **X.AI** - `xai::grok`
//! - **Groq** - `groq::llama3-70b-8192`
//! - **Mistral** - `mistral::mistral-large`
//! - **Cohere** - `cohere::command`
//! - **OpenRouter** - `openrouter::anthropic/claude-3-opus`

mod backend;
mod client;
mod config;
mod error;
mod types;

pub use client::UniversalLLMClient;
pub use config::UniversalConfig;
pub use error::{LlmError, Result};
pub use types::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, LlmUsage, MessageRole, ResponseStream,
    ToolDefinition,
};

#[cfg(test)]
mod tests;
