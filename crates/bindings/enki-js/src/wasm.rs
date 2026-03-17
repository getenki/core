use js_sys::{Function, Promise};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

thread_local! {
    static CALLBACKS: RefCell<CallbackRegistry> = RefCell::new(CallbackRegistry::default());
}

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct CallbackRegistry {
    next_id: u32,
    functions: HashMap<u32, Function>,
}

impl CallbackRegistry {
    fn insert(&mut self, function: Function) -> u32 {
        self.next_id = self.next_id.saturating_add(1).max(1);
        self.functions.insert(self.next_id, function);
        self.next_id
    }

    fn get(&self, callback_id: u32) -> Option<Function> {
        self.functions.get(&callback_id).cloned()
    }

    fn remove(&mut self, callback_id: u32) {
        self.functions.remove(&callback_id);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnkiJsTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters_json: String,
}

#[derive(Clone, Debug)]
struct AgentOptions {
    name: String,
    system_prompt_preamble: String,
    model: String,
    max_iterations: u32,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            name: "Personal Assistant".to_string(),
            system_prompt_preamble: "You are a helpful Personal Assistant agent.".to_string(),
            model: "js::callback".to_string(),
            max_iterations: 20,
        }
    }
}

#[derive(Serialize)]
struct JsLlmRequest {
    agent: JsAgentMetadata,
    messages: Vec<JsChatMessage>,
    tools: Vec<JsToolDefinition>,
}

#[derive(Serialize)]
struct JsAgentMetadata {
    name: String,
    system_prompt_preamble: String,
    model: String,
    max_iterations: u32,
}

#[derive(Serialize)]
struct JsChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct JsToolDefinition {
    name: String,
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Value>,
}

#[derive(Serialize)]
struct JsToolCallbackRequest {
    tool: String,
    args: Value,
    context: JsToolCallbackContext,
}

#[derive(Serialize)]
struct JsToolCallbackContext {
    agent_dir: String,
    workspace_dir: String,
    sessions_dir: String,
}

#[derive(Deserialize)]
struct JsLlmResponse {
    content: String,
    #[serde(default)]
    tool_calls: Vec<Value>,
}

#[derive(Clone)]
struct ToolContext {
    agent_dir: PathBuf,
    workspace_dir: PathBuf,
    sessions_dir: PathBuf,
}

struct ToolInvocation {
    name: String,
    args: Value,
    call_id: Option<String>,
}

#[wasm_bindgen]
pub struct EnkiJsAgent {
    inner: Rc<EnkiJsAgentInner>,
}

struct EnkiJsAgentInner {
    options: AgentOptions,
    llm_callback_id: u32,
    tool_callback_id: Option<u32>,
    tools: Vec<EnkiJsTool>,
    sessions: RefCell<HashMap<String, Vec<Value>>>,
}

#[wasm_bindgen]
impl EnkiJsAgent {
    #[wasm_bindgen(constructor)]
    pub fn new(
        name: Option<String>,
        system_prompt_preamble: Option<String>,
        model: Option<String>,
        max_iterations: Option<u32>,
        llm_handler: Function,
        tool_handler: Option<Function>,
        tools: JsValue,
    ) -> Result<EnkiJsAgent, JsValue> {
        let mut options = AgentOptions::default();
        if let Some(name) = name {
            options.name = name;
        }
        if let Some(system_prompt_preamble) = system_prompt_preamble {
            options.system_prompt_preamble = system_prompt_preamble;
        }
        if let Some(model) = model {
            options.model = model;
        }
        if let Some(max_iterations) = max_iterations {
            options.max_iterations = max_iterations.max(1);
        }

        let tools = if tools.is_null() || tools.is_undefined() {
            Vec::new()
        } else {
            serde_wasm_bindgen::from_value::<Vec<EnkiJsTool>>(tools)
                .map_err(|error| JsValue::from_str(&format!("Invalid tools array: {error}")))?
        };

        let llm_callback_id =
            CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(llm_handler));
        let tool_callback_id = tool_handler
            .map(|function| CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(function)));

        Ok(Self {
            inner: Rc::new(EnkiJsAgentInner {
                options,
                llm_callback_id,
                tool_callback_id,
                tools,
                sessions: RefCell::new(HashMap::new()),
            }),
        })
    }

    #[wasm_bindgen(js_name = run)]
    pub async fn run(&self, session_id: String, user_message: String) -> Result<String, JsValue> {
        let inner = Rc::clone(&self.inner);
        let mut messages = inner
            .sessions
            .borrow()
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| vec![inner.system_message()]);

        messages.push(inner.user_message(user_message));
        let response = inner.run_loop(&mut messages).await?;
        inner.sessions.borrow_mut().insert(session_id, messages);
        Ok(response)
    }

    #[wasm_bindgen(js_name = toolCatalogJson)]
    pub fn tool_catalog_json(&self) -> String {
        let catalog = self
            .inner
            .tools
            .iter()
            .map(|tool| {
                (
                    tool.name.clone(),
                    json!({
                        "description": tool.description,
                        "parameters_json": tool.parameters_json,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>();

        Value::Object(catalog).to_string()
    }
}

impl EnkiJsAgentInner {
    async fn run_loop(&self, messages: &mut Vec<Value>) -> Result<String, JsValue> {
        let mut last_response = String::new();
        let ctx = self.tool_context();

        for _ in 0..self.options.max_iterations.max(1) {
            let response = self.invoke_llm(messages).await?;
            let mut assistant_message = json!({
                "role": "assistant",
                "content": response.content,
            });

            if !response.tool_calls.is_empty() {
                assistant_message["tool_calls"] = Value::Array(response.tool_calls.clone());
            }

            last_response = assistant_message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            let invocations = self.extract_tool_invocations(&assistant_message);
            messages.push(assistant_message);

            if invocations.is_empty() {
                return Ok(last_response);
            }

            for invocation in invocations {
                let tool_message = self.execute_tool(&invocation, &ctx).await;
                messages.push(tool_message);
            }
        }

        Ok(last_response)
    }

    async fn invoke_llm(&self, messages: &[Value]) -> Result<JsLlmResponse, JsValue> {
        let payload = JsLlmRequest {
            agent: JsAgentMetadata {
                name: self.options.name.clone(),
                system_prompt_preamble: self.options.system_prompt_preamble.clone(),
                model: self.options.model.clone(),
                max_iterations: self.options.max_iterations,
            },
            messages: self.to_js_messages(messages),
            tools: self.js_tools()?,
        };

        let payload = serde_wasm_bindgen::to_value(&payload)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let result = invoke_callback(self.llm_callback_id, &payload).await?;

        if let Some(content) = result.as_string() {
            return Ok(JsLlmResponse {
                content,
                tool_calls: Vec::new(),
            });
        }

        serde_wasm_bindgen::from_value(result)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    async fn execute_tool(&self, invocation: &ToolInvocation, ctx: &ToolContext) -> Value {
        let content = match self.tool_callback_id {
            Some(callback_id) => {
                let payload = serde_wasm_bindgen::to_value(&JsToolCallbackRequest {
                    tool: invocation.name.clone(),
                    args: normalize_tool_args(&invocation.args),
                    context: JsToolCallbackContext {
                        agent_dir: ctx.agent_dir.to_string_lossy().into_owned(),
                        workspace_dir: ctx.workspace_dir.to_string_lossy().into_owned(),
                        sessions_dir: ctx.sessions_dir.to_string_lossy().into_owned(),
                    },
                });

                match payload {
                    Ok(payload) => match invoke_callback(callback_id, &payload).await {
                        Ok(result) => result
                            .as_string()
                            .unwrap_or_else(|| js_error_message(&result)),
                        Err(error) => format!("Error: {}", js_error_message(&error)),
                    },
                    Err(_) => "Error: failed to serialize tool request.".to_string(),
                }
            }
            None => format!(
                "Error: tool handler not configured for `{}`",
                invocation.name
            ),
        };

        let mut message = json!({
            "role": "tool",
            "tool_name": invocation.name,
            "content": content,
        });

        if let Some(call_id) = &invocation.call_id {
            message["tool_call_id"] = Value::String(call_id.clone());
        }

        message
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
                        .map(normalize_tool_args)
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

    fn parse_content_tool_call(&self, assistant_message: &Value) -> Option<(String, Value)> {
        let content = assistant_message.get("content")?.as_str()?;
        Self::extract_embedded_tool_call(content)
    }

    fn extract_embedded_tool_call(content: &str) -> Option<(String, Value)> {
        if let Some(tool_call) = Self::parse_tool_call_value(content) {
            return Some(tool_call);
        }

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
            let body_start = after_fence.find('\n').map(|index| index + 1).unwrap_or(0);
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

    fn parse_tool_call_value(raw: &str) -> Option<(String, Value)> {
        if let Some(result) = Self::try_parse_tool_call(raw) {
            return Some(result);
        }

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

        for (index, ch) in content.char_indices() {
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
                        start = Some(index);
                    }
                    depth += 1;
                }
                '}' => {
                    if depth == 0 {
                        continue;
                    }

                    depth -= 1;
                    if depth == 0 {
                        if let Some(start_index) = start.take() {
                            candidates.push(&content[start_index..=index]);
                        }
                    }
                }
                _ => {}
            }
        }

        if depth > 0 {
            if let Some(start_index) = start {
                candidates.push(&content[start_index..]);
            }
        }

        candidates
    }

    fn to_js_messages(&self, messages: &[Value]) -> Vec<JsChatMessage> {
        messages
            .iter()
            .filter_map(|value| {
                let role = value.get("role").and_then(Value::as_str)?.to_string();
                Some(JsChatMessage {
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

    fn js_tools(&self) -> Result<Vec<JsToolDefinition>, JsValue> {
        self.tools
            .iter()
            .map(|tool| {
                let parameters = if tool.parameters_json.trim().is_empty() {
                    None
                } else {
                    Some(
                        serde_json::from_str::<Value>(&tool.parameters_json).map_err(|error| {
                            JsValue::from_str(&format!(
                                "Invalid parameters_json for tool `{}`: {error}",
                                tool.name
                            ))
                        })?,
                    )
                };

                Ok(JsToolDefinition {
                    name: tool.name.clone(),
                    description: Some(tool.description.clone()),
                    parameters,
                })
            })
            .collect()
    }

    fn system_message(&self) -> Value {
        json!({
            "role": "system",
            "content": self.system_prompt(),
        })
    }

    fn user_message(&self, content: String) -> Value {
        json!({
            "role": "user",
            "content": content,
            "request_id": next_request_id(),
        })
    }

    fn system_prompt(&self) -> String {
        let ctx = self.tool_context();

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
Agent workspace: {}
Current task workspace: {}"#,
            self.options.name,
            self.options.system_prompt_preamble,
            self.prompt_tool_catalog(),
            ctx.agent_dir.display(),
            ctx.workspace_dir.display()
        )
    }

    fn prompt_tool_catalog(&self) -> Value {
        Value::Object(
            self.tools
                .iter()
                .map(|tool| {
                    let parameters = if tool.parameters_json.trim().is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str::<Value>(&tool.parameters_json).unwrap_or(Value::Null)
                    };

                    (
                        tool.name.clone(),
                        json!({
                            "description": tool.description,
                            "parameters": parameters,
                        }),
                    )
                })
                .collect(),
        )
    }

    fn tool_context(&self) -> ToolContext {
        ToolContext {
            agent_dir: PathBuf::from("agent"),
            workspace_dir: PathBuf::from("workspace"),
            sessions_dir: PathBuf::from("sessions"),
        }
    }
}

impl Drop for EnkiJsAgentInner {
    fn drop(&mut self) {
        CALLBACKS.with(|callbacks| {
            let mut callbacks = callbacks.borrow_mut();
            callbacks.remove(self.llm_callback_id);
            if let Some(tool_callback_id) = self.tool_callback_id {
                callbacks.remove(tool_callback_id);
            }
        });
    }
}

async fn invoke_callback(callback_id: u32, payload: &JsValue) -> Result<JsValue, JsValue> {
    let Some(function) = CALLBACKS.with(|callbacks| callbacks.borrow().get(callback_id)) else {
        return Err(JsValue::from_str("Callback is no longer registered."));
    };

    let result = function.call1(&JsValue::NULL, payload)?;
    if result.is_instance_of::<Promise>() {
        JsFuture::from(Promise::from(result)).await
    } else {
        Ok(result)
    }
}

fn normalize_tool_args(args: &Value) -> Value {
    match args {
        Value::String(raw) => {
            serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()))
        }
        _ => args.clone(),
    }
}

fn next_request_id() -> String {
    format!("req-{}", REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn js_error_message(value: &JsValue) -> String {
    value.as_string().unwrap_or_else(|| {
        js_sys::JSON::stringify(value)
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "JavaScript error".to_string())
    })
}
