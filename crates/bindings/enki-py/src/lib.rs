use async_trait::async_trait;
use core_next::agent::{
    Agent, AgentDefinition, AgentRunResult as CoreAgentRunResult,
    ExecutionStep as CoreExecutionStep,
};
use core_next::llm::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, ResponseStream, ToolDefinition,
};
use core_next::memory::{
    MemoryEntry, MemoryKind, MemoryManager, MemoryProvider, MemoryRouter, MemoryStrategy,
};
use core_next::runtime::{Runtime, RuntimeHandler, RuntimeRequest, SessionContext};
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry};
use futures::stream;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

#[derive(Clone, Debug)]
pub struct EnkiTool {
    pub name: String,
    pub description: String,
    pub parameters_json: String,
}

#[derive(Clone, Debug)]
pub struct EnkiExecutionStep {
    pub index: u64,
    pub phase: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct EnkiAgentRunResult {
    pub output: String,
    pub steps: Vec<EnkiExecutionStep>,
}

#[derive(Clone, Copy, Debug)]
pub enum EnkiMemoryKind {
    RecentMessage,
    Summary,
    Entity,
    Preference,
}

#[derive(Clone, Debug)]
pub struct EnkiMemoryEntry {
    pub key: String,
    pub content: String,
    pub kind: EnkiMemoryKind,
    pub relevance: f32,
    pub timestamp_ns: u64,
}

#[derive(Clone, Debug)]
pub struct EnkiMemoryModule {
    pub name: String,
}

pub trait EnkiToolHandler: Send + Sync {
    fn execute(
        &self,
        tool_name: String,
        args_json: String,
        agent_dir: String,
        workspace_dir: String,
        sessions_dir: String,
    ) -> String;
}

pub trait EnkiMemoryHandler: Send + Sync {
    fn record(
        &self,
        memory_name: String,
        session_id: String,
        user_msg: String,
        assistant_msg: String,
    );

    fn recall(
        &self,
        memory_name: String,
        session_id: String,
        query: String,
        max_entries: u32,
    ) -> Vec<EnkiMemoryEntry>;

    fn flush(&self, memory_name: String, session_id: String);

    fn consolidate(&self, memory_name: String, session_id: String);
}

pub trait EnkiLlmHandler: Send + Sync {
    fn complete(&self, model: String, messages_json: String, tools_json: String) -> String;
}

pub trait EnkiStepHandler: Send + Sync {
    fn on_step(&self, step: EnkiExecutionStep);
}

struct PythonTool {
    name: String,
    description: String,
    parameters: Value,
    handler: Arc<dyn EnkiToolHandler>,
}

struct PythonMemoryProvider {
    name: String,
    handler: Arc<dyn EnkiMemoryHandler>,
}

struct PythonMemoryRouter {
    provider_names: Vec<String>,
}

struct PythonLlmProvider {
    model: String,
    handler: Arc<dyn EnkiLlmHandler>,
}

struct BindingAgentRuntimeHandler {
    agent: Agent,
}

#[async_trait(?Send)]
impl Tool for PythonTool {
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
        self.handler.execute(
            self.name.clone(),
            args.to_string(),
            ctx.agent_dir.to_string_lossy().into_owned(),
            ctx.workspace_dir.to_string_lossy().into_owned(),
            ctx.sessions_dir.to_string_lossy().into_owned(),
        )
    }
}

fn build_tool_registry(
    tools: Vec<EnkiTool>,
    handler: Arc<dyn EnkiToolHandler>,
) -> Result<ToolRegistry, String> {
    let mut registry = ToolRegistry::new();

    for tool in tools {
        let parameters = serde_json::from_str::<Value>(&tool.parameters_json).map_err(|error| {
            format!("Invalid parameters_json for tool '{}': {error}", tool.name)
        })?;

        let name = tool.name;
        registry.insert(
            name.clone(),
            Box::new(PythonTool {
                name,
                description: tool.description,
                parameters,
                handler: handler.clone(),
            }),
        );
    }

    Ok(registry)
}

#[async_trait(?Send)]
impl MemoryProvider for PythonMemoryProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String> {
        self.handler.record(
            self.name.clone(),
            session_id.to_string(),
            user_msg.to_string(),
            assistant_msg.to_string(),
        );
        Ok(())
    }

    async fn recall(
        &self,
        session_id: &str,
        query: &str,
        max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        Ok(self
            .handler
            .recall(
                self.name.clone(),
                session_id.to_string(),
                query.to_string(),
                max_entries.min(u32::MAX as usize) as u32,
            )
            .into_iter()
            .map(MemoryEntry::from)
            .collect())
    }

    async fn flush(&self, session_id: &str) -> Result<(), String> {
        self.handler
            .flush(self.name.clone(), session_id.to_string());
        Ok(())
    }

    async fn consolidate(&mut self, session_id: &str) -> Result<(), String> {
        self.handler
            .consolidate(self.name.clone(), session_id.to_string());
        Ok(())
    }
}

#[async_trait(?Send)]
impl MemoryRouter for PythonMemoryRouter {
    async fn select(&self, _user_message: &str) -> MemoryStrategy {
        MemoryStrategy {
            active_providers: self.provider_names.clone(),
            max_context_entries: 6,
        }
    }
}

#[async_trait(?Send)]
impl RuntimeHandler for BindingAgentRuntimeHandler {
    async fn handle(
        &self,
        request: &RuntimeRequest,
        _session: &SessionContext,
    ) -> Result<String, String> {
        Ok(self.agent.run(&request.session_id, &request.content).await)
    }

    async fn handle_detailed(
        &self,
        request: &RuntimeRequest,
        _session: &SessionContext,
        on_step: Option<std::sync::Arc<dyn Fn(CoreExecutionStep) + Send + Sync>>,
    ) -> Result<(String, Vec<CoreExecutionStep>), String> {
        let result = self
            .agent
            .run_detailed(&request.session_id, &request.content, on_step)
            .await;
        Ok((result.content, result.steps))
    }
}

fn build_memory_manager(
    memories: Vec<EnkiMemoryModule>,
    handler: Arc<dyn EnkiMemoryHandler>,
) -> MemoryManager {
    let provider_names = memories
        .iter()
        .map(|memory| memory.name.clone())
        .collect::<Vec<_>>();
    let providers = memories
        .into_iter()
        .map(|memory| {
            Box::new(PythonMemoryProvider {
                name: memory.name,
                handler: handler.clone(),
            }) as Box<dyn MemoryProvider>
        })
        .collect();

    MemoryManager::new(Box::new(PythonMemoryRouter { provider_names }), providers)
}

fn error_run_result(message: impl Into<String>) -> CoreAgentRunResult {
    CoreAgentRunResult {
        content: message.into(),
        steps: Vec::new(),
    }
}

#[async_trait]
impl LlmProvider for PythonLlmProvider {
    async fn complete(
        &self,
        messages: &[ChatMessage],
        _config: &LlmConfig,
    ) -> core_next::llm::Result<LlmResponse> {
        self.complete_with_tools(messages, &[], &LlmConfig::default())
            .await
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
        let messages_json = serde_json::to_string(messages).map_err(|error| {
            core_next::llm::LlmError::Provider(format!("Failed to serialize messages: {error}"))
        })?;
        let tools_json = serde_json::to_string(tools).map_err(|error| {
            core_next::llm::LlmError::Provider(format!("Failed to serialize tools: {error}"))
        })?;

        let raw = self
            .handler
            .complete(self.model.clone(), messages_json, tools_json);

        if let Ok(response) = serde_json::from_str::<LlmResponse>(&raw) {
            return Ok(response);
        }

        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            return Ok(LlmResponse {
                content: value
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                usage: None,
                tool_calls: value
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|tool_call| tool_call.to_string())
                    .collect(),
                model: self.model.clone(),
                finish_reason: Some("stop".to_string()),
            });
        }

        Ok(LlmResponse {
            content: raw,
            usage: None,
            tool_calls: Vec::new(),
            model: self.model.clone(),
            finish_reason: Some("stop".to_string()),
        })
    }

    fn name(&self) -> &'static str {
        "python"
    }

    fn available_models(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

struct RunRequest {
    session_id: String,
    user_message: String,
    on_step: Option<std::sync::Arc<dyn Fn(CoreExecutionStep) + Send + Sync>>,
    reply_tx: tokio::sync::oneshot::Sender<CoreAgentRunResult>,
}

pub struct EnkiAgent {
    request_tx: Mutex<mpsc::Sender<RunRequest>>,
}

impl EnkiAgent {
    pub fn new(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
    ) -> Self {
        Self::from_registry(
            AgentDefinition {
                name,
                system_prompt_preamble,
                model,
                max_iterations: max_iterations as usize,
            },
            workspace_home,
        )
    }

    pub fn with_tools(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        handler: Box<dyn EnkiToolHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools(definition, workspace_home, tools, handler)
    }

    pub fn with_memory(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        memories: Vec<EnkiMemoryModule>,
        handler: Box<dyn EnkiMemoryHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_and_memory(
            definition,
            workspace_home,
            Vec::new(),
            None,
            memories,
            Some(handler),
        )
    }

    pub fn with_tools_and_memory(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        tool_handler: Box<dyn EnkiToolHandler>,
        memories: Vec<EnkiMemoryModule>,
        memory_handler: Box<dyn EnkiMemoryHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_and_memory(
            definition,
            workspace_home,
            tools,
            Some(tool_handler),
            memories,
            Some(memory_handler),
        )
    }

    pub fn with_llm(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        llm_handler: Box<dyn EnkiLlmHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_memory_and_llm(
            definition,
            workspace_home,
            Vec::new(),
            None,
            Vec::new(),
            None,
            Some(llm_handler),
        )
    }

    pub fn with_tools_and_llm(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        handler: Box<dyn EnkiToolHandler>,
        llm_handler: Box<dyn EnkiLlmHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_memory_and_llm(
            definition,
            workspace_home,
            tools,
            Some(handler),
            Vec::new(),
            None,
            Some(llm_handler),
        )
    }

    pub fn with_memory_and_llm(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        memories: Vec<EnkiMemoryModule>,
        handler: Box<dyn EnkiMemoryHandler>,
        llm_handler: Box<dyn EnkiLlmHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_memory_and_llm(
            definition,
            workspace_home,
            Vec::new(),
            None,
            memories,
            Some(handler),
            Some(llm_handler),
        )
    }

    pub fn with_tools_memory_and_llm(
        name: String,
        system_prompt_preamble: String,
        model: String,
        max_iterations: u32,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        tool_handler: Box<dyn EnkiToolHandler>,
        memories: Vec<EnkiMemoryModule>,
        memory_handler: Box<dyn EnkiMemoryHandler>,
        llm_handler: Box<dyn EnkiLlmHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools_memory_and_llm(
            definition,
            workspace_home,
            tools,
            Some(tool_handler),
            memories,
            Some(memory_handler),
            Some(llm_handler),
        )
    }

    fn from_registry(definition: AgentDefinition, workspace_home: Option<String>) -> Self {
        let workspace_home = workspace_home.map(PathBuf::from);
        let (request_tx, request_rx) = mpsc::channel::<RunRequest>();

        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let message =
                        format!("Initialization error: failed to create tokio runtime: {error}");
                    for request in request_rx {
                        let _ = request.reply_tx.send(error_run_result(message.clone()));
                    }
                    return;
                }
            };

            let agent =
                match runtime.block_on(Agent::with_definition_tool_registry_executor_and_workspace(
                    definition,
                    ToolRegistry::new(),
                    Box::new(RegistryToolExecutor),
                    workspace_home,
                )) {
                    Ok(agent) => agent,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        for request in request_rx {
                            let _ = request.reply_tx.send(error_run_result(message.clone()));
                        }
                        return;
                    }
                };
            let runtime_instance = Runtime::new(BindingAgentRuntimeHandler { agent });

            for request in request_rx {
                let response = match runtime.block_on(runtime_instance.process_detailed(
                    RuntimeRequest::new(&request.session_id, "binding-py", &request.user_message),
                    request.on_step,
                )) {
                    Ok(result) => CoreAgentRunResult {
                        content: result.response.content,
                        steps: result.steps,
                    },
                    Err(error) => error_run_result(error),
                };
                let _ = request.reply_tx.send(response);
            }
        });

        Self {
            request_tx: Mutex::new(request_tx),
        }
    }

    fn from_custom_tools(
        definition: AgentDefinition,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        handler: Box<dyn EnkiToolHandler>,
    ) -> Self {
        Self::from_custom_tools_and_memory(
            definition,
            workspace_home,
            tools,
            Some(handler),
            Vec::new(),
            None,
        )
    }

    fn from_custom_tools_and_memory(
        definition: AgentDefinition,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        tool_handler: Option<Box<dyn EnkiToolHandler>>,
        memories: Vec<EnkiMemoryModule>,
        memory_handler: Option<Box<dyn EnkiMemoryHandler>>,
    ) -> Self {
        Self::from_custom_tools_memory_and_llm(
            definition,
            workspace_home,
            tools,
            tool_handler,
            memories,
            memory_handler,
            None,
        )
    }

    fn from_custom_tools_memory_and_llm(
        definition: AgentDefinition,
        workspace_home: Option<String>,
        tools: Vec<EnkiTool>,
        tool_handler: Option<Box<dyn EnkiToolHandler>>,
        memories: Vec<EnkiMemoryModule>,
        memory_handler: Option<Box<dyn EnkiMemoryHandler>>,
        llm_handler: Option<Box<dyn EnkiLlmHandler>>,
    ) -> Self {
        let workspace_home = workspace_home.map(PathBuf::from);
        let (request_tx, request_rx) = mpsc::channel::<RunRequest>();

        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let message =
                        format!("Initialization error: failed to create tokio runtime: {error}");
                    for request in request_rx {
                        let _ = request.reply_tx.send(error_run_result(message.clone()));
                    }
                    return;
                }
            };

            let tool_registry = match tool_handler {
                Some(handler) => match build_tool_registry(tools, Arc::from(handler)) {
                    Ok(tool_registry) => tool_registry,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        for request in request_rx {
                            let _ = request.reply_tx.send(error_run_result(message.clone()));
                        }
                        return;
                    }
                },
                None => ToolRegistry::new(),
            };

            let memory =
                memory_handler.map(|handler| build_memory_manager(memories, Arc::from(handler)));
            let llm = llm_handler.map(|handler| {
                Box::new(PythonLlmProvider {
                    model: definition.model.clone(),
                    handler: Arc::from(handler),
                }) as Box<dyn LlmProvider>
            });

            let agent = match runtime.block_on(
                Agent::with_definition_tool_registry_executor_llm_and_workspace(
                    definition,
                    tool_registry,
                    Box::new(RegistryToolExecutor),
                    llm,
                    memory,
                    workspace_home,
                ),
            ) {
                Ok(agent) => agent,
                Err(error) => {
                    let message = format!("Initialization error: {error}");
                    for request in request_rx {
                        let _ = request.reply_tx.send(error_run_result(message.clone()));
                    }
                    return;
                }
            };
            let runtime_instance = Runtime::new(BindingAgentRuntimeHandler { agent });

            for request in request_rx {
                let response = match runtime.block_on(runtime_instance.process_detailed(
                    RuntimeRequest::new(&request.session_id, "binding-py", &request.user_message),
                    request.on_step,
                )) {
                    Ok(result) => CoreAgentRunResult {
                        content: result.response.content,
                        steps: result.steps,
                    },
                    Err(error) => error_run_result(error),
                };
                let _ = request.reply_tx.send(response);
            }
        });

        Self {
            request_tx: Mutex::new(request_tx),
        }
    }

    pub async fn run(&self, session_id: String, user_message: String) -> String {
        self.run_with_trace(session_id, user_message).await.output
    }

    pub async fn run_with_trace(
        &self,
        session_id: String,
        user_message: String,
    ) -> EnkiAgentRunResult {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = RunRequest {
            session_id,
            user_message,
            on_step: None,
            reply_tx,
        };

        let send_result = self
            .request_tx
            .lock()
            .map_err(|_| "Worker error: request mutex poisoned".to_string())
            .and_then(|sender| {
                sender
                    .send(request)
                    .map_err(|_| "Worker error: agent worker has stopped".to_string())
            });

        if let Err(message) = send_result {
            return EnkiAgentRunResult {
                output: message,
                steps: Vec::new(),
            };
        }

        reply_rx
            .await
            .map(EnkiAgentRunResult::from)
            .unwrap_or_else(|_| EnkiAgentRunResult {
                output: "Worker error: agent worker dropped reply channel".to_string(),
                steps: Vec::new(),
            })
    }

    pub async fn run_with_events(
        &self,
        session_id: String,
        user_message: String,
        handler: Box<dyn EnkiStepHandler>,
    ) -> EnkiAgentRunResult {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        
        let handler_arc: Arc<dyn EnkiStepHandler> = handler.into();
        let step_closure = Arc::new(move |step: CoreExecutionStep| {
            handler_arc.on_step(step.into());
        });

        let request = RunRequest {
            session_id,
            user_message,
            on_step: Some(step_closure),
            reply_tx,
        };

        let send_result = self
            .request_tx
            .lock()
            .map_err(|_| "Worker error: request mutex poisoned".to_string())
            .and_then(|sender| {
                sender
                    .send(request)
                    .map_err(|_| "Worker error: agent worker has stopped".to_string())
            });

        if let Err(message) = send_result {
            return EnkiAgentRunResult {
                output: message,
                steps: Vec::new(),
            };
        }

        reply_rx
            .await
            .map(EnkiAgentRunResult::from)
            .unwrap_or_else(|_| EnkiAgentRunResult {
                output: "Worker error: agent worker dropped reply channel".to_string(),
                steps: Vec::new(),
            })
    }
}

impl From<EnkiMemoryKind> for MemoryKind {
    fn from(value: EnkiMemoryKind) -> Self {
        match value {
            EnkiMemoryKind::RecentMessage => MemoryKind::RecentMessage,
            EnkiMemoryKind::Summary => MemoryKind::Summary,
            EnkiMemoryKind::Entity => MemoryKind::Entity,
            EnkiMemoryKind::Preference => MemoryKind::Preference,
        }
    }
}

impl From<MemoryKind> for EnkiMemoryKind {
    fn from(value: MemoryKind) -> Self {
        match value {
            MemoryKind::RecentMessage => EnkiMemoryKind::RecentMessage,
            MemoryKind::Summary => EnkiMemoryKind::Summary,
            MemoryKind::Entity => EnkiMemoryKind::Entity,
            MemoryKind::Preference => EnkiMemoryKind::Preference,
        }
    }
}

impl From<EnkiMemoryEntry> for MemoryEntry {
    fn from(value: EnkiMemoryEntry) -> Self {
        Self {
            key: value.key,
            content: value.content,
            kind: value.kind.into(),
            relevance: value.relevance,
            timestamp_ns: value.timestamp_ns as u128,
        }
    }
}

impl From<MemoryEntry> for EnkiMemoryEntry {
    fn from(value: MemoryEntry) -> Self {
        Self {
            key: value.key,
            content: value.content,
            kind: value.kind.into(),
            relevance: value.relevance,
            timestamp_ns: value.timestamp_ns.min(u64::MAX as u128) as u64,
        }
    }
}

impl From<CoreExecutionStep> for EnkiExecutionStep {
    fn from(value: CoreExecutionStep) -> Self {
        Self {
            index: value.index.min(u64::MAX as usize) as u64,
            phase: value.phase,
            kind: value.kind,
            detail: value.detail,
        }
    }
}

impl From<CoreAgentRunResult> for EnkiAgentRunResult {
    fn from(value: CoreAgentRunResult) -> Self {
        Self {
            output: value.content,
            steps: value.steps.into_iter().map(EnkiExecutionStep::from).collect(),
        }
    }
}

pub fn init_logger(level: String) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .try_init();
}

uniffi::include_scaffolding!("enki");
