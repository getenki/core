use super::backend::{get_api_key_env_var, parse_backend};
use super::config::UniversalConfig;
use llm::builder::LLMBackend;

#[test]
fn test_universal_config_parsing() {
    let config = UniversalConfig::new("ollama::gemma3:latest");
    assert_eq!(config.provider(), Some("ollama"));
    assert_eq!(config.model_name(), "gemma3:latest");
}

#[test]
fn test_universal_config_simple_model() {
    let config = UniversalConfig::new("openai::gpt-4o");
    assert_eq!(config.provider(), Some("openai"));
    assert_eq!(config.model_name(), "gpt-4o");
}

#[test]
fn test_universal_config_builder() {
    let config = UniversalConfig::new("anthropic::claude-3-sonnet-20240229")
        .with_api_key("test-key")
        .with_temperature(0.7)
        .with_max_tokens(2048)
        .with_resilience(3);

    assert_eq!(config.api_key, Some("test-key".to_string()));
    assert_eq!(config.temperature, Some(0.7));
    assert_eq!(config.max_tokens, Some(2048));
    assert_eq!(config.resilient, Some(true));
    assert_eq!(config.resilient_attempts, Some(3));
}

#[test]
fn test_parse_backend_valid() {
    assert!(matches!(parse_backend("ollama"), Ok(LLMBackend::Ollama)));
    assert!(matches!(parse_backend("openai"), Ok(LLMBackend::OpenAI)));
    assert!(matches!(
        parse_backend("anthropic"),
        Ok(LLMBackend::Anthropic)
    ));
    assert!(matches!(parse_backend("google"), Ok(LLMBackend::Google)));
}

#[test]
fn test_parse_backend_invalid() {
    assert!(parse_backend("unknown").is_err());
}

#[test]
fn test_api_key_env_vars() {
    assert_eq!(get_api_key_env_var("ollama"), None);
    assert_eq!(get_api_key_env_var("openai"), Some("OPENAI_API_KEY"));
    assert_eq!(get_api_key_env_var("anthropic"), Some("ANTHROPIC_API_KEY"));
    assert_eq!(get_api_key_env_var("google"), Some("GOOGLE_API_KEY"));
}
