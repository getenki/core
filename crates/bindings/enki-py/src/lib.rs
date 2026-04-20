use async_trait::async_trait;
use core_next::agent::{
    Agent, AgentDefinition, AgentExecutionContext, AgentRunResult as CoreAgentRunResult,
    CallbackAgentLoop, DefaultAgentLoop, ExecutionStep as CoreExecutionStep,
    ExternalAgentLoopHandler,
};
use core_next::llm::{
    ChatMessage, LlmConfig, LlmProvider, LlmResponse, ResponseStream, ToolDefinition,
};
use core_next::memory::{
    MemoryEntry, MemoryKind, MemoryManager, MemoryProvider, MemoryRouter, MemoryStrategy,
};
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry, WorkflowToolContext};
use core_next::workflow::{TaskTarget, WorkflowTaskResult};
use core_next::{
    TaskDefinition, WorkflowDefinition, WorkflowRequest, WorkflowRuntime, WorkflowTaskRunner,
};
use futures::stream;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::HashMap;
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

pub trait EnkiAgentLoopHandler: Send + Sync {
    fn run(&self, request_json: String) -> String;
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

struct PythonAgentLoop {
    handler: Arc<dyn EnkiAgentLoopHandler>,
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
    exec_ctx: AgentExecutionContext,
    on_step: Option<std::sync::Arc<dyn Fn(CoreExecutionStep) + Send + Sync>>,
    reply_tx: tokio::sync::oneshot::Sender<CoreAgentRunResult>,
}

impl ExternalAgentLoopHandler for PythonAgentLoop {
    fn run(&self, request_json: String) -> String {
        self.handler.run(request_json)
    }
}

enum AgentWorkerMessage {
    Run(RunRequest),
    SetLoopHandler {
        handler: Arc<dyn EnkiAgentLoopHandler>,
        reply_tx: mpsc::Sender<Result<(), String>>,
    },
    ClearLoopHandler {
        reply_tx: mpsc::Sender<Result<(), String>>,
    },
}

#[derive(Clone, Debug)]
struct WorkflowRegistration {
    agent_id: String,
    capabilities: Vec<String>,
}

struct BindingWorkflowTaskRunner {
    agents_by_id: HashMap<String, Arc<EnkiAgent>>,
    registrations: Vec<WorkflowRegistration>,
}

enum WorkflowRequestMessage {
    ListWorkflows {
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
    ListRuns {
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
    Inspect {
        run_id: String,
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
    Start {
        request_json: String,
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
    Resume {
        run_id: String,
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
    SubmitIntervention {
        run_id: String,
        intervention_id: String,
        response: Option<String>,
        reply_tx: tokio::sync::oneshot::Sender<String>,
    },
}

pub struct EnkiAgent {
    workflow_registration: Mutex<WorkflowRegistration>,
    request_tx: Mutex<mpsc::Sender<AgentWorkerMessage>>,
}

pub struct EnkiWorkflowRuntime {
    request_tx: Mutex<mpsc::Sender<WorkflowRequestMessage>>,
}

impl EnkiWorkflowRuntime {
    pub fn new(
        agents: Vec<Arc<EnkiAgent>>,
        tasks_json: Vec<String>,
        workflows_json: Vec<String>,
        workspace_home: Option<String>,
    ) -> Self {
        let request_tx = spawn_workflow_worker(agents, tasks_json, workflows_json, workspace_home);
        Self {
            request_tx: Mutex::new(request_tx),
        }
    }

    pub async fn list_workflows_json(&self) -> String {
        self.send_request(|reply_tx| WorkflowRequestMessage::ListWorkflows { reply_tx })
            .await
    }

    pub async fn list_runs_json(&self) -> String {
        self.send_request(|reply_tx| WorkflowRequestMessage::ListRuns { reply_tx })
            .await
    }

    pub async fn inspect_json(&self, run_id: String) -> String {
        self.send_request(move |reply_tx| WorkflowRequestMessage::Inspect { run_id, reply_tx })
            .await
    }

    pub async fn start_json(&self, request_json: String) -> String {
        self.send_request(move |reply_tx| WorkflowRequestMessage::Start {
            request_json,
            reply_tx,
        })
        .await
    }

    pub async fn resume_json(&self, run_id: String) -> String {
        self.send_request(move |reply_tx| WorkflowRequestMessage::Resume { run_id, reply_tx })
            .await
    }

    pub async fn submit_intervention_json(
        &self,
        run_id: String,
        intervention_id: String,
        response: Option<String>,
    ) -> String {
        self.send_request(move |reply_tx| WorkflowRequestMessage::SubmitIntervention {
            run_id,
            intervention_id,
            response,
            reply_tx,
        })
        .await
    }

    async fn send_request<F>(&self, build: F) -> String
    where
        F: FnOnce(tokio::sync::oneshot::Sender<String>) -> WorkflowRequestMessage,
    {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = build(reply_tx);

        let send_result = self
            .request_tx
            .lock()
            .map_err(|_| "Worker error: request mutex poisoned".to_string())
            .and_then(|sender| {
                sender
                    .send(request)
                    .map_err(|_| "Worker error: workflow worker has stopped".to_string())
            });

        if let Err(message) = send_result {
            return json_error_payload(&message);
        }

        reply_rx.await.unwrap_or_else(|_| {
            json_error_payload("Worker error: workflow worker dropped reply channel")
        })
    }
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

    pub fn configure_workflow(&self, agent_id: String, capabilities: Vec<String>) {
        if let Ok(mut registration) = self.workflow_registration.lock() {
            registration.agent_id = agent_id;
            registration.capabilities = capabilities;
        }
    }

    pub fn set_agent_loop_handler(&self, handler: Box<dyn EnkiAgentLoopHandler>) {
        let (reply_tx, reply_rx) = mpsc::channel();
        let request = AgentWorkerMessage::SetLoopHandler {
            handler: Arc::from(handler),
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

        if send_result.is_ok() {
            let _ = reply_rx.recv();
        }
    }

    pub fn clear_agent_loop_handler(&self) {
        let (reply_tx, reply_rx) = mpsc::channel();
        let request = AgentWorkerMessage::ClearLoopHandler { reply_tx };

        let send_result = self
            .request_tx
            .lock()
            .map_err(|_| "Worker error: request mutex poisoned".to_string())
            .and_then(|sender| {
                sender
                    .send(request)
                    .map_err(|_| "Worker error: agent worker has stopped".to_string())
            });

        if send_result.is_ok() {
            let _ = reply_rx.recv();
        }
    }

    fn from_registry(definition: AgentDefinition, workspace_home: Option<String>) -> Self {
        let workflow_registration = WorkflowRegistration {
            agent_id: definition.name.clone(),
            capabilities: Vec::new(),
        };
        let workspace_home = workspace_home.map(PathBuf::from);
        let (request_tx, request_rx) = mpsc::channel::<AgentWorkerMessage>();

        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let message =
                        format!("Initialization error: failed to create tokio runtime: {error}");
                    for message_request in request_rx {
                        match message_request {
                            AgentWorkerMessage::Run(request) => {
                                let _ = request.reply_tx.send(error_run_result(message.clone()));
                            }
                            AgentWorkerMessage::SetLoopHandler { reply_tx, .. }
                            | AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                                let _ = reply_tx.send(Err(message.clone()));
                            }
                        }
                    }
                    return;
                }
            };

            let mut agent = match runtime.block_on(
                Agent::with_definition_tool_registry_executor_and_workspace(
                    definition,
                    ToolRegistry::new(),
                    Box::new(RegistryToolExecutor),
                    workspace_home,
                ),
            ) {
                Ok(agent) => agent,
                Err(error) => {
                    let message = format!("Initialization error: {error}");
                    for message_request in request_rx {
                        match message_request {
                            AgentWorkerMessage::Run(request) => {
                                let _ = request.reply_tx.send(error_run_result(message.clone()));
                            }
                            AgentWorkerMessage::SetLoopHandler { reply_tx, .. }
                            | AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                                let _ = reply_tx.send(Err(message.clone()));
                            }
                        }
                    }
                    return;
                }
            };

            for message_request in request_rx {
                match message_request {
                    AgentWorkerMessage::Run(request) => {
                        let response = runtime.block_on(agent.run_detailed_with_context(
                            &request.session_id,
                            &request.user_message,
                            request.exec_ctx,
                            request.on_step,
                        ));
                        let _ = request.reply_tx.send(response);
                    }
                    AgentWorkerMessage::SetLoopHandler { handler, reply_tx } => {
                        agent.agent_loop =
                            Box::new(CallbackAgentLoop::new(Arc::new(PythonAgentLoop {
                                handler,
                            })));
                        let _ = reply_tx.send(Ok(()));
                    }
                    AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                        agent.agent_loop = Box::new(DefaultAgentLoop);
                        let _ = reply_tx.send(Ok(()));
                    }
                }
            }
        });

        Self {
            workflow_registration: Mutex::new(workflow_registration),
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
        let workflow_registration = WorkflowRegistration {
            agent_id: definition.name.clone(),
            capabilities: Vec::new(),
        };
        let workspace_home = workspace_home.map(PathBuf::from);
        let (request_tx, request_rx) = mpsc::channel::<AgentWorkerMessage>();

        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let message =
                        format!("Initialization error: failed to create tokio runtime: {error}");
                    for message_request in request_rx {
                        match message_request {
                            AgentWorkerMessage::Run(request) => {
                                let _ = request.reply_tx.send(error_run_result(message.clone()));
                            }
                            AgentWorkerMessage::SetLoopHandler { reply_tx, .. }
                            | AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                                let _ = reply_tx.send(Err(message.clone()));
                            }
                        }
                    }
                    return;
                }
            };

            let tool_registry = match tool_handler {
                Some(handler) => match build_tool_registry(tools, Arc::from(handler)) {
                    Ok(tool_registry) => tool_registry,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        for message_request in request_rx {
                            match message_request {
                                AgentWorkerMessage::Run(request) => {
                                    let _ =
                                        request.reply_tx.send(error_run_result(message.clone()));
                                }
                                AgentWorkerMessage::SetLoopHandler { reply_tx, .. }
                                | AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                                    let _ = reply_tx.send(Err(message.clone()));
                                }
                            }
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

            let mut agent = match runtime.block_on(
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
                    for message_request in request_rx {
                        match message_request {
                            AgentWorkerMessage::Run(request) => {
                                let _ = request.reply_tx.send(error_run_result(message.clone()));
                            }
                            AgentWorkerMessage::SetLoopHandler { reply_tx, .. }
                            | AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                                let _ = reply_tx.send(Err(message.clone()));
                            }
                        }
                    }
                    return;
                }
            };

            for message_request in request_rx {
                match message_request {
                    AgentWorkerMessage::Run(request) => {
                        let response = runtime.block_on(agent.run_detailed_with_context(
                            &request.session_id,
                            &request.user_message,
                            request.exec_ctx,
                            request.on_step,
                        ));
                        let _ = request.reply_tx.send(response);
                    }
                    AgentWorkerMessage::SetLoopHandler { handler, reply_tx } => {
                        agent.agent_loop =
                            Box::new(CallbackAgentLoop::new(Arc::new(PythonAgentLoop {
                                handler,
                            })));
                        let _ = reply_tx.send(Ok(()));
                    }
                    AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
                        agent.agent_loop = Box::new(DefaultAgentLoop);
                        let _ = reply_tx.send(Ok(()));
                    }
                }
            }
        });

        Self {
            workflow_registration: Mutex::new(workflow_registration),
            request_tx: Mutex::new(request_tx),
        }
    }

    pub async fn run(&self, session_id: String, user_message: String) -> String {
        self.run_core(
            session_id,
            user_message,
            AgentExecutionContext::default(),
            None,
        )
        .await
        .content
    }

    pub async fn run_with_trace(
        &self,
        session_id: String,
        user_message: String,
    ) -> EnkiAgentRunResult {
        EnkiAgentRunResult::from(
            self.run_core(
                session_id,
                user_message,
                AgentExecutionContext::default(),
                None,
            )
            .await,
        )
    }

    pub async fn run_with_events(
        &self,
        session_id: String,
        user_message: String,
        handler: Box<dyn EnkiStepHandler>,
    ) -> EnkiAgentRunResult {
        let handler_arc: Arc<dyn EnkiStepHandler> = handler.into();
        let step_closure = Arc::new(move |step: CoreExecutionStep| {
            handler_arc.on_step(step.into());
        });

        EnkiAgentRunResult::from(
            self.run_core(
                session_id,
                user_message,
                AgentExecutionContext::default(),
                Some(step_closure),
            )
            .await,
        )
    }

    fn workflow_registration(&self) -> Result<WorkflowRegistration, String> {
        self.workflow_registration
            .lock()
            .map(|registration| registration.clone())
            .map_err(|_| "Worker error: workflow registration mutex poisoned".to_string())
    }

    async fn run_core(
        &self,
        session_id: String,
        user_message: String,
        exec_ctx: AgentExecutionContext,
        on_step: Option<std::sync::Arc<dyn Fn(CoreExecutionStep) + Send + Sync>>,
    ) -> CoreAgentRunResult {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = AgentWorkerMessage::Run(RunRequest {
            session_id,
            user_message,
            exec_ctx,
            on_step,
            reply_tx,
        });

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
            return error_run_result(message);
        }

        reply_rx.await.unwrap_or_else(|_| {
            error_run_result("Worker error: agent worker dropped reply channel")
        })
    }
}

fn parse_json_value<T: DeserializeOwned>(json: &str, label: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid {label} JSON: {error}"))
}

fn to_json_string<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("Failed to serialize {label}: {error}"))
}

fn json_error_payload(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

fn workflow_task_failure_message(result: &CoreAgentRunResult) -> Option<String> {
    if result
        .steps
        .last()
        .is_some_and(|step| matches!(step.kind.as_str(), "failed" | "error"))
    {
        return Some(result.content.trim().to_string());
    }

    if result.content.trim_start().starts_with("LLM error:")
        || result
            .content
            .trim_start()
            .starts_with("Initialization error:")
    {
        return Some(result.content.trim().to_string());
    }

    None
}

fn spawn_workflow_worker(
    agents: Vec<Arc<EnkiAgent>>,
    tasks_json: Vec<String>,
    workflows_json: Vec<String>,
    workspace_home: Option<String>,
) -> mpsc::Sender<WorkflowRequestMessage> {
    let workspace_home = workspace_home.map(PathBuf::from);
    let (request_tx, request_rx) = mpsc::channel::<WorkflowRequestMessage>();

    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let message =
                    format!("Initialization error: failed to create tokio runtime: {error}");
                fail_workflow_requests(request_rx, &message);
                return;
            }
        };

        let mut workflow_builder = WorkflowRuntime::builder();

        if let Some(home) = workspace_home {
            workflow_builder = workflow_builder.with_workspace_home(home);
        }

        let mut registrations = Vec::with_capacity(agents.len());
        let mut agents_by_id = HashMap::with_capacity(agents.len());
        for agent in agents {
            let registration = match agent.workflow_registration() {
                Ok(registration) => registration,
                Err(error) => {
                    fail_workflow_requests(request_rx, &error);
                    return;
                }
            };

            if registration.agent_id.trim().is_empty() {
                fail_workflow_requests(
                    request_rx,
                    "Initialization error: workflow agent_id cannot be empty.",
                );
                return;
            }

            if agents_by_id
                .insert(registration.agent_id.clone(), agent.clone())
                .is_some()
            {
                let message = format!(
                    "Initialization error: duplicate workflow agent_id '{}'.",
                    registration.agent_id
                );
                fail_workflow_requests(request_rx, &message);
                return;
            }

            registrations.push(registration);
        }

        for task_json in tasks_json {
            let task = match parse_json_value::<TaskDefinition>(&task_json, "workflow task") {
                Ok(task) => task,
                Err(error) => {
                    let message = format!("Initialization error: {error}");
                    fail_workflow_requests(request_rx, &message);
                    return;
                }
            };
            workflow_builder = workflow_builder.add_task(task);
        }

        for workflow_json in workflows_json {
            let workflow =
                match parse_json_value::<WorkflowDefinition>(&workflow_json, "workflow definition")
                {
                    Ok(workflow) => workflow,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        fail_workflow_requests(request_rx, &message);
                        return;
                    }
                };
            workflow_builder = workflow_builder.add_workflow(workflow);
        }

        let task_runner: Arc<dyn WorkflowTaskRunner> = Arc::new(BindingWorkflowTaskRunner {
            agents_by_id,
            registrations,
        });
        let workflow_runtime: WorkflowRuntime =
            match runtime.block_on(workflow_builder.with_task_runner(task_runner).build()) {
                Ok(runtime_instance) => runtime_instance,
                Err(error) => {
                    let message = format!("Initialization error: {error}");
                    fail_workflow_requests(request_rx, &message);
                    return;
                }
            };

        for request in request_rx {
            match request {
                WorkflowRequestMessage::ListWorkflows { reply_tx } => {
                    let payload = workflow_runtime
                        .list_workflows()
                        .into_iter()
                        .map(|workflow| to_json_string(workflow, "workflow definition"))
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|workflows| {
                            to_json_string(&workflows, "workflow definition list")
                        })
                        .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
                WorkflowRequestMessage::ListRuns { reply_tx } => {
                    let payload = runtime
                        .block_on(workflow_runtime.list_runs())
                        .and_then(|runs| to_json_string(&runs, "workflow run list"))
                        .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
                WorkflowRequestMessage::Inspect { run_id, reply_tx } => {
                    let payload = runtime
                        .block_on(workflow_runtime.inspect(&run_id))
                        .and_then(|state| to_json_string(&state, "workflow run state"))
                        .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
                WorkflowRequestMessage::Start {
                    request_json,
                    reply_tx,
                } => {
                    let payload =
                        parse_json_value::<WorkflowRequest>(&request_json, "workflow request")
                            .and_then(|request| runtime.block_on(workflow_runtime.start(request)))
                            .and_then(|response| to_json_string(&response, "workflow response"))
                            .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
                WorkflowRequestMessage::Resume { run_id, reply_tx } => {
                    let payload = runtime
                        .block_on(workflow_runtime.resume(&run_id))
                        .and_then(|response| to_json_string(&response, "workflow response"))
                        .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
                WorkflowRequestMessage::SubmitIntervention {
                    run_id,
                    intervention_id,
                    response,
                    reply_tx,
                } => {
                    let payload = runtime
                        .block_on(workflow_runtime.submit_intervention(
                            &run_id,
                            &intervention_id,
                            response,
                        ))
                        .and_then(|state| to_json_string(&state, "workflow run state"))
                        .unwrap_or_else(|error| json_error_payload(&error));
                    let _ = reply_tx.send(payload);
                }
            }
        }
    });

    request_tx
}

#[async_trait(?Send)]
impl WorkflowTaskRunner for BindingWorkflowTaskRunner {
    async fn run_task(
        &self,
        target: &TaskTarget,
        metadata: &WorkflowToolContext,
        workspace_dir: &std::path::Path,
        prompt: &str,
    ) -> Result<WorkflowTaskResult, String> {
        let registration = match target {
            TaskTarget::AgentId(agent_id) => self
                .registrations
                .iter()
                .find(|registration| registration.agent_id == *agent_id)
                .ok_or_else(|| format!("Workflow target agent '{}' not found.", agent_id))?,
            TaskTarget::Capabilities(required) => {
                let mut matches = self
                    .registrations
                    .iter()
                    .filter(|registration| {
                        required.iter().all(|required| {
                            registration
                                .capabilities
                                .iter()
                                .any(|capability| capability == required)
                        })
                    })
                    .collect::<Vec<_>>();
                matches.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
                match matches.as_slice() {
                    [registration] => *registration,
                    [] => {
                        return Err(format!(
                            "No agent matched workflow capabilities: {}",
                            required.join(", ")
                        ));
                    }
                    _ => {
                        let matched_ids = matches
                            .into_iter()
                            .map(|registration| registration.agent_id.clone())
                            .collect::<Vec<_>>();
                        return Err(format!(
                            "Multiple agents matched workflow capabilities {}: {}",
                            required.join(", "),
                            matched_ids.join(", ")
                        ));
                    }
                }
            }
        };

        let agent = self
            .agents_by_id
            .get(&registration.agent_id)
            .ok_or_else(|| {
                format!(
                    "Workflow target agent '{}' not found.",
                    registration.agent_id
                )
            })?;
        let session_id = format!(
            "wf-{}-{}-attempt-{}",
            metadata.run_id, metadata.node_id, metadata.attempt
        );
        let result = agent
            .run_core(
                session_id.clone(),
                prompt.to_string(),
                AgentExecutionContext {
                    workspace_dir: Some(workspace_dir.to_path_buf()),
                    workflow: Some(metadata.clone()),
                },
                None,
            )
            .await;

        if let Some(error) = workflow_task_failure_message(&result) {
            return Err(error);
        }

        Ok(WorkflowTaskResult {
            content: result.content.clone(),
            value: serde_json::json!({
                "content": result.content,
                "agent_id": registration.agent_id.clone(),
                "session_id": session_id,
                "attempt": metadata.attempt,
            }),
            agent_id: registration.agent_id.clone(),
            steps: result.steps,
        })
    }
}

fn fail_workflow_requests(request_rx: mpsc::Receiver<WorkflowRequestMessage>, message: &str) {
    let payload = json_error_payload(message);
    for request in request_rx {
        match request {
            WorkflowRequestMessage::ListWorkflows { reply_tx }
            | WorkflowRequestMessage::ListRuns { reply_tx }
            | WorkflowRequestMessage::Inspect { reply_tx, .. }
            | WorkflowRequestMessage::Start { reply_tx, .. }
            | WorkflowRequestMessage::Resume { reply_tx, .. }
            | WorkflowRequestMessage::SubmitIntervention { reply_tx, .. } => {
                let _ = reply_tx.send(payload.clone());
            }
        }
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
            steps: value
                .steps
                .into_iter()
                .map(EnkiExecutionStep::from)
                .collect(),
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
