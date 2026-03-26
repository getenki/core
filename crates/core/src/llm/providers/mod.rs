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

#[cfg(all(not(target_arch = "wasm32"), feature = "universal-llm-provider"))]
mod backend;
#[cfg(all(not(target_arch = "wasm32"), feature = "universal-llm-provider"))]
mod client;
mod config;
mod error;
mod types;

#[cfg(all(not(target_arch = "wasm32"), feature = "universal-llm-provider"))]
pub use client::UniversalLLMClient;
pub use config::UniversalConfig;
pub use error::{LlmError, Result};
pub use types::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, LlmUsage, MessageRole, ResponseStream,
    ToolDefinition,
};

#[cfg(target_arch = "wasm32")]
use async_trait::async_trait;
#[cfg(target_arch = "wasm32")]
use futures::stream;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "universal-llm-provider")))]
use async_trait::async_trait;
#[cfg(all(not(target_arch = "wasm32"), not(feature = "universal-llm-provider")))]
use futures::stream;

#[cfg(any(target_arch = "wasm32", not(feature = "universal-llm-provider")))]
pub struct UniversalLLMClient {
    config: UniversalConfig,
}

#[cfg(any(target_arch = "wasm32", not(feature = "universal-llm-provider")))]
impl UniversalLLMClient {
    pub fn new(provider_model: &str) -> Result<Self> {
        Ok(Self {
            config: UniversalConfig::new(provider_model),
        })
    }

    pub fn with_config(config: UniversalConfig) -> Result<Self> {
        Ok(Self { config })
    }

    pub fn with_api_key(provider_model: &str, api_key: impl Into<String>) -> Result<Self> {
        Ok(Self {
            config: UniversalConfig::new(provider_model).with_api_key(api_key),
        })
    }

    pub fn config(&self) -> &UniversalConfig {
        &self.config
    }

    pub fn provider(&self) -> Option<&str> {
        self.config.provider()
    }

    pub fn model_name(&self) -> &str {
        self.config.model_name()
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl LlmProvider for UniversalLLMClient {
    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> Result<LlmResponse> {
        Err(LlmError::Provider(format!(
            "UniversalLLMClient is unavailable on this target/build. Supply a custom LlmProvider or enable the `universal-llm-provider` feature for model `{}`.",
            self.config.model
        )))
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> Result<ResponseStream> {
        Ok(Box::pin(stream::empty()))
    }

    async fn complete_with_tools(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> Result<LlmResponse> {
        Err(LlmError::Provider(format!(
            "UniversalLLMClient is unavailable on this target/build. Supply a custom LlmProvider or enable the `universal-llm-provider` feature for model `{}`.",
            self.config.model
        )))
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn available_models(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "universal-llm-provider")))]
#[async_trait]
impl LlmProvider for UniversalLLMClient {
    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> Result<LlmResponse> {
        Err(LlmError::Provider(format!(
            "UniversalLLMClient is unavailable on this target/build. Supply a custom LlmProvider or enable the `universal-llm-provider` feature for model `{}`.",
            self.config.model
        )))
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> Result<ResponseStream> {
        Ok(Box::pin(stream::empty()))
    }

    async fn complete_with_tools(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> Result<LlmResponse> {
        Err(LlmError::Provider(format!(
            "UniversalLLMClient is unavailable on this target/build. Supply a custom LlmProvider or enable the `universal-llm-provider` feature for model `{}`.",
            self.config.model
        )))
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }

    fn available_models(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

#[cfg(all(test, feature = "universal-llm-provider", not(target_arch = "wasm32")))]
mod tests;
