use js_sys::{Function, Promise, Reflect};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnkiJsTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters_json: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AgentOptions {
    name: String,
    system_prompt_preamble: String,
    max_iterations: u32,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            name: "Personal Assistant".to_string(),
            system_prompt_preamble: "You are a helpful Personal Assistant agent.".to_string(),
            max_iterations: 20,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SessionMessage {
    role: String,
    content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<Value>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LlmRequest {
    agent: AgentOptions,
    messages: Vec<SessionMessage>,
    tools: Vec<ToolDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LlmResponse {
    content: String,
    #[serde(default)]
    tool_calls: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ToolCallFunction {
    name: String,
    arguments: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ToolCall {
    #[serde(default)]
    id: Option<String>,
    function: ToolCallFunction,
}

#[wasm_bindgen]
pub struct EnkiJsAgent {
    options: AgentOptions,
    llm_handler: Function,
    tool_handler: Option<Function>,
    tools: Vec<EnkiJsTool>,
    sessions: RefCell<HashMap<String, Vec<SessionMessage>>>,
}

#[wasm_bindgen]
impl EnkiJsAgent {
    #[wasm_bindgen(constructor)]
    pub fn new(
        name: Option<String>,
        system_prompt_preamble: Option<String>,
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
        if let Some(max_iterations) = max_iterations {
            options.max_iterations = max_iterations.max(1);
        }

        let tools = if tools.is_undefined() || tools.is_null() {
            Vec::new()
        } else {
            serde_wasm_bindgen::from_value::<Vec<EnkiJsTool>>(tools)
                .map_err(|error| JsValue::from_str(&format!("Invalid tools array: {error}")))?
        };

        Ok(Self {
            options,
            llm_handler,
            tool_handler,
            tools,
            sessions: RefCell::new(HashMap::new()),
        })
    }

    #[wasm_bindgen(js_name = run)]
    pub async fn run(&self, session_id: String, user_message: String) -> Result<String, JsValue> {
        let mut messages = {
            let mut sessions = self.sessions.borrow_mut();
            let session = sessions.entry(session_id.clone()).or_insert_with(|| {
                vec![SessionMessage {
                    role: "system".to_string(),
                    content: self.system_prompt(),
                    tool_call_id: None,
                    tool_calls: None,
                }]
            });
            session.clone()
        };

        messages.push(SessionMessage {
            role: "user".to_string(),
            content: user_message,
            tool_call_id: None,
            tool_calls: None,
        });

        for _ in 0..self.options.max_iterations {
            let response = self.call_llm(&messages).await?;
            let invocations = extract_tool_invocations(&response);

            messages.push(SessionMessage {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_call_id: None,
                tool_calls: (!response.tool_calls.is_empty()).then_some(response.tool_calls.clone()),
            });

            if invocations.is_empty() {
                self.sessions.borrow_mut().insert(session_id, messages);
                return Ok(response.content);
            }

            for invocation in invocations {
                let tool_result = self.execute_tool(&session_id, &invocation).await?;
                messages.push(SessionMessage {
                    role: "tool".to_string(),
                    content: tool_result,
                    tool_call_id: invocation.id,
                    tool_calls: None,
                });
            }
        }

        let content = "Max iterations reached.".to_string();
        messages.push(SessionMessage {
            role: "assistant".to_string(),
            content: content.clone(),
            tool_call_id: None,
            tool_calls: None,
        });
        self.sessions.borrow_mut().insert(session_id, messages);
        Ok(content)
    }

    #[wasm_bindgen(js_name = resetSession)]
    pub fn reset_session(&self, session_id: String) {
        self.sessions.borrow_mut().remove(&session_id);
    }

    #[wasm_bindgen(js_name = toolCatalogJson)]
    pub fn tool_catalog_json(&self) -> String {
        let catalog = self
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

impl EnkiJsAgent {
    fn system_prompt(&self) -> String {
        format!(
            "You are {}.\n{} Use tools via JSON calls when needed.\nAvailable tools: {}",
            self.options.name,
            self.options.system_prompt_preamble,
            self.tools
                .iter()
                .map(|tool| format!("{} ({})", tool.name, tool.description))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    async fn call_llm(&self, messages: &[SessionMessage]) -> Result<LlmResponse, JsValue> {
        let request = LlmRequest {
            agent: self.options.clone(),
            messages: messages.to_vec(),
            tools: self
                .tools
                .iter()
                .map(|tool| ToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                })
                .collect(),
        };

        let request = serde_wasm_bindgen::to_value(&request)
            .map_err(|error| JsValue::from_str(&format!("Failed to serialize LLM request: {error}")))?;
        let output = invoke_async(&self.llm_handler, &request).await?;

        if let Some(text) = output.as_string() {
            return Ok(LlmResponse {
                content: text,
                tool_calls: Vec::new(),
            });
        }

        serde_wasm_bindgen::from_value(output)
            .map_err(|error| JsValue::from_str(&format!("Invalid LLM response: {error}")))
    }

    async fn execute_tool(
        &self,
        session_id: &str,
        invocation: &ToolCall,
    ) -> Result<String, JsValue> {
        let Some(tool_handler) = &self.tool_handler else {
            return Ok(format!(
                "Error: tool handler not configured for tool `{}`",
                invocation.function.name
            ));
        };

        let payload = serde_wasm_bindgen::to_value(&json!({
            "session_id": session_id,
            "tool": invocation.function.name,
            "args": invocation.function.arguments,
        }))
        .map_err(|error| JsValue::from_str(&format!("Failed to serialize tool request: {error}")))?;

        let output = invoke_async(tool_handler, &payload).await?;
        Ok(output
            .as_string()
            .unwrap_or_else(|| js_value_to_string(&output)))
    }
}

async fn invoke_async(function: &Function, payload: &JsValue) -> Result<JsValue, JsValue> {
    let value = function.call1(&JsValue::NULL, payload)?;

    if value.is_instance_of::<Promise>() {
        JsFuture::from(Promise::from(value)).await
    } else {
        Ok(value)
    }
}

fn extract_tool_invocations(response: &LlmResponse) -> Vec<ToolCall> {
    if !response.tool_calls.is_empty() {
        return response
            .tool_calls
            .iter()
            .filter_map(|value| serde_json::from_value::<ToolCall>(value.clone()).ok())
            .collect();
    }

    extract_embedded_tool_call(&response.content)
        .map(|(name, arguments)| {
            vec![ToolCall {
                id: None,
                function: ToolCallFunction { name, arguments },
            }]
        })
        .unwrap_or_default()
}

fn extract_embedded_tool_call(content: &str) -> Option<(String, Value)> {
    if let Some(result) = parse_tool_call_value(content) {
        return Some(result);
    }

    for block in extract_fenced_code_blocks(content) {
        if let Some(result) = parse_tool_call_value(block) {
            return Some(result);
        }
        for candidate in json_object_candidates(block) {
            if let Some(result) = parse_tool_call_value(candidate) {
                return Some(result);
            }
        }
    }

    for candidate in json_object_candidates(content) {
        if let Some(result) = parse_tool_call_value(candidate) {
            return Some(result);
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
    if let Some(result) = try_parse_tool_call(raw) {
        return Some(result);
    }

    let mut repaired = raw.to_string();
    for _ in 0..3 {
        repaired.push('}');
        if let Some(result) = try_parse_tool_call(&repaired) {
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

fn js_value_to_string(value: &JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }

    if let Ok(stringified) = js_sys::JSON::stringify(value) {
        if let Some(text) = stringified.as_string() {
            return text;
        }
    }

    if let Ok(to_string) = Reflect::get(value, &JsValue::from_str("toString")) {
        if let Some(function) = to_string.dyn_ref::<Function>() {
            if let Ok(result) = function.call0(value) {
                if let Some(text) = result.as_string() {
                    return text;
                }
            }
        }
    }

    "[object Object]".to_string()
}
