mod workspace;

use crate::agent::workspace::AgentWorkspace;
use crate::llm::{
    ChatMessage, LlmConfig, LlmProvider, MessageRole, ToolDefinition, UniversalLLMClient,
};
use crate::memory::MemoryManager;
use crate::message::IndexedValue;
use crate::message::{Message, next_request_id};
use crate::tooling::builtin_tools;
use crate::tooling::tool_calling::{RegistryToolExecutor, ToolCallRegistry, ToolExecutor};
use crate::tooling::types::{ToolContext, ToolRegistry};
use serde_json::Value;
use std::path::PathBuf;

// Config
const DEFAULT_MAX_ITERATIONS: usize = 20;

pub struct AgentDefinition {
    pub name: String,
    pub system_prompt_preamble: String,
    pub model: String,
    pub max_iterations: usize,
}

impl Default for AgentDefinition {
    fn default() -> Self {
        Self {
            name: "Personal Assistant".to_string(),
            system_prompt_preamble: "You are a helpful Personal Assistant agent.".to_string(),
            model: String::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }
}

pub struct Agent {
    definition: AgentDefinition,
    tool_registry: ToolCallRegistry,
    tool_executor: Box<dyn ToolExecutor>,
    workspace: AgentWorkspace,
    llm: Box<dyn LlmProvider>,
    memory: MemoryManager,
    #[cfg(target_arch = "wasm32")]
    sessions: Mutex<HashMap<String, Vec<Message>>>,
}

enum StepOutcome {
    Continue,
    Final(String),
}

struct ToolInvocation {
    name: String,
    args: Value,
    call_id: Option<String>,
}

impl Agent {
    fn with_builtin_tools(mut tool_registry: ToolRegistry) -> ToolRegistry {
        let mut builtin_registry = builtin_tools::default_registry();
        builtin_registry.append(&mut tool_registry);
        builtin_registry
    }

    fn resolve_model(definition: &AgentDefinition) -> Result<String, String> {
        if !definition.model.trim().is_empty() {
            return Ok(definition.model.clone());
        }

        std::env::var("ENKI_MODEL")
            .map_err(|_| "Missing model. Set AgentDefinition.model or ENKI_MODEL.".to_string())
    }

    pub async fn new() -> Result<Self, String> {
        Self::with_definition(AgentDefinition::default()).await
    }

    pub async fn with_definition(definition: AgentDefinition) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            ToolRegistry::new(),
            Box::new(RegistryToolExecutor),
            None,
            None,
            None,
        )
        .await
    }

    pub async fn with_definition_and_executor(
        definition: AgentDefinition,
        tool_executor: Box<dyn ToolExecutor>,
    ) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            ToolRegistry::new(),
            tool_executor,
            None,
            None,
            None,
        )
        .await
    }

    pub async fn with_definition_and_tool_registry(
        definition: AgentDefinition,
        tool_registry: ToolRegistry,
    ) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            tool_registry,
            Box::new(RegistryToolExecutor),
            None,
            None,
            None,
        )
        .await
    }

    pub async fn with_definition_executor_and_workspace(
        definition: AgentDefinition,
        tool_executor: Box<dyn ToolExecutor>,
        workspace_home: Option<PathBuf>,
    ) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            ToolRegistry::new(),
            tool_executor,
            None,
            None,
            workspace_home,
        )
        .await
    }

    pub async fn with_definition_tool_registry_executor_and_workspace(
        definition: AgentDefinition,
        tool_registry: ToolRegistry,
        tool_executor: Box<dyn ToolExecutor>,
        workspace_home: Option<PathBuf>,
    ) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            tool_registry,
            tool_executor,
            None,
            None,
            workspace_home,
        )
        .await
    }

    pub async fn with_definition_executor_llm_and_workspace(
        definition: AgentDefinition,
        tool_executor: Box<dyn ToolExecutor>,
        llm: Option<Box<dyn LlmProvider>>,
        memory: Option<MemoryManager>,
        workspace_home: Option<PathBuf>,
    ) -> Result<Self, String> {
        Self::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            ToolRegistry::new(),
            tool_executor,
            llm,
            memory,
            workspace_home,
        )
        .await
    }

    pub async fn with_definition_tool_registry_executor_llm_and_workspace(
        definition: AgentDefinition,
        tool_registry: ToolRegistry,
        tool_executor: Box<dyn ToolExecutor>,
        llm: Option<Box<dyn LlmProvider>>,
        memory: Option<MemoryManager>,
        workspace_home: Option<PathBuf>,
    ) -> Result<Self, String> {
        let workspace = AgentWorkspace::new(&definition.name, workspace_home);
        workspace.ensure_dirs().await?;
        let tool_registry = Self::with_builtin_tools(tool_registry);
        let llm = match llm {
            Some(llm) => llm,
            None => {
                let model = Self::resolve_model(&definition)?;
                Box::new(UniversalLLMClient::new(&model).map_err(|e| e.to_string())?)
            }
        };

        Ok(Self {
            llm,
            memory: memory
                .unwrap_or_else(|| MemoryManager::with_defaults(workspace.memory_dir.clone())),
            definition,
            tool_registry: ToolCallRegistry::new(tool_registry),
            tool_executor,
            workspace,
            #[cfg(target_arch = "wasm32")]
            sessions: Mutex::new(HashMap::new()),
        })
    }

    fn system_prompt(&self, ctx: &ToolContext, memory_context: &str) -> String {
        let mut prompt = format!(
            r#"You are {}.
{} Use tools via JSON calls when needed.
- Process each incoming user message as a loop:
  1. Receive the message.
  2. Interpret it.
  3. Choose the next action.
  4. Either reply immediately, call a tool, or ask a follow-up question.
  5. If you call a tool, read the result and continue the loop.
  6. Stop only when a final reply is ready.
- One user message may require multiple internal iterations before the final answer.
- If a tool is needed, prefer native tool calls. If native tool calling is unavailable, respond with ONLY {{"tool": "tool_name", "args": {{...}}}}.
- When done, respond with plain text.
Available tools: {}
Agent workspace: {}
Current task workspace: {}
- When using write_file or read_file, use simple relative paths (e.g. "note.md", "output/data.csv"). Paths are resolved relative to the current task workspace automatically. Do NOT construct full workspace paths manually."#,
            self.definition.name,
            self.definition.system_prompt_preamble,
            self.tool_registry.catalog_json(),
            ctx.agent_dir.display(),
            ctx.workspace_dir.display()
        );

        if !memory_context.trim().is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(memory_context);
        }

        prompt
    }

    fn to_llm_messages(&self, messages: &[Message]) -> Vec<ChatMessage> {
        messages
            .iter()
            .filter_map(|message| {
                let value = Value::from(message);
                let role = match value.get("role").and_then(Value::as_str)? {
                    "system" => MessageRole::System,
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    _ => return None,
                };

                Some(ChatMessage {
                    role,
                    content: value
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    tool_call_id: value
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect()
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_registry
            .catalog_json()
            .as_object()
            .map(|tools| {
                tools
                    .iter()
                    .map(|(name, entry)| ToolDefinition {
                        name: name.clone(),
                        description: entry
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn decode_tool_calls(&self, tool_calls: Vec<String>) -> Vec<Value> {
        tool_calls
            .into_iter()
            .filter_map(|tool_call| serde_json::from_str::<Value>(&tool_call).ok())
            .collect()
    }

    async fn call_llm(&self, messages: &[Message]) -> Result<Value, String> {
        let tool_definitions = self.tool_definitions();
        let response = self
            .llm
            .complete_with_tools(
                &self.to_llm_messages(messages),
                &tool_definitions,
                &LlmConfig::default(),
            )
            .await
            .map_err(|e| e.to_string())?;

        let mut assistant_message = serde_json::json!({
            "role": "assistant",
            "content": response.content,
        });

        let tool_calls = self.decode_tool_calls(response.tool_calls);
        if !tool_calls.is_empty() {
            assistant_message["tool_calls"] = Value::Array(tool_calls);
        }

        Ok(assistant_message)
    }

    fn parse_content_tool_call(&self, assistant_message: &Value) -> Option<(String, Value)> {
        let content = assistant_message.get("content")?.as_str()?;
        Self::extract_embedded_tool_call(content)
    }

    fn extract_embedded_tool_call(content: &str) -> Option<(String, Value)> {
        if let Some(tool_call) = Self::parse_tool_call_value(content) {
            return Some(tool_call);
        }

        // Strip markdown code fences and try the inner content
        for block in Self::extract_fenced_code_blocks(content) {
            if let Some(tool_call) = Self::parse_tool_call_value(block) {
                return Some(tool_call);
            }
            for candidate in Self::json_object_candidates(block) {
                if let Some(tool_call) = Self::parse_tool_call_value(candidate) {
                    return Some(tool_call);
                }
            }
        }

        for candidate in Self::json_object_candidates(content) {
            if let Some(tool_call) = Self::parse_tool_call_value(candidate) {
                return Some(tool_call);
            }
        }

        None
    }

    fn extract_fenced_code_blocks(content: &str) -> Vec<&str> {
        let mut blocks = Vec::new();
        let mut remaining = content;

        while let Some(fence_start) = remaining.find("```") {
            let after_fence = &remaining[fence_start + 3..];
            let body_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
            let body = &after_fence[body_start..];

            if let Some(fence_end) = body.find("```") {
                let block = body[..fence_end].trim();
                if !block.is_empty() {
                    blocks.push(block);
                }
                remaining = &body[fence_end + 3..];
            } else {
                break;
            }
        }

        blocks
    }

    /// Try to parse as a tool call, and if that fails, attempt to repair common
    /// LLM mistakes such as missing trailing closing braces.
    fn parse_tool_call_value(raw: &str) -> Option<(String, Value)> {
        // Try as-is first
        if let Some(result) = Self::try_parse_tool_call(raw) {
            return Some(result);
        }

        // Small LLMs often drop trailing `}` after very long string values.
        // Try appending up to 3 closing braces.
        let mut repaired = raw.to_string();
        for _ in 0..3 {
            repaired.push('}');
            if let Some(result) = Self::try_parse_tool_call(&repaired) {
                return Some(result);
            }
        }

        None
    }

    fn try_parse_tool_call(raw: &str) -> Option<(String, Value)> {
        let parsed: Value = serde_json::from_str(raw).ok()?;
        let tool_name = parsed.get("tool")?.as_str()?.to_string();
        let args = parsed.get("args").cloned().unwrap_or(Value::Null);
        Some((tool_name, args))
    }

    fn json_object_candidates(content: &str) -> Vec<&str> {
        let mut candidates = Vec::new();
        let mut start = None;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;

        for (idx, ch) in content.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }

                continue;
            }

            match ch {
                '"' => in_string = true,
                '{' => {
                    if depth == 0 {
                        start = Some(idx);
                    }
                    depth += 1;
                }
                '}' => {
                    if depth == 0 {
                        continue;
                    }

                    depth -= 1;
                    if depth == 0
                        && let Some(start_idx) = start.take()
                    {
                        candidates.push(&content[start_idx..=idx]);
                    }
                }
                _ => {}
            }
        }

        // If we still have an unclosed candidate (LLM dropped trailing `}`),
        // include the remainder as a candidate so parse_tool_call_value can
        // attempt to repair it.
        if depth > 0
            && let Some(start_idx) = start
        {
            candidates.push(&content[start_idx..]);
        }

        candidates
    }

    async fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, String> {
        #[cfg(target_arch = "wasm32")]
        {
            return Ok(self
                .sessions
                .lock()
                .await
                .get(session_id)
                .cloned()
                .unwrap_or_default());
        }

        #[cfg(not(target_arch = "wasm32"))]
        let path = self.workspace.session_file(session_id);
        #[cfg(not(target_arch = "wasm32"))]
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|e| format!("Failed to check session state: {e}"))?
        {
            return Ok(Vec::new());
        }

        #[cfg(not(target_arch = "wasm32"))]
        let raw = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read session state: {e}"))?;
        #[cfg(not(target_arch = "wasm32"))]
        let values: Vec<Value> = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse session state: {e}"))?;

        #[cfg(not(target_arch = "wasm32"))]
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| Message::try_from(IndexedValue { index, value }))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Failed to decode session messages.".to_string())
    }

    async fn save_messages(&self, session_id: &str, messages: &[Message]) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            self.sessions
                .lock()
                .await
                .insert(session_id.to_string(), messages.to_vec());
            return Ok(());
        }

        #[cfg(not(target_arch = "wasm32"))]
        let path = self.workspace.session_file(session_id);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create session directory: {e}"))?;
        }

        #[cfg(not(target_arch = "wasm32"))]
        let values = messages
            .iter()
            .cloned()
            .map(Value::from)
            .collect::<Vec<_>>();
        #[cfg(not(target_arch = "wasm32"))]
        let raw = serde_json::to_string_pretty(&values)
            .map_err(|e| format!("Failed to serialize session state: {e}"))?;

        #[cfg(not(target_arch = "wasm32"))]
        tokio::fs::write(path, raw)
            .await
            .map_err(|e| format!("Failed to write session state: {e}"))
    }

    async fn persist(&self, session_id: &str) {
        let _ = self.memory.flush_all(session_id).await;
    }

    async fn persist_state(&self, session_id: &str, messages: &[Message]) {
        let _ = self.save_messages(session_id, messages).await;
        self.persist(session_id).await;
    }

    fn push_out_message(messages: &mut Vec<Message>, value: Value) {
        let prev_message_id = messages.last().map(|message| message.message_id.clone());
        messages.push(Message::out(value, prev_message_id));
    }

    fn extract_tool_invocations(&self, assistant_message: &Value) -> Vec<ToolInvocation> {
        let native_calls = assistant_message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if !native_calls.is_empty() {
            return native_calls
                .into_iter()
                .map(|tool_call| ToolInvocation {
                    name: tool_call
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    args: tool_call
                        .get("function")
                        .and_then(|function| function.get("arguments"))
                        .map(|arguments| match arguments {
                            Value::String(raw) => serde_json::from_str(raw)
                                .unwrap_or_else(|_| Value::String(raw.clone())),
                            _ => arguments.clone(),
                        })
                        .unwrap_or(Value::Null),
                    call_id: tool_call
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect();
        }

        self.parse_content_tool_call(assistant_message)
            .map(|(name, args)| {
                vec![ToolInvocation {
                    name,
                    args,
                    call_id: None,
                }]
            })
            .unwrap_or_default()
    }

    async fn build_tool_result_messages(
        &self,
        invocations: Vec<ToolInvocation>,
        ctx: &ToolContext,
        parent_message_id: Option<String>,
    ) -> Vec<Message> {
        let mut messages = Vec::with_capacity(invocations.len());

        for invocation in invocations {
            let tool_message = self
                .tool_executor
                .build_tool_message(
                    &self.tool_registry,
                    &invocation.name,
                    &invocation.args,
                    ctx,
                    invocation.call_id.as_deref(),
                )
                .await;
            messages.push(Message::out(tool_message, parent_message_id.clone()));
        }

        messages
    }

    async fn step(
        &self,
        messages: &mut Vec<Message>,
        ctx: &ToolContext,
    ) -> Result<StepOutcome, String> {
        let assistant_message = self.call_llm(messages).await?;
        Self::push_out_message(messages, assistant_message.clone());

        let parent_message_id = messages.last().map(|message| message.message_id.clone());
        let invocations = self.extract_tool_invocations(&assistant_message);

        if !invocations.is_empty() {
            let tool_messages = self
                .build_tool_result_messages(invocations, ctx, parent_message_id)
                .await;
            messages.extend(tool_messages);
            return Ok(StepOutcome::Continue);
        }

        let content = assistant_message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        Ok(StepOutcome::Final(content))
    }

    pub async fn run(&self, session_id: &str, user_message: &str) -> String {
        let ctx = self.workspace.tool_context(session_id);
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = tokio::fs::create_dir_all(&ctx.workspace_dir).await {
            return format!("Workspace error: {e}");
        }

        let mut messages = match self.load_messages(session_id).await {
            Ok(messages) => messages,
            Err(e) => return format!("Session state error: {e}"),
        };

        if messages.is_empty() {
            let memory_context = self
                .memory
                .build_context(session_id, user_message)
                .await
                .unwrap_or_default();
            messages.push(Message::system(self.system_prompt(&ctx, &memory_context)));
        }

        let request_id = next_request_id();
        let prev_message_id = messages.last().map(|message| message.message_id.clone());

        messages.push(Message::user(
            user_message.to_string(),
            request_id,
            prev_message_id,
            None,
        ));

        for _ in 0..self.definition.max_iterations {
            match self.step(&mut messages, &ctx).await {
                Ok(StepOutcome::Continue) => {
                    self.persist_state(session_id, &messages).await;
                }
                Ok(StepOutcome::Final(content)) => {
                    let _ = self
                        .memory
                        .record_all(session_id, user_message, &content)
                        .await;
                    let _ = self.memory.consolidate_all(session_id).await;
                    self.persist_state(session_id, &messages).await;
                    return content;
                }
                Err(e) => {
                    let content = format!("LLM error: {e}");
                    Self::push_out_message(
                        &mut messages,
                        serde_json::json!({
                            "role": "assistant",
                            "content": content,
                        }),
                    );
                    self.persist_state(session_id, &messages).await;
                    return messages
                        .last()
                        .and_then(|message| {
                            let value = Value::from(message);
                            value
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| "LLM error".to_string());
                }
            }
        }

        let content = "Max iterations reached.".to_string();
        Self::push_out_message(
            &mut messages,
            serde_json::json!({
                "role": "assistant",
                "content": content.clone(),
            }),
        );
        let _ = self
            .memory
            .record_all(session_id, user_message, &content)
            .await;
        let _ = self.memory.consolidate_all(session_id).await;
        self.persist_state(session_id, &messages).await;
        content
    }
}

#[cfg(test)]
mod tests {
    use super::Agent;
    use crate::agent::AgentDefinition;
    use crate::llm::{
        ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult,
        ToolDefinition,
    };
    use crate::tooling::types::{parse_tool_args, Tool, ToolContext, ToolRegistryBuilder};
    use async_trait::async_trait;
    use futures::stream;
    use serde::Deserialize;
    use serde_json::json;
    use serde_json::Value;
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
}
