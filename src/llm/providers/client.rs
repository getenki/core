use crate::llm::providers::backend::{get_api_key_from_env, parse_backend};
use crate::llm::providers::config::UniversalConfig;
use crate::llm::providers::error::{LlmError, Result};
use crate::llm::providers::types::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, MessageRole, ResponseStream, ToolDefinition,
};
use async_trait::async_trait;
use llm::LLMProvider as BackendProvider;
use llm::builder::LLMBuilder;
use llm::chat::ChatMessage as LlmChatMessage;
use std::sync::Arc;
use tracing::debug;

/// Universal LLM Client wrapping graniet/llm.
///
/// Provides a unified interface for multiple LLM backends using the `provider::model` format.
pub struct UniversalLLMClient {
    config: UniversalConfig,
    llm: Arc<Box<dyn BackendProvider>>,
}

impl UniversalLLMClient {
    /// Create a new client from "provider::model" format.
    pub fn new(provider_model: &str) -> Result<Self> {
        Self::with_config(UniversalConfig::new(provider_model))
    }

    /// Create a new client with explicit API key.
    pub fn with_api_key(provider_model: &str, api_key: impl Into<String>) -> Result<Self> {
        Self::with_config(UniversalConfig::new(provider_model).with_api_key(api_key))
    }

    /// Create a new client with full configuration.
    pub fn with_config(config: UniversalConfig) -> Result<Self> {
        let parts: Vec<&str> = config.model.split("::").collect();

        if parts.len() < 2 {
            return Err(LlmError::Config(format!(
                "Invalid model format. Use 'provider::model-name'. Got: {}",
                config.model
            )));
        }

        let provider = parts[0];
        let model = parts[1..].join(":");
        let api_key = match &config.api_key {
            Some(key) => Some(key.clone()),
            None => get_api_key_from_env(provider)?,
        };
        let backend = parse_backend(provider)?;
        let base_url = config.base_url.clone().or_else(|| {
            if provider.eq_ignore_ascii_case("ollama") {
                Some(
                    std::env::var("OLLAMA_URL")
                        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
                )
            } else {
                None
            }
        });

        let mut builder = LLMBuilder::new().backend(backend).model(&model);

        if let Some(ref key) = api_key {
            builder = builder.api_key(key);
        }
        if let Some(ref url) = base_url {
            builder = builder.base_url(url);
        }
        if let Some(max_tokens) = config.max_tokens {
            builder = builder.max_tokens(max_tokens);
        }
        if let Some(temp) = config.temperature {
            builder = builder.temperature(temp);
        }
        if let Some(top_p) = config.top_p {
            builder = builder.top_p(top_p);
        }
        if let Some(top_k) = config.top_k {
            builder = builder.top_k(top_k);
        }
        if let Some(ref system) = config.system {
            builder = builder.system(system);
        }
        if let Some(timeout) = config.timeout_seconds {
            builder = builder.timeout_seconds(timeout);
        }
        if config.resilient == Some(true) {
            builder = builder.resilient(true);
            if let Some(attempts) = config.resilient_attempts {
                builder = builder.resilient_attempts(attempts);
            }
        }

        let llm = builder
            .build()
            .map_err(|e| LlmError::Config(format!("Failed to build LLM: {}", e)))?;

        debug!(
            provider = provider,
            model = model,
            "Created UniversalLLMClient"
        );

        Ok(Self {
            config,
            llm: Arc::new(llm),
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

    fn convert_messages(messages: &[ChatMessage]) -> Vec<LlmChatMessage> {
        messages
            .iter()
            .map(|msg| match msg.role {
                MessageRole::System => LlmChatMessage::user().content(&msg.content).build(),
                MessageRole::User => LlmChatMessage::user().content(&msg.content).build(),
                MessageRole::Assistant => LlmChatMessage::assistant().content(&msg.content).build(),
                MessageRole::Tool => {
                    let content = match &msg.tool_call_id {
                        Some(tool_call_id) => {
                            format!(
                                "Tool result (tool_call_id={}): {}",
                                tool_call_id, msg.content
                            )
                        }
                        None => format!("Tool result: {}", msg.content),
                    };
                    LlmChatMessage::user().content(content).build()
                }
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for UniversalLLMClient {
    async fn complete(&self, messages: &[ChatMessage], _config: &LlmConfig) -> Result<LlmResponse> {
        let chat_messages = Self::convert_messages(messages);
        let response = self
            .llm
            .chat(&chat_messages)
            .await
            .map_err(|e| LlmError::Provider(format!("LLM error: {}", e)))?;
        let content = response.text().unwrap_or_default();

        Ok(LlmResponse {
            content,
            usage: None,
            tool_calls: Vec::new(),
            model: self.config.model.clone(),
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> Result<ResponseStream> {
        Err(LlmError::Provider(
            "Streaming not yet implemented for UniversalLLMClient".to_string(),
        ))
    }

    async fn complete_with_tools(
        &self,
        messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<LlmResponse> {
        self.complete(messages, config).await
    }

    fn name(&self) -> &'static str {
        match self.provider() {
            Some("ollama") => "ollama",
            Some("openai") => "openai",
            Some("anthropic") => "anthropic",
            Some("google") => "google",
            Some("deepseek") => "deepseek",
            Some("xai") => "xai",
            Some("groq") => "groq",
            Some("mistral") => "mistral",
            _ => "universal",
        }
    }

    fn available_models(&self) -> Vec<&'static str> {
        match self.provider() {
            Some("ollama") => vec![
                "llama3",
                "llama3.2",
                "gemma3",
                "mistral",
                "codellama",
                "phi3",
            ],
            Some("openai") => vec!["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"],
            Some("anthropic") => vec![
                "claude-3-opus-20240229",
                "claude-3-sonnet-20240229",
                "claude-3-haiku-20240307",
            ],
            Some("google") => vec!["gemini-pro", "gemini-1.5-pro"],
            _ => vec![],
        }
    }
}
