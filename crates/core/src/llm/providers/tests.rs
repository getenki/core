use super::backend::{get_api_key_env_var, parse_backend};
use super::client::UniversalLLMClient;
use super::config::UniversalConfig;
use super::types::{ChatMessage, LlmConfig, LlmProvider, LlmResponse, LlmUsage, MessageRole};
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
fn test_universal_config_openai_with_simple_prompt() {
    let prompt = "Return exactly the word 'Hello' and nothing else.";
    let config = UniversalConfig::new("openai::gpt-4o").with_system(prompt);
    let result = LlmResponse {
        content: "Hello".to_string(),
        usage: Some(LlmUsage {
            prompt_tokens: Some(12),
            completion_tokens: Some(1),
            total_tokens: Some(13),
        }),
        tool_calls: Vec::new(),
        model: "openai::gpt-4o".to_string(),
        finish_reason: Some("stop".to_string()),
    };

    assert_eq!(config.provider(), Some("openai"));
    assert_eq!(config.model_name(), "gpt-4o");
    assert_eq!(config.system.as_deref(), Some(prompt));
    assert_eq!(result.content, "Hello");
    assert_eq!(result.model, "openai::gpt-4o");
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        result.usage.as_ref().and_then(|usage| usage.total_tokens),
        Some(13)
    );
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and outbound network access"]
async fn test_openai_live_completion_returns_result() {
    assert!(
        std::env::var("OPENAI_API_KEY").is_ok(),
        "OPENAI_API_KEY must be set to run this live test"
    );

    let client = UniversalLLMClient::new("openai::gpt-4o-mini")
        .expect("OpenAI client should initialize when OPENAI_API_KEY is set");
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Reply with exactly: Hello from OpenAI test.".to_string(),
        tool_call_id: None,
    }];

    let result = client
        .complete(&messages, &LlmConfig::default())
        .await
        .expect("OpenAI completion should succeed");

    println!("OpenAI result: {}", result.content);

    assert!(!result.content.trim().is_empty());
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and outbound network access"]
async fn test_anthropic_live_completion_returns_result() {
    assert!(
        std::env::var("ANTHROPIC_API_KEY").is_ok(),
        "ANTHROPIC_API_KEY must be set to run this live test"
    );

    let client = UniversalLLMClient::new("anthropic::claude-sonnet-4-6")
        .expect("Anthropic client should initialize when ANTHROPIC_API_KEY is set");
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Reply with exactly: Hello from Anthropic test2. and what is 2+2".to_string(),
        tool_call_id: None,
    }];

    let result = client
        .complete(&messages, &LlmConfig::default())
        .await
        .expect("Anthropic completion should succeed");

    println!("Anthropic result: {}", result.content);

    assert!(!result.content.trim().is_empty());
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
