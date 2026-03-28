use std::path::PathBuf;

#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

use serde_json::Value;

use crate::agent::agent_loop::{AgentLoop, DefaultAgentLoop};
use crate::agent::types::{
    AgentDefinition, AgentRunResult, StepOutcome, ToolCallTrace, ToolInvocation,
};
use crate::agent::workspace::AgentWorkspace;
#[cfg(all(not(target_arch = "wasm32"), feature = "universal-llm-provider"))]
use crate::llm::UniversalLLMClient;
use crate::llm::{ChatMessage, LlmConfig, LlmProvider, MessageRole, ToolDefinition};
use crate::memory::MemoryManager;
use crate::message::{IndexedValue, Message};
use crate::tooling::builtin_tools;
use crate::tooling::tool_calling::{RegistryToolExecutor, ToolCallRegistry, ToolExecutor};
use crate::tooling::types::{ToolContext, ToolRegistry};

pub struct Agent {
    pub definition: AgentDefinition,
    pub tool_registry: ToolCallRegistry,
    pub tool_executor: Box<dyn ToolExecutor>,
    pub workspace: AgentWorkspace,
    pub llm: Box<dyn LlmProvider>,
    pub memory: MemoryManager,
    pub agent_loop: Box<dyn AgentLoop>,
    #[cfg(target_arch = "wasm32")]
    pub sessions: Mutex<HashMap<String, Vec<Message>>>,
}

impl Agent {
    pub fn with_agent_loop(mut self, agent_loop: Box<dyn AgentLoop>) -> Self {
        self.agent_loop = agent_loop;
        self
    }

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
                #[cfg(all(not(target_arch = "wasm32"), feature = "universal-llm-provider"))]
                {
                    Box::new(UniversalLLMClient::new(&model).map_err(|e| e.to_string())?)
                }

                #[cfg(any(target_arch = "wasm32", not(feature = "universal-llm-provider")))]
                {
                    return Err(format!(
                        "No built-in LLM provider is available for model `{model}`. Supply a custom LlmProvider or enable the `universal-llm-provider` feature."
                    ));
                }
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
            agent_loop: Box::new(DefaultAgentLoop),
            #[cfg(target_arch = "wasm32")]
            sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn system_prompt(&self, ctx: &ToolContext, memory_context: &str) -> String {
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

    pub fn to_llm_messages(&self, messages: &[Message]) -> Vec<ChatMessage> {
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

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
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

    pub fn decode_tool_calls(&self, tool_calls: Vec<String>) -> Vec<Value> {
        tool_calls
            .into_iter()
            .filter_map(|tool_call| serde_json::from_str::<Value>(&tool_call).ok())
            .collect()
    }

    pub async fn call_llm(&self, messages: &[Message]) -> Result<Value, String> {
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

    pub fn parse_content_tool_call(&self, assistant_message: &Value) -> Option<(String, Value)> {
        let content = assistant_message.get("content")?.as_str()?;
        Self::extract_embedded_tool_call(content)
    }

    pub fn extract_embedded_tool_call(content: &str) -> Option<(String, Value)> {
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

    pub fn extract_fenced_code_blocks(content: &str) -> Vec<&str> {
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
    pub fn parse_tool_call_value(raw: &str) -> Option<(String, Value)> {
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

    pub fn try_parse_tool_call(raw: &str) -> Option<(String, Value)> {
        let parsed: Value = serde_json::from_str(raw).ok()?;
        let tool_name = parsed.get("tool")?.as_str()?.to_string();
        let args = parsed.get("args").cloned().unwrap_or(Value::Null);
        Some((tool_name, args))
    }

    pub fn json_object_candidates(content: &str) -> Vec<&str> {
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

    pub async fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, String> {
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

    pub async fn save_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), String> {
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

    pub async fn persist(&self, session_id: &str) {
        let _ = self.memory.flush_all(session_id).await;
    }

    pub async fn persist_state(&self, session_id: &str, messages: &[Message]) {
        let _ = self.save_messages(session_id, messages).await;
        self.persist(session_id).await;
    }

    pub fn push_out_message(messages: &mut Vec<Message>, value: Value) {
        let prev_message_id = messages.last().map(|message| message.message_id.clone());
        messages.push(Message::out(value, prev_message_id));
    }

    pub fn extract_tool_invocations(&self, assistant_message: &Value) -> Vec<ToolInvocation> {
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

    pub async fn execute_tool_invocations(
        &self,
        invocations: Vec<ToolInvocation>,
        ctx: &ToolContext,
        parent_message_id: Option<String>,
    ) -> (Vec<ToolCallTrace>, Vec<Message>) {
        let mut traces = Vec::with_capacity(invocations.len());
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
            traces.push(ToolCallTrace {
                name: invocation.name.clone(),
                args: invocation.args.clone(),
                call_id: invocation.call_id.clone(),
                result: tool_message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
            messages.push(Message::out(tool_message, parent_message_id.clone()));
        }

        (traces, messages)
    }

    pub async fn step(
        &self,
        messages: &mut Vec<Message>,
        ctx: &ToolContext,
    ) -> Result<StepOutcome, String> {
        let assistant_message = self.call_llm(messages).await?;
        Self::push_out_message(messages, assistant_message.clone());

        let parent_message_id = messages.last().map(|message| message.message_id.clone());
        let invocations = self.extract_tool_invocations(&assistant_message);

        if !invocations.is_empty() {
            let tool_names = invocations
                .iter()
                .map(|invocation| invocation.name.clone())
                .collect();
            let (tool_traces, tool_messages) = self
                .execute_tool_invocations(invocations, ctx, parent_message_id)
                .await;
            messages.extend(tool_messages);
            return Ok(StepOutcome::Continue {
                tool_names,
                tool_traces,
            });
        }

        let content = assistant_message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        Ok(StepOutcome::Final(content))
    }

    pub async fn run(&self, session_id: &str, user_message: &str) -> String {
        self.agent_loop.run(self, session_id, user_message).await
    }

    pub async fn run_detailed(
        &self,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(crate::agent::types::ExecutionStep) + Send + Sync>>,
    ) -> AgentRunResult {
        self.agent_loop
            .run_detailed(self, session_id, user_message, on_step)
            .await
    }

    pub async fn run_detailed_with_human(
        &self,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(crate::agent::types::ExecutionStep) + Send + Sync>>,
        human: Option<std::sync::Arc<dyn crate::tooling::types::AskHumanFn>>,
    ) -> AgentRunResult {
        self.agent_loop
            .run_detailed_with_human(self, session_id, user_message, on_step, human)
            .await
    }
}
