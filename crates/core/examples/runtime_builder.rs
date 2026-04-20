//! RuntimeBuilder example with a custom Rust tool and a mock LLM provider.
//!
//! This example is fully local and does not require a network LLM:
//!
//! ```powershell
//! cargo run -p enki-next --example runtime_builder
//! ```

use async_trait::async_trait;
use enki_next::agent::AgentDefinition;
use enki_next::llm::{
    ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult, ToolDefinition,
};
use enki_next::runtime::{RuntimeBuilder, RuntimeRequest};
use enki_next::tooling::types::{Tool, ToolContext, parse_tool_args};
use futures::stream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct EchoArgs {
    value: String,
}

struct EchoTool;

#[async_trait(?Send)]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo a string and include the current workspace path."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            },
            "required": ["value"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let params: EchoArgs = match parse_tool_args(args) {
            Ok(params) => params,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

        format!(
            "workspace={} value={}",
            ctx.workspace_dir.display(),
            params.value
        )
    }
}

#[derive(Clone)]
struct MockLlm {
    responses: Arc<Mutex<VecDeque<LlmResponse>>>,
}

impl MockLlm {
    fn new(responses: Vec<LlmResponse>) -> Self {
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
            "This example uses complete_with_tools only.".to_string(),
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

fn temp_home() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "enki-runtime-builder-example-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let llm = MockLlm::new(vec![
        LlmResponse {
            content: String::new(),
            usage: None,
            tool_calls: vec![
                json!({
                    "id": "call-1",
                    "function": {
                        "name": "echo",
                        "arguments": "{\"value\":\"hello from RuntimeBuilder\"}"
                    }
                })
                .to_string(),
            ],
            model: "mock".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        },
        LlmResponse {
            content: "The runtime successfully called the echo tool and finished the request."
                .to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "mock".to_string(),
            finish_reason: Some("stop".to_string()),
        },
    ]);

    let runtime = RuntimeBuilder::new(AgentDefinition {
        name: "Runtime Builder Demo".to_string(),
        system_prompt_preamble:
            "Use the echo tool once, then summarize the result in one sentence.".to_string(),
        model: "mock".to_string(),
        max_iterations: 4,
    })
    .with_llm(Box::new(llm))
    .with_workspace_home(temp_home())
    .register_tool(EchoTool)
    .build()
    .await?;

    let response = runtime
        .process(RuntimeRequest::new(
            "demo-session",
            "cli",
            "Show that the Rust runtime can call a custom tool.",
        ))
        .await?;

    println!("{}", response.content);

    Ok(())
}
