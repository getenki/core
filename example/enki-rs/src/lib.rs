use async_trait::async_trait;
use enki_next::agent::ExecutionStep;
use enki_next::llm::{
    ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult,
    ToolDefinition,
};
use futures::stream;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct MockLlm {
    responses: Arc<Mutex<VecDeque<LlmResponse>>>,
}

impl MockLlm {
    pub fn new(responses: Vec<LlmResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

#[async_trait]
impl LlmProvider for MockLlm {
    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> LlmResult<LlmResponse> {
        Err(LlmError::Provider(
            "MockLlm only supports complete_with_tools in these examples.".to_string(),
        ))
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> LlmResult<enki_next::llm::ResponseStream> {
        Ok(Box::pin(stream::empty()))
    }

    async fn complete_with_tools(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> LlmResult<LlmResponse> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| LlmError::Provider("missing mock response".to_string()))
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn available_models(&self) -> Vec<&'static str> {
        vec!["mock"]
    }
}

pub fn mock_tool_call(
    call_id: &str,
    tool_name: &str,
    arguments: Value,
    finish_reason: &str,
) -> LlmResponse {
    LlmResponse {
        content: String::new(),
        usage: None,
        tool_calls: vec![
            json!({
                "id": call_id,
                "function": {
                    "name": tool_name,
                    "arguments": arguments.to_string(),
                }
            })
            .to_string(),
        ],
        model: "mock".to_string(),
        finish_reason: Some(finish_reason.to_string()),
    }
}

pub fn mock_text(content: impl Into<String>) -> LlmResponse {
    LlmResponse {
        content: content.into(),
        usage: None,
        tool_calls: Vec::new(),
        model: "mock".to_string(),
        finish_reason: Some("stop".to_string()),
    }
}

pub fn temp_workspace(label: &str) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!(
        "enki-rs-example-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .map_err(|error| error.to_string())?
    ));
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, content).map_err(|error| error.to_string())
}

pub fn print_steps(label: &str, steps: &[ExecutionStep]) {
    println!("{label}");
    for step in steps {
        println!(
            "  {}. [{}] {} -> {}",
            step.index, step.phase, step.kind, step.detail
        );
    }
    if steps.is_empty() {
        println!("  (no intermediate execution steps)");
    }
}
