use crate::tooling::tool_calling::{RegistryToolExecutor, ToolCallRegistry, ToolExecutor};
use crate::tooling::types::ToolContext;
use crate::{build_context, build_tools, ensure_dirs, post_json};
use crate::message::message::{IndexedValue, Message, next_request_id};
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::io::{BufRead, BufReader};

// Config
const DEFAULT_LLM_URL: &str = "http://localhost:11434/api/chat";
const DEFAULT_MODEL: &str = "qwen3.5:latest";
const DEFAULT_MAX_ITERATIONS: usize = 20;

pub struct AgentDefinition {
    pub name: String,
    pub system_prompt_preamble: String,
    pub llm_url: String,
    pub model: String,
    pub max_iterations: usize,
}

impl Default for AgentDefinition {
    fn default() -> Self {
        Self {
            name: "Personal Assistant".to_string(),
            system_prompt_preamble: "You are a helpful Personal Assistant agent.".to_string(),
            llm_url: DEFAULT_LLM_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }
}

pub struct Agent {
    definition: AgentDefinition,
    tool_registry: ToolCallRegistry,
    tool_executor: Box<dyn ToolExecutor>,
    ctx: ToolContext,
}

enum StepOutcome {
    Final(String),
    ToolResults(Vec<Message>),
}

impl Agent {
    pub(crate) fn new() -> Result<Self, String> {
        Self::with_definition(AgentDefinition::default())
    }

    pub(crate) fn with_definition(definition: AgentDefinition) -> Result<Self, String> {
        Self::with_definition_and_executor(definition, Box::new(RegistryToolExecutor))
    }

    pub(crate) fn with_definition_and_executor(
        definition: AgentDefinition,
        tool_executor: Box<dyn ToolExecutor>,
    ) -> Result<Self, String> {
        let ctx = build_context();
        ensure_dirs(&ctx)?;

        Ok(Self {
            definition,
            tool_registry: ToolCallRegistry::new(build_tools()),
            tool_executor,
            ctx,
        })
    }

    fn system_prompt(&self) -> String {
        format!(
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
Workspace: {}"#,
            self.definition.name,
            self.definition.system_prompt_preamble,
            self.tool_registry.catalog_json(),
            self.ctx.workspace_dir.display()
        )
    }

    fn load_session(&self, session_id: &str) -> Vec<Message> {
        let session_file = self.ctx.sessions_dir.join(format!("{session_id}.jsonl"));

        if session_file.exists() {
            if let Ok(file) = File::open(&session_file) {
                let reader = BufReader::new(file);
                let messages: Vec<Message> = reader
                    .lines()
                    .enumerate()
                    .filter_map(|(index, line)| {
                        line.ok().and_then(|line| {
                            serde_json::from_str::<Value>(&line)
                                .ok()
                                .and_then(|value| {
                                    Message::try_from(IndexedValue { index, value }).ok()
                                })
                        })
                    })
                    .collect();

                if !messages.is_empty() {
                    return messages;
                }
            }
        }

        vec![Message::system(self.system_prompt())]
    }

    fn save_messages(&self, session_id: &str, messages: &[Message]) -> Result<(), String> {
        let session_file = self.ctx.sessions_dir.join(format!("{session_id}.jsonl"));
        let start = messages.len().saturating_sub(10);

        let mut file = File::create(&session_file)
            .map_err(|e| format!("Failed to open session file for writing: {e}"))?;

        for msg in &messages[start..] {
            let line = serde_json::to_string(&Value::from(msg.clone()))
                .map_err(|e| format!("Failed to serialize message: {e}"))?;
            writeln!(file, "{line}").map_err(|e| format!("Failed to write session file: {e}"))?;
        }

        Ok(())
    }

    fn call_llm(&self, messages: &[Message]) -> Result<Value, String> {
        let payload = json!({
            "model": self.definition.model,
            "messages": messages.iter().map(Value::from).collect::<Vec<_>>(),
            "stream": false,
            "tools": self.tool_registry.tools_payload(),
        });

        let data = post_json(&self.definition.llm_url, &payload, 60)?;
        data.get("message")
            .cloned()
            .ok_or_else(|| "Missing `message` in LLM response.".to_string())
    }

    fn parse_content_tool_call(&self, assistant_message: &Value) -> Option<(String, Value)> {
        let content = assistant_message.get("content")?.as_str()?;
        let parsed: Value = serde_json::from_str(content).ok()?;
        let tool_name = parsed.get("tool")?.as_str()?.to_string();
        let args = parsed.get("args").cloned().unwrap_or(Value::Null);
        Some((tool_name, args))
    }

    fn step(&self, messages: &mut Vec<Message>) -> Result<StepOutcome, String> {
        let assistant_message = self.call_llm(messages)?;
        let prev_message_id = messages.last().map(|message| message.message_id.clone());
        messages.push(Message::out(assistant_message.clone(), prev_message_id));

        let tool_calls = assistant_message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        if !tool_calls.is_empty() {
            let mut tool_results = Vec::new();

            for tc in tool_calls {
                let tool_name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");

                let tool_call_id = tc.get("id").and_then(Value::as_str);

                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .cloned()
                    .unwrap_or(Value::Null);

                let prev_message_id = messages.last().map(|message| message.message_id.clone());
                tool_results.push(Message::out(
                    self.tool_executor.build_tool_message(
                        &self.tool_registry,
                        tool_name,
                        &args,
                        &self.ctx,
                        tool_call_id,
                    ),
                    prev_message_id,
                ));
            }

            return Ok(StepOutcome::ToolResults(tool_results));
        }

        if let Some((tool_name, args)) = self.parse_content_tool_call(&assistant_message) {
            let prev_message_id = messages.last().map(|message| message.message_id.clone());
            return Ok(StepOutcome::ToolResults(vec![Message::out(
                self.tool_executor.build_tool_message(
                    &self.tool_registry,
                    &tool_name,
                    &args,
                    &self.ctx,
                    None,
                ),
                prev_message_id,
            )]));
        }

        let content = assistant_message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        Ok(StepOutcome::Final(content))
    }

    pub(crate) fn run(&self, session_id: &str, user_message: &str) -> String {
        let mut messages = self.load_session(session_id);
        let request_id = next_request_id();
        let prev_message_id = messages.last().map(|message| message.message_id.clone());

        messages.push(Message::user(
            user_message.to_string(),
            request_id,
            prev_message_id,
        ));

        for _ in 0..self.definition.max_iterations {
            match self.step(&mut messages) {
                Ok(StepOutcome::Final(content)) => {
                    let _ = self.save_messages(session_id, &messages);
                    return content;
                }
                Ok(StepOutcome::ToolResults(tool_messages)) => {
                    messages.extend(tool_messages);
                    let _ = self.save_messages(session_id, &messages);
                }
                Err(e) => return format!("LLM error: {e}"),
            }
        }

        "Max iterations reached.".to_string()
    }
}
