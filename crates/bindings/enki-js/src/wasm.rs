use async_trait::async_trait;
use core_next::agent::{Agent, AgentDefinition};
use core_next::llm::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, MessageRole, ResponseStream, ToolDefinition,
};
use core_next::memory::{MemoryManager, MemoryRouter, MemoryStrategy};
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry};
use futures::stream;
use js_sys::{Function, Promise};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

thread_local! {
    static CALLBACKS: RefCell<CallbackRegistry> = RefCell::new(CallbackRegistry::default());
}

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
}

#[derive(Deserialize)]
struct JsLlmResponse {
    content: String,
    #[serde(default)]
    tool_calls: Vec<Value>,
}

#[derive(Default)]
struct NoopMemoryRouter;

#[async_trait(?Send)]
impl MemoryRouter for NoopMemoryRouter {
    async fn select(&self, _user_message: &str) -> MemoryStrategy {
        MemoryStrategy {
            active_providers: Vec::new(),
            max_context_entries: 0,
        }
    }
}

struct JsLlmProvider {
    callback_id: u32,
    options: AgentOptions,
}

#[async_trait(?Send)]
impl LlmProvider for JsLlmProvider {
    async fn complete(
        &self,
        messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> core_next::llm::Result<LlmResponse> {
        self.invoke(messages, &[]).await
    }

    async fn complete_stream(
        &self,
        _messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> core_next::llm::Result<ResponseStream> {
        Ok(Box::pin(stream::empty()))
    }

    async fn complete_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        _config: &LlmConfig,
    ) -> core_next::llm::Result<LlmResponse> {
        self.invoke(messages, tools).await
    }

    fn name(&self) -> &'static str {
        "javascript"
    }

    fn available_models(&self) -> Vec<&'static str> {
        vec!["js::callback"]
    }
}

impl JsLlmProvider {
    async fn invoke(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> core_next::llm::Result<LlmResponse> {
        let payload = JsLlmRequest {
            agent: JsAgentMetadata {
                name: self.options.name.clone(),
                system_prompt_preamble: self.options.system_prompt_preamble.clone(),
                model: self.options.model.clone(),
                max_iterations: self.options.max_iterations,
            },
            messages: messages
                .iter()
                .map(|message| JsChatMessage {
                    role: message_role_name(message.role).to_string(),
                    content: message.content.clone(),
                    tool_call_id: message.tool_call_id.clone(),
                })
                .collect(),
            tools: tools
                .iter()
                .map(|tool| JsToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                })
                .collect(),
        };

        let payload = serde_wasm_bindgen::to_value(&payload)
            .map_err(|error| core_next::llm::LlmError::Provider(error.to_string()))?;
        let result = invoke_callback(self.callback_id, &payload)
            .await
            .map_err(|error| core_next::llm::LlmError::Provider(js_error_message(&error)))?;

        if let Some(content) = result.as_string() {
            return Ok(LlmResponse {
                content,
                usage: None,
                tool_calls: Vec::new(),
                model: self.options.model.clone(),
                finish_reason: Some("stop".to_string()),
            });
        }

        let response: JsLlmResponse = serde_wasm_bindgen::from_value(result)
            .map_err(|error| core_next::llm::LlmError::Provider(error.to_string()))?;

        Ok(LlmResponse {
            content: response.content,
            usage: None,
            tool_calls: response
                .tool_calls
                .into_iter()
                .map(|tool_call| tool_call.to_string())
                .collect(),
            model: self.options.model.clone(),
            finish_reason: Some("stop".to_string()),
        })
    }
}

struct JsTool {
    name: String,
    description: String,
    parameters: Value,
    callback_id: Option<u32>,
}

#[async_trait(?Send)]
impl Tool for JsTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let Some(callback_id) = self.callback_id else {
            return format!("Error: tool handler not configured for `{}`", self.name);
        };

        let payload = serde_wasm_bindgen::to_value(&serde_json::json!({
            "tool": self.name,
            "args": args,
            "context": {
                "agent_dir": ctx.agent_dir.to_string_lossy(),
                "workspace_dir": ctx.workspace_dir.to_string_lossy(),
                "sessions_dir": ctx.sessions_dir.to_string_lossy(),
            }
        }));

        let Ok(payload) = payload else {
            return "Error: failed to serialize tool request.".to_string();
        };

        match invoke_callback(callback_id, &payload).await {
            Ok(result) => result.as_string().unwrap_or_else(|| js_error_message(&result)),
            Err(error) => format!("Error: {}", js_error_message(&error)),
        }
    }
}

#[wasm_bindgen]
pub struct EnkiJsAgent {
    options: AgentOptions,
    llm_callback_id: u32,
    tool_callback_id: Option<u32>,
    tools: Vec<EnkiJsTool>,
    agent: RefCell<Option<Agent>>,
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

        let llm_callback_id = CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(llm_handler));
        let tool_callback_id =
            tool_handler.map(|function| CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(function)));

        Ok(Self {
            options,
            llm_callback_id,
            tool_callback_id,
            tools,
            agent: RefCell::new(None),
        })
    }

    #[wasm_bindgen(js_name = run)]
    pub async fn run(&self, session_id: String, user_message: String) -> Result<String, JsValue> {
        if self.agent.borrow().is_none() {
            let agent = self.build_agent().await?;
            self.agent.borrow_mut().replace(agent);
        }

        let agent = self
            .agent
            .borrow_mut()
            .take()
            .expect("agent initialized");
        let response = agent.run(&session_id, &user_message).await;
        self.agent.borrow_mut().replace(agent);
        Ok(response)
    }

    #[wasm_bindgen(js_name = toolCatalogJson)]
    pub fn tool_catalog_json(&self) -> String {
        let catalog = self
            .tools
            .iter()
            .map(|tool| {
                (
                    tool.name.clone(),
                    serde_json::json!({
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
    async fn build_agent(&self) -> Result<Agent, JsValue> {
        let tool_registry = self.build_tool_registry()?;
        let llm = JsLlmProvider {
            callback_id: self.llm_callback_id,
            options: self.options.clone(),
        };
        let memory = MemoryManager::new(Box::new(NoopMemoryRouter), Vec::new());

        Agent::with_definition_tool_registry_executor_llm_and_workspace(
            AgentDefinition {
                name: self.options.name.clone(),
                system_prompt_preamble: self.options.system_prompt_preamble.clone(),
                model: self.options.model.clone(),
                max_iterations: self.options.max_iterations as usize,
            },
            tool_registry,
            Box::new(RegistryToolExecutor),
            Some(Box::new(llm)),
            Some(memory),
            None,
        )
        .await
        .map_err(|error| JsValue::from_str(&error))
    }

    fn build_tool_registry(&self) -> Result<ToolRegistry, JsValue> {
        let mut registry = ToolRegistry::new();

        for tool in &self.tools {
            let parameters = if tool.parameters_json.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str::<Value>(&tool.parameters_json).map_err(|error| {
                    JsValue::from_str(&format!(
                        "Invalid parameters_json for tool `{}`: {error}",
                        tool.name
                    ))
                })?
            };

            registry.insert(
                tool.name.clone(),
                Box::new(JsTool {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters,
                    callback_id: self.tool_callback_id,
                }),
            );
        }

        Ok(registry)
    }
}

impl Drop for EnkiJsAgent {
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

fn message_role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn js_error_message(value: &JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| js_sys::JSON::stringify(value).ok().and_then(|v| v.as_string()).unwrap_or_else(|| "JavaScript error".to_string()))
}
