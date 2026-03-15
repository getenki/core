/// Error type for LLM operations in this module.
#[derive(Debug, Clone)]
pub enum LlmError {
    Config(String),
    Provider(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(msg) => write!(f, "Config error: {}", msg),
            Self::Provider(msg) => write!(f, "Provider error: {}", msg),
        }
    }
}

impl std::error::Error for LlmError {}

/// Convenience result type for this module.
pub type Result<T> = std::result::Result<T, LlmError>;
