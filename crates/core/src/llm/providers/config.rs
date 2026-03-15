use serde::{Deserialize, Serialize};

/// Configuration for Universal LLM Client.
///
/// Extended configuration supporting all graniet/llm options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalConfig {
    /// Model in "provider::model" format
    pub model: String,
    /// Optional API key (auto-detected from env if not provided)
    pub api_key: Option<String>,
    /// Optional base URL override
    pub base_url: Option<String>,
    /// Max tokens to generate
    pub max_tokens: Option<u32>,
    /// Sampling temperature
    pub temperature: Option<f32>,
    /// Top-p sampling
    pub top_p: Option<f32>,
    /// Top-k sampling
    pub top_k: Option<u32>,
    /// System prompt
    pub system: Option<String>,
    /// Timeout in seconds
    pub timeout_seconds: Option<u64>,
    /// Enable resilient mode with retries
    pub resilient: Option<bool>,
    /// Number of retry attempts
    pub resilient_attempts: Option<usize>,
}

impl Default for UniversalConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            api_key: None,
            base_url: None,
            max_tokens: Some(4096),
            temperature: None,
            top_p: None,
            top_k: None,
            system: None,
            timeout_seconds: None,
            resilient: None,
            resilient_attempts: None,
        }
    }
}

impl UniversalConfig {
    /// Create new config with model in "provider::model" format.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set API key.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature.
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set system prompt.
    pub fn with_system(mut self, prompt: impl Into<String>) -> Self {
        self.system = Some(prompt.into());
        self
    }

    /// Enable resilience with retry count.
    pub fn with_resilience(mut self, attempts: usize) -> Self {
        self.resilient = Some(true);
        self.resilient_attempts = Some(attempts);
        self
    }

    /// Parse provider name from model string.
    pub fn provider(&self) -> Option<&str> {
        self.model.split("::").next()
    }

    /// Parse model name from model string.
    pub fn model_name(&self) -> &str {
        self.model.split("::").nth(1).unwrap_or(&self.model)
    }
}
