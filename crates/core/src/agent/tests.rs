
use crate::agent::AgentDefinition;
use crate::agent::core::Agent;
use crate::llm::{
    ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult, ToolDefinition,
};
use crate::tooling::types::{Tool, ToolContext, ToolRegistryBuilder, parse_tool_args};
use async_trait::async_trait;
use futures::stream;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct RecordingLlm {
    responses: Arc<Mutex<VecDeque<LlmResponse>>>,
    calls: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    tool_calls: Arc<Mutex<Vec<Vec<ToolDefinition>>>>,
}

impl RecordingLlm {
    fn new(responses: Vec<LlmResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            calls: Arc::new(Mutex::new(Vec::new())),
            tool_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<Vec<ChatMessage>> {
        self.calls.lock().unwrap().clone()
    }

    fn requested_tools(&self) -> Vec<Vec<ToolDefinition>> {
        self.tool_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl LlmProvider for RecordingLlm {
    async fn complete(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> LlmResult<LlmResponse> {
        Err(LlmError::Provider("not used".to_string()))
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> LlmResult<crate::llm::ResponseStream> {
        Ok(Box::pin(stream::empty()))
    }

    async fn complete_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> LlmResult<LlmResponse> {
        self.calls.lock().unwrap().push(messages.to_vec());
        self.tool_calls.lock().unwrap().push(tools.to_vec());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| LlmError::Provider("missing response".to_string()))
    }

    fn name(&self) -> &'static str {
        "recording"
    }

    fn available_models(&self) -> Vec<&'static str> {
        vec!["recording"]
    }
}

fn temp_home(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "core-next-agent-tests-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[derive(Deserialize)]
struct EchoParams {
    value: String,
}

struct EchoTool;

#[async_trait(?Send)]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo a value"
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

    async fn execute(&self, args: &Value, _ctx: &ToolContext) -> String {
        let params: EchoParams = match parse_tool_args(args) {
            Ok(params) => params,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

        format!("echo:{}", params.value)
    }
}

#[test]
fn extracts_tool_call_from_mixed_content() {
    let assistant_message = json!({
        "role": "assistant",
        "content": "I'll save the note for you.\n\n{\"tool\":\"write_file\",\"args\":{\"path\":\"note.md\",\"content\":\"hello\"}}\n\nDone."
    });

    let tool_call =
        Agent::extract_embedded_tool_call(assistant_message["content"].as_str().unwrap());

    assert_eq!(
        tool_call,
        Some((
            "write_file".to_string(),
            json!({
                "path": "note.md",
                "content": "hello"
            })
        ))
    );
}

#[test]
fn ignores_non_tool_json_objects() {
    let content = "Summary: {\"ok\":true}\n{\"tool\":\"exec\",\"args\":{\"cmd\":\"pwd\"}}";

    let tool_call = Agent::extract_embedded_tool_call(content);

    assert_eq!(
        tool_call,
        Some((
            "exec".to_string(),
            json!({
                "cmd": "pwd"
            })
        ))
    );
}

#[test]
fn repairs_tool_call_with_missing_closing_brace() {
    // Small LLMs sometimes drop the final `}` after long content strings
    let content = r##"I'll write the note.

{"tool": "write_file", "args": {"path": "note.md", "content": "# Hello World"}"##;

    let tool_call = Agent::extract_embedded_tool_call(content);

    assert_eq!(
        tool_call,
        Some((
            "write_file".to_string(),
            json!({
                "path": "note.md",
                "content": "# Hello World"
            })
        ))
    );
}

#[test]
fn extracts_tool_call_from_code_fence() {
    let content = "Here is the tool call:\n\n```json\n{\"tool\": \"write_file\", \"args\": {\"path\": \"note.md\", \"content\": \"hello\"}}\n```\n\nDone.";

    let tool_call = Agent::extract_embedded_tool_call(content);

    assert_eq!(
        tool_call,
        Some((
            "write_file".to_string(),
            json!({
                "path": "note.md",
                "content": "hello"
            })
        ))
    );
}

#[tokio::test]
async fn reloads_previous_session_messages_before_next_request() {
    let home = temp_home("resume");
    let llm = RecordingLlm::new(vec![
        LlmResponse {
            content: "First answer".to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "recording".to_string(),
            finish_reason: Some("stop".to_string()),
        },
        LlmResponse {
            content: "Second answer".to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "recording".to_string(),
            finish_reason: Some("stop".to_string()),
        },
    ]);

    let agent = Agent::with_definition_executor_llm_and_workspace(
        AgentDefinition::default(),
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm.clone())),
        None,
        Some(home.clone()),
    )
    .await
    .unwrap();

    assert_eq!(agent.run("session-a", "hello").await, "First answer");
    assert_eq!(agent.run("session-a", "follow up").await, "Second answer");

    let calls = llm.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].len(), 4);
    assert_eq!(calls[1][1].content, "hello");
    assert_eq!(calls[1][2].content, "First answer");
    assert_eq!(calls[1][3].content, "follow up");
}

#[tokio::test]
async fn persists_terminal_error_to_session_transcript() {
    let home = temp_home("error");
    let llm = RecordingLlm::new(Vec::new());

    let agent = Agent::with_definition_executor_llm_and_workspace(
        AgentDefinition::default(),
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm)),
        None,
        Some(home.clone()),
    )
    .await
    .unwrap();

    let result = agent.run("session-a", "hello").await;
    assert_eq!(result, "LLM error: Provider error: missing response");

    let session_file = home
        .join(".atomiagent")
        .join("agents")
        .join("personal-assistant")
        .join("sessions")
        .join("session-a.json");
    let raw = std::fs::read_to_string(session_file).unwrap();
    let transcript: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();

    let last = transcript.last().unwrap();
    assert_eq!(
        last["payload"]["content"].as_str().unwrap(),
        "LLM error: Provider error: missing response"
    );
}

#[tokio::test]
async fn exposes_builtin_tools_by_default() {
    let home = temp_home("builtin-tools-default");
    let llm = RecordingLlm::new(vec![LlmResponse {
        content: "Builtin tools enabled".to_string(),
        usage: None,
        tool_calls: Vec::new(),
        model: "recording".to_string(),
        finish_reason: Some("stop".to_string()),
    }]);

    let agent = Agent::with_definition_executor_llm_and_workspace(
        AgentDefinition::default(),
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm.clone())),
        None,
        Some(home),
    )
    .await
    .unwrap();

    assert_eq!(
        agent.run("session-a", "hello").await,
        "Builtin tools enabled"
    );

    let requested_tools = llm.requested_tools();
    assert_eq!(requested_tools.len(), 1);
    let tool_names = requested_tools[0]
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(tool_names, vec!["exec", "read_file", "write_file"]);
}

#[tokio::test]
async fn custom_tool_registry_is_merged_with_builtin_tools() {
    let home = temp_home("builtin-tools-merge");
    let llm = RecordingLlm::new(vec![LlmResponse {
        content: "Merged tools enabled".to_string(),
        usage: None,
        tool_calls: Vec::new(),
        model: "recording".to_string(),
        finish_reason: Some("stop".to_string()),
    }]);

    let tool_registry = ToolRegistryBuilder::new().register(EchoTool).build();

    let agent = Agent::with_definition_tool_registry_executor_llm_and_workspace(
        AgentDefinition::default(),
        tool_registry,
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm.clone())),
        None,
        Some(home),
    )
    .await
    .unwrap();

    assert_eq!(
        agent.run("session-a", "hello").await,
        "Merged tools enabled"
    );

    let requested_tools = llm.requested_tools();
    assert_eq!(requested_tools.len(), 1);
    let tool_names = requested_tools[0]
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(tool_names, vec!["echo", "exec", "read_file", "write_file"]);
}

#[tokio::test]
async fn executes_function_tools_from_native_stringified_arguments() {
    let home = temp_home("function-tool");
    let llm = RecordingLlm::new(vec![
        LlmResponse {
            content: String::new(),
            usage: None,
            tool_calls: vec![
                json!({
                    "id": "call-1",
                    "function": {
                        "name": "echo",
                        "arguments": "{\"value\":\"hello\"}"
                    }
                })
                .to_string(),
            ],
            model: "recording".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        },
        LlmResponse {
            content: "done".to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "recording".to_string(),
            finish_reason: Some("stop".to_string()),
        },
    ]);

    let tool_registry = ToolRegistryBuilder::new().register(EchoTool).build();

    let agent = Agent::with_definition_tool_registry_executor_llm_and_workspace(
        AgentDefinition::default(),
        tool_registry,
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm.clone())),
        None,
        Some(home),
    )
    .await
    .unwrap();

    assert_eq!(agent.run("session-a", "hello").await, "done");

    let calls = llm.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1][3].content, "echo:hello");
    assert_eq!(calls[1][3].tool_call_id.as_deref(), Some("call-1"));
}

#[tokio::test]
async fn detailed_run_traces_tool_call_minor_steps() {
    let home = temp_home("trace-tool-steps");
    let llm = RecordingLlm::new(vec![
        LlmResponse {
            content: String::new(),
            usage: None,
            tool_calls: vec![
                json!({
                    "id": "call-1",
                    "function": {
                        "name": "echo",
                        "arguments": "{\"value\":\"hello\"}"
                    }
                })
                .to_string(),
            ],
            model: "recording".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        },
        LlmResponse {
            content: "done".to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "recording".to_string(),
            finish_reason: Some("stop".to_string()),
        },
    ]);

    let tool_registry = ToolRegistryBuilder::new().register(EchoTool).build();

    let agent = Agent::with_definition_tool_registry_executor_llm_and_workspace(
        AgentDefinition::default(),
        tool_registry,
        Box::new(crate::tooling::tool_calling::RegistryToolExecutor),
        Some(Box::new(llm)),
        None,
        Some(home),
    )
    .await
    .unwrap();

    let result = agent.run_detailed("session-a", "hello", None).await;

    assert_eq!(result.content, "done");
    assert!(result.steps.iter().any(|step| {
        step.kind == "tool_call"
            && step.detail.contains("echo")
            && step.detail.contains("{\"value\":\"hello\"}")
    }));
    assert!(result.steps.iter().any(|step| {
        step.kind == "tool_result"
            && step.detail.contains("echo")
            && step.detail.contains("echo:hello")
    }));
}
