#![deny(clippy::all)]

use async_trait::async_trait;
use core_next::agent::{
  Agent as CoreAgent, AgentDefinition, AgentExecutionContext, AgentRunResult as CoreAgentRunResult,
  CallbackAgentLoop, DefaultAgentLoop, ExecutionStep as CoreExecutionStep,
  ExternalAgentLoopHandler,
};
use core_next::memory::{
  MemoryEntry, MemoryKind, MemoryManager, MemoryProvider, MemoryRouter, MemoryStrategy,
};
use core_next::registry::{AgentCard, AgentStatus, DiscoverQuery};
use core_next::runtime::MultiAgentRuntime;
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry, WorkflowToolContext};
use core_next::workflow::{TaskTarget, WorkflowTaskResult};
use core_next::{
  TaskDefinition, WorkflowDefinition, WorkflowRequest, WorkflowRuntime, WorkflowTaskRunner,
};
use napi::bindgen_prelude::{ClassInstance, FnArgs, Function, JsObjectValue, Object, Unknown};
use napi::threadsafe_function::{ThreadsafeCallContext, ThreadsafeFunction};
use napi::{Env, JSON, JsValue};
use napi_derive::napi;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

#[napi]
pub fn init_logger(level: String) {
  let _ = tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
    )
    .try_init();
}

const DEFAULT_NAME: &str = "Personal Assistant";
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful Personal Assistant agent.";
const DEFAULT_MAX_ITERATIONS: u32 = 20;
const CUSTOM_AGENTIC_LOOP_START: &str = "<enki:agentic-loop>";
const CUSTOM_AGENTIC_LOOP_END: &str = "</enki:agentic-loop>";

type ToolHandler =
  ThreadsafeFunction<ToolInvocation, String, FnArgs<(String, String)>, napi::Status, false>;
type SharedToolHandler = ThreadsafeFunction<
  ToolInvocation,
  String,
  FnArgs<(String, String, String, String, String)>,
  napi::Status,
  false,
>;
type MemoryRecordHandler = ThreadsafeFunction<
  MemoryRecordInvocation,
  (),
  FnArgs<(String, String, String, String)>,
  napi::Status,
  false,
>;
type MemoryRecallHandler = ThreadsafeFunction<
  MemoryRecallInvocation,
  Vec<JsMemoryEntry>,
  FnArgs<(String, String, String, u32)>,
  napi::Status,
  false,
>;
type MemoryFlushHandler =
  ThreadsafeFunction<MemorySessionInvocation, (), FnArgs<(String, String)>, napi::Status, false>;
type MemoryConsolidateHandler =
  ThreadsafeFunction<MemorySessionInvocation, (), FnArgs<(String, String)>, napi::Status, false>;
type SharedToolCallback<'scope> =
  Function<'scope, FnArgs<(String, String, String, String, String)>, String>;
type RecordCallback<'scope> = Function<'scope, FnArgs<(String, String, String, String)>, ()>;
type RecallCallback<'scope> =
  Function<'scope, FnArgs<(String, String, String, u32)>, Vec<JsMemoryEntry>>;
type SessionCallback<'scope> = Function<'scope, FnArgs<(String, String)>, ()>;
type LoopHandler = ThreadsafeFunction<String, String, FnArgs<(String,)>, napi::Status, false>;
type LoopCallback<'scope> = Function<'scope, FnArgs<(String,)>, String>;

struct RunRequest {
  session_id: String,
  user_message: String,
  exec_ctx: AgentExecutionContext,
  reply_tx: mpsc::Sender<CoreAgentRunResult>,
}

enum AgentWorkerMessage {
  Run(RunRequest),
  SetLoopHandler {
    handler: Arc<dyn ExternalAgentLoopHandler>,
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

struct AgentHandle {
  workflow_registration: Mutex<WorkflowRegistration>,
  request_tx: Mutex<mpsc::Sender<AgentWorkerMessage>>,
}

struct MultiAgentHandle {
  request_tx: Mutex<mpsc::Sender<MultiAgentRequest>>,
}

struct WorkflowHandle {
  request_tx: Mutex<mpsc::Sender<WorkflowBindingRequest>>,
}

struct BindingWorkflowTaskRunner {
  agents_by_id: HashMap<String, Arc<AgentHandle>>,
  registrations: Vec<WorkflowRegistration>,
}

struct JsTool {
  name: String,
  description: String,
  parameters: Value,
  handler: Arc<JsToolHandler>,
}

struct JsMemoryProvider {
  name: String,
  handlers: Arc<JsMemoryHandlers>,
}

struct JsMemoryRouter {
  provider_names: Vec<String>,
}

struct JsMemoryHandlers {
  record: MemoryRecordHandler,
  recall: MemoryRecallHandler,
  flush: MemoryFlushHandler,
  consolidate: MemoryConsolidateHandler,
}

struct JsAgentLoopHandler {
  handler: LoopHandler,
}

struct WorkerConfig {
  tools: Vec<ResolvedToolDefinition>,
  memories: Vec<JsMemoryModule>,
  memory_handlers: Option<Arc<JsMemoryHandlers>>,
}

struct BuildOptions {
  name: Option<String>,
  system_prompt_preamble: Option<String>,
  agentic_loop: Option<String>,
  model: Option<String>,
  max_iterations: Option<u32>,
  workspace_home: Option<String>,
}

struct MemoryFactoryOptions<'scope> {
  build: BuildOptions,
  memories: Vec<JsMemoryModule>,
  handlers: JsMemoryCallbackSet<'scope>,
}

struct ToolAndMemoryFactoryOptions<'scope> {
  build: BuildOptions,
  tools: Vec<Object<'scope>>,
  tool_handler: Option<SharedToolCallback<'scope>>,
  memories: Vec<JsMemoryModule>,
  handlers: JsMemoryCallbackSet<'scope>,
}

struct JsMemoryCallbackSet<'scope> {
  record: RecordCallback<'scope>,
  recall: RecallCallback<'scope>,
  flush: SessionCallback<'scope>,
  consolidate: SessionCallback<'scope>,
}

struct ToolInvocation {
  tool_name: String,
  input_json: String,
  agent_dir: String,
  workspace_dir: String,
  sessions_dir: String,
}

struct MemoryRecordInvocation {
  memory_name: String,
  session_id: String,
  user_msg: String,
  assistant_msg: String,
}

struct MemoryRecallInvocation {
  memory_name: String,
  session_id: String,
  query: String,
  max_entries: u32,
}

struct MemorySessionInvocation {
  memory_name: String,
  session_id: String,
}

enum MultiAgentRequest {
  Process {
    agent_id: String,
    session_id: String,
    user_message: String,
    reply_tx: mpsc::Sender<Result<CoreAgentRunResult, String>>,
  },
  Registry {
    reply_tx: mpsc::Sender<Result<Vec<JsAgentCard>, String>>,
  },
  Discover {
    capability: Option<String>,
    status: Option<JsAgentStatus>,
    reply_tx: mpsc::Sender<Result<Vec<JsAgentCard>, String>>,
  },
}

enum WorkflowBindingRequest {
  ListWorkflows {
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
  ListRuns {
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
  Inspect {
    run_id: String,
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
  Start {
    request_json: String,
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
  Resume {
    run_id: String,
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
  SubmitIntervention {
    run_id: String,
    intervention_id: String,
    response: Option<String>,
    reply_tx: mpsc::Sender<Result<String, String>>,
  },
}

#[napi(string_enum)]
pub enum JsMemoryKind {
  RecentMessage,
  Summary,
  Entity,
  Preference,
}

#[napi(string_enum)]
pub enum JsAgentStatus {
  Online,
  Busy,
  Offline,
}

struct ResolvedToolDefinition {
  name: String,
  description: String,
  parameters: Value,
  handler: Arc<JsToolHandler>,
}

enum JsToolHandler {
  PerTool(ToolHandler),
  Shared(SharedToolHandler),
}

#[napi(object)]
pub struct JsMultiAgentMember {
  pub agent_id: String,
  pub name: String,
  pub system_prompt_preamble: Option<String>,
  pub model: Option<String>,
  pub max_iterations: Option<u32>,
  pub capabilities: Vec<String>,
}

#[napi(object)]
pub struct JsAgentCard {
  pub agent_id: String,
  pub name: String,
  pub description: String,
  pub capabilities: Vec<String>,
  pub status: JsAgentStatus,
}

#[napi(object)]
pub struct JsMemoryModule {
  pub name: String,
}

#[napi(object)]
pub struct JsMemoryEntry {
  pub key: String,
  pub content: String,
  pub kind: JsMemoryKind,
  pub relevance: f64,
  pub timestamp_ns: String,
}

#[napi(object)]
pub struct JsExecutionStep {
  pub index: u32,
  pub phase: String,
  pub kind: String,
  pub detail: String,
}

#[napi(object)]
pub struct JsAgentRunResult {
  pub output: String,
  pub steps: Vec<JsExecutionStep>,
}

#[napi(js_name = "NativeEnkiAgent")]
pub struct NativeEnkiAgent {
  inner: Arc<AgentHandle>,
}

#[napi(js_name = "NativeMultiAgentRuntime")]
pub struct NativeMultiAgentRuntime {
  inner: Arc<MultiAgentHandle>,
}

#[napi(js_name = "NativeWorkflowRuntime")]
pub struct NativeWorkflowRuntime {
  inner: Arc<WorkflowHandle>,
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
    let invocation = ToolInvocation {
      tool_name: self.name.clone(),
      input_json: args.to_string(),
      agent_dir: ctx.agent_dir.to_string_lossy().into_owned(),
      workspace_dir: ctx.workspace_dir.to_string_lossy().into_owned(),
      sessions_dir: ctx.sessions_dir.to_string_lossy().into_owned(),
    };

    match self.handler.as_ref() {
      JsToolHandler::PerTool(handler) => handler
        .call_async(invocation)
        .await
        .unwrap_or_else(|error| format!("Error: failed to execute tool '{}': {error}", self.name)),
      JsToolHandler::Shared(handler) => handler
        .call_async(invocation)
        .await
        .unwrap_or_else(|error| format!("Error: failed to execute tool '{}': {error}", self.name)),
    }
  }
}

#[async_trait(?Send)]
impl MemoryProvider for JsMemoryProvider {
  fn name(&self) -> &str {
    &self.name
  }

  async fn record(
    &mut self,
    session_id: &str,
    user_msg: &str,
    assistant_msg: &str,
  ) -> Result<(), String> {
    self
      .handlers
      .record
      .call_async(MemoryRecordInvocation {
        memory_name: self.name.clone(),
        session_id: session_id.to_string(),
        user_msg: user_msg.to_string(),
        assistant_msg: assistant_msg.to_string(),
      })
      .await
      .map_err(|error| format!("Failed to record memory '{}': {error}", self.name))
  }

  async fn recall(
    &self,
    session_id: &str,
    query: &str,
    max_entries: usize,
  ) -> Result<Vec<MemoryEntry>, String> {
    self
      .handlers
      .recall
      .call_async(MemoryRecallInvocation {
        memory_name: self.name.clone(),
        session_id: session_id.to_string(),
        query: query.to_string(),
        max_entries: max_entries.min(u32::MAX as usize) as u32,
      })
      .await
      .map(|entries| entries.into_iter().map(MemoryEntry::from).collect())
      .map_err(|error| format!("Failed to recall memory '{}': {error}", self.name))
  }

  async fn flush(&self, session_id: &str) -> Result<(), String> {
    self
      .handlers
      .flush
      .call_async(MemorySessionInvocation {
        memory_name: self.name.clone(),
        session_id: session_id.to_string(),
      })
      .await
      .map_err(|error| format!("Failed to flush memory '{}': {error}", self.name))
  }

  async fn consolidate(&mut self, session_id: &str) -> Result<(), String> {
    self
      .handlers
      .consolidate
      .call_async(MemorySessionInvocation {
        memory_name: self.name.clone(),
        session_id: session_id.to_string(),
      })
      .await
      .map_err(|error| format!("Failed to consolidate memory '{}': {error}", self.name))
  }
}

#[async_trait(?Send)]
impl MemoryRouter for JsMemoryRouter {
  async fn select(&self, _user_message: &str) -> MemoryStrategy {
    MemoryStrategy {
      active_providers: self.provider_names.clone(),
      max_context_entries: 6,
    }
  }
}

impl ExternalAgentLoopHandler for JsAgentLoopHandler {
  fn run(&self, request_json: String) -> String {
    futures::executor::block_on(self.handler.call_async(request_json)).unwrap_or_else(|error| {
      json!({
        "content": format!("Custom agent loop error: {error}"),
        "steps": [],
      })
      .to_string()
    })
  }
}

#[napi]
impl NativeWorkflowRuntime {
  #[napi(constructor)]
  pub fn new(
    agents: Vec<ClassInstance<'_, NativeEnkiAgent>>,
    tasks_json: Vec<String>,
    workflows_json: Vec<String>,
    workspace_home: Option<String>,
  ) -> napi::Result<Self> {
    let agent_handles = agents
      .into_iter()
      .map(|agent| Arc::clone(&agent.inner))
      .collect();
    let request_tx =
      spawn_workflow_worker(agent_handles, tasks_json, workflows_json, workspace_home)?;

    Ok(Self {
      inner: Arc::new(WorkflowHandle {
        request_tx: Mutex::new(request_tx),
      }),
    })
  }

  #[napi(js_name = "listWorkflowsJson")]
  pub async fn list_workflows_json(&self) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.list_workflows_json())
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi(js_name = "listRunsJson")]
  pub async fn list_runs_json(&self) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.list_runs_json())
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi(js_name = "inspectJson")]
  pub async fn inspect_json(&self, run_id: String) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.inspect_json(run_id))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi(js_name = "startJson")]
  pub async fn start_json(&self, request_json: String) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.start_json(request_json))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi(js_name = "resumeJson")]
  pub async fn resume_json(&self, run_id: String) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.resume_json(run_id))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi(js_name = "submitInterventionJson")]
  pub async fn submit_intervention_json(
    &self,
    run_id: String,
    intervention_id: String,
    response: Option<String>,
  ) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || {
      inner.submit_intervention_json(run_id, intervention_id, response)
    })
    .await
    .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }
}

#[napi]
impl NativeEnkiAgent {
  #[napi(constructor)]
  pub fn new(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    agentic_loop: Option<String>,
  ) -> napi::Result<Self> {
    Self::build(
      name,
      system_prompt_preamble,
      agentic_loop,
      model,
      max_iterations,
      workspace_home,
      WorkerConfig {
        tools: Vec::new(),
        memories: Vec::new(),
        memory_handlers: None,
      },
    )
  }

  #[napi(factory, js_name = "withTools")]
  #[allow(clippy::too_many_arguments)]
  pub fn with_tools(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    tools: Vec<Object<'_>>,
    tool_handler: Option<SharedToolCallback<'_>>,
    agentic_loop: Option<String>,
  ) -> napi::Result<Self> {
    let tools = resolve_tool_definitions(tools, tool_handler)?;

    Self::build(
      name,
      system_prompt_preamble,
      agentic_loop,
      model,
      max_iterations,
      workspace_home,
      WorkerConfig {
        tools,
        memories: Vec::new(),
        memory_handlers: None,
      },
    )
  }

  #[napi(factory, js_name = "withMemory")]
  #[allow(clippy::too_many_arguments)]
  pub fn with_memory(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    memories: Vec<JsMemoryModule>,
    record_handler: RecordCallback<'_>,
    recall_handler: RecallCallback<'_>,
    flush_handler: SessionCallback<'_>,
    consolidate_handler: SessionCallback<'_>,
    agentic_loop: Option<String>,
  ) -> napi::Result<Self> {
    Self::build_with_memory(MemoryFactoryOptions {
      build: BuildOptions {
        name,
        system_prompt_preamble,
        agentic_loop,
        model,
        max_iterations,
        workspace_home,
      },
      memories,
      handlers: JsMemoryCallbackSet {
        record: record_handler,
        recall: recall_handler,
        flush: flush_handler,
        consolidate: consolidate_handler,
      },
    })
  }

  #[napi(factory, js_name = "withToolsAndMemory")]
  #[allow(clippy::too_many_arguments)]
  pub fn with_tools_and_memory(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    tools: Vec<Object<'_>>,
    tool_handler: Option<SharedToolCallback<'_>>,
    memories: Vec<JsMemoryModule>,
    record_handler: RecordCallback<'_>,
    recall_handler: RecallCallback<'_>,
    flush_handler: SessionCallback<'_>,
    consolidate_handler: SessionCallback<'_>,
    agentic_loop: Option<String>,
  ) -> napi::Result<Self> {
    Self::build_with_tools_and_memory(ToolAndMemoryFactoryOptions {
      build: BuildOptions {
        name,
        system_prompt_preamble,
        agentic_loop,
        model,
        max_iterations,
        workspace_home,
      },
      tools,
      tool_handler,
      memories,
      handlers: JsMemoryCallbackSet {
        record: record_handler,
        recall: recall_handler,
        flush: flush_handler,
        consolidate: consolidate_handler,
      },
    })
  }

  #[napi]
  pub async fn run(&self, session_id: String, user_message: String) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.run(session_id, user_message))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
      .map(|result| result.content)
  }

  #[napi(js_name = "runWithTrace")]
  pub async fn run_with_trace(
    &self,
    session_id: String,
    user_message: String,
  ) -> napi::Result<JsAgentRunResult> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.run(session_id, user_message))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
      .map(JsAgentRunResult::from)
  }

  #[napi(js_name = "configureWorkflow")]
  pub fn configure_workflow(
    &self,
    agent_id: String,
    capabilities: Vec<String>,
  ) -> napi::Result<()> {
    self.inner.configure_workflow(agent_id, capabilities)
  }

  #[napi(js_name = "setAgentLoopHandler")]
  pub fn set_agent_loop_handler(&self, handler: LoopCallback<'_>) -> napi::Result<()> {
    self
      .inner
      .set_agent_loop_handler(build_loop_handler(handler)?)
  }

  #[napi(js_name = "clearAgentLoopHandler")]
  pub fn clear_agent_loop_handler(&self) -> napi::Result<()> {
    self.inner.clear_agent_loop_handler()
  }
}

#[napi]
impl NativeMultiAgentRuntime {
  #[napi(constructor)]
  pub fn new(
    members: Vec<JsMultiAgentMember>,
    workspace_home: Option<String>,
  ) -> napi::Result<Self> {
    let request_tx = spawn_multi_agent_worker(members, workspace_home)?;

    Ok(Self {
      inner: Arc::new(MultiAgentHandle {
        request_tx: Mutex::new(request_tx),
      }),
    })
  }

  #[napi]
  pub async fn process(
    &self,
    agent_id: String,
    session_id: String,
    user_message: String,
  ) -> napi::Result<String> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.process(agent_id, session_id, user_message))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
      .map(|result| result.content)
  }

  #[napi(js_name = "processWithTrace")]
  pub async fn process_with_trace(
    &self,
    agent_id: String,
    session_id: String,
    user_message: String,
  ) -> napi::Result<JsAgentRunResult> {
    let inner = Arc::clone(&self.inner);
    let result =
      tokio::task::spawn_blocking(move || inner.process(agent_id, session_id, user_message))
        .await
        .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))??;
    Ok(JsAgentRunResult::from(result))
  }

  #[napi]
  pub async fn registry(&self) -> napi::Result<Vec<JsAgentCard>> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.registry())
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }

  #[napi]
  pub async fn discover(
    &self,
    capability: Option<String>,
    status: Option<JsAgentStatus>,
  ) -> napi::Result<Vec<JsAgentCard>> {
    let inner = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || inner.discover(capability, status))
      .await
      .map_err(|error| napi::Error::from_reason(format!("Worker join error: {error}")))?
  }
}

impl NativeEnkiAgent {
  fn build_from_options(build: BuildOptions, worker_config: WorkerConfig) -> napi::Result<Self> {
    Self::build(
      build.name,
      build.system_prompt_preamble,
      build.agentic_loop,
      build.model,
      build.max_iterations,
      build.workspace_home,
      worker_config,
    )
  }

  fn build_with_memory(options: MemoryFactoryOptions<'_>) -> napi::Result<Self> {
    Self::build_from_options(
      options.build,
      WorkerConfig {
        tools: Vec::new(),
        memories: options.memories,
        memory_handlers: Some(Arc::new(build_memory_handlers(options.handlers)?)),
      },
    )
  }

  fn build_with_tools_and_memory(options: ToolAndMemoryFactoryOptions<'_>) -> napi::Result<Self> {
    let tools = resolve_tool_definitions(options.tools, options.tool_handler)?;

    Self::build_from_options(
      options.build,
      WorkerConfig {
        tools,
        memories: options.memories,
        memory_handlers: Some(Arc::new(build_memory_handlers(options.handlers)?)),
      },
    )
  }

  fn build(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    agentic_loop: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    worker_config: WorkerConfig,
  ) -> napi::Result<Self> {
    let definition = build_definition(
      name,
      system_prompt_preamble,
      agentic_loop,
      model,
      max_iterations,
    );
    let workflow_agent_id = definition.name.clone();
    let request_tx = spawn_agent_worker(definition, workspace_home, worker_config)?;

    Ok(Self {
      inner: Arc::new(AgentHandle {
        workflow_registration: Mutex::new(WorkflowRegistration {
          agent_id: workflow_agent_id,
          capabilities: Vec::new(),
        }),
        request_tx: Mutex::new(request_tx),
      }),
    })
  }
}

impl AgentHandle {
  fn configure_workflow(&self, agent_id: String, capabilities: Vec<String>) -> napi::Result<()> {
    let mut registration = self.workflow_registration.lock().map_err(|_| {
      napi::Error::from_reason("Worker error: workflow registration mutex poisoned".to_string())
    })?;

    registration.agent_id = agent_id;
    registration.capabilities = capabilities;
    Ok(())
  }

  fn workflow_registration(&self) -> napi::Result<WorkflowRegistration> {
    self
      .workflow_registration
      .lock()
      .map(|registration| registration.clone())
      .map_err(|_| {
        napi::Error::from_reason("Worker error: workflow registration mutex poisoned".to_string())
      })
  }

  fn run(&self, session_id: String, user_message: String) -> napi::Result<CoreAgentRunResult> {
    self.run_core(session_id, user_message, AgentExecutionContext::default())
  }

  fn set_agent_loop_handler(&self, handler: LoopHandler) -> napi::Result<()> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = AgentWorkerMessage::SetLoopHandler {
      handler: Arc::new(JsAgentLoopHandler { handler }),
      reply_tx,
    };

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn clear_agent_loop_handler(&self) -> napi::Result<()> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = AgentWorkerMessage::ClearLoopHandler { reply_tx };

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn run_core(
    &self,
    session_id: String,
    user_message: String,
    exec_ctx: AgentExecutionContext,
  ) -> napi::Result<CoreAgentRunResult> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = AgentWorkerMessage::Run(RunRequest {
      session_id,
      user_message,
      exec_ctx,
      reply_tx,
    });

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))
  }
}

impl WorkflowHandle {
  fn list_workflows_json(&self) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::ListWorkflows { reply_tx };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn list_runs_json(&self) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::ListRuns { reply_tx };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn inspect_json(&self, run_id: String) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::Inspect { run_id, reply_tx };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn start_json(&self, request_json: String) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::Start {
      request_json,
      reply_tx,
    };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn resume_json(&self, run_id: String) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::Resume { run_id, reply_tx };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn submit_intervention_json(
    &self,
    run_id: String,
    intervention_id: String,
    response: Option<String>,
  ) -> napi::Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = WorkflowBindingRequest::SubmitIntervention {
      run_id,
      intervention_id,
      response,
      reply_tx,
    };
    self.send(request)?;
    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn send(&self, request: WorkflowBindingRequest) -> napi::Result<()> {
    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: workflow worker has stopped".to_string())
    })
  }
}

impl MultiAgentHandle {
  fn process(
    &self,
    agent_id: String,
    session_id: String,
    user_message: String,
  ) -> napi::Result<CoreAgentRunResult> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = MultiAgentRequest::Process {
      agent_id,
      session_id,
      user_message,
      reply_tx,
    };

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: multi-agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn registry(&self) -> napi::Result<Vec<JsAgentCard>> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = MultiAgentRequest::Registry { reply_tx };

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: multi-agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }

  fn discover(
    &self,
    capability: Option<String>,
    status: Option<JsAgentStatus>,
  ) -> napi::Result<Vec<JsAgentCard>> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = MultiAgentRequest::Discover {
      capability,
      status,
      reply_tx,
    };

    let sender = self
      .request_tx
      .lock()
      .map_err(|_| napi::Error::from_reason("Worker error: request mutex poisoned".to_string()))?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: multi-agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))?
      .map_err(napi::Error::from_reason)
  }
}

fn build_definition(
  name: Option<String>,
  system_prompt_preamble: Option<String>,
  agentic_loop: Option<String>,
  model: Option<String>,
  max_iterations: Option<u32>,
) -> AgentDefinition {
  AgentDefinition {
    name: name.unwrap_or_else(|| DEFAULT_NAME.to_string()),
    system_prompt_preamble: compose_system_prompt_preamble(
      system_prompt_preamble.unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
      agentic_loop,
    ),
    model: model.unwrap_or_default(),
    max_iterations: max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS).max(1) as usize,
  }
}

fn build_multi_agent_definition(
  member: JsMultiAgentMember,
) -> (String, AgentDefinition, Vec<String>) {
  (
    member.agent_id,
    build_definition(
      Some(member.name),
      member.system_prompt_preamble,
      None,
      member.model,
      member.max_iterations,
    ),
    member.capabilities,
  )
}

fn compose_system_prompt_preamble(
  system_prompt_preamble: String,
  agentic_loop: Option<String>,
) -> String {
  let Some(agentic_loop) = agentic_loop.map(|value| value.trim().to_string()) else {
    return system_prompt_preamble;
  };

  if agentic_loop.is_empty() {
    return system_prompt_preamble;
  }

  format!(
    "{system_prompt_preamble}\n{CUSTOM_AGENTIC_LOOP_START}\n{agentic_loop}\n{CUSTOM_AGENTIC_LOOP_END}"
  )
}

fn parse_json_value<T: DeserializeOwned>(json: &str, label: &str) -> Result<T, String> {
  serde_json::from_str(json).map_err(|error| format!("Invalid {label} JSON: {error}"))
}

fn to_json_string<T: Serialize>(value: &T, label: &str) -> Result<String, String> {
  serde_json::to_string(value).map_err(|error| format!("Failed to serialize {label}: {error}"))
}

fn build_tool_handler(
  tool_handler: Function<'_, FnArgs<(String, String)>, String>,
) -> napi::Result<ToolHandler> {
  tool_handler.build_threadsafe_function().build_callback(
    |ctx: ThreadsafeCallContext<ToolInvocation>| {
      let context_json = json!({
        "agentDir": ctx.value.agent_dir,
        "workspaceDir": ctx.value.workspace_dir,
        "sessionsDir": ctx.value.sessions_dir,
      })
      .to_string();
      Ok(FnArgs::from((ctx.value.input_json, context_json)))
    },
  )
}

fn build_shared_tool_handler(
  tool_handler: SharedToolCallback<'_>,
) -> napi::Result<SharedToolHandler> {
  tool_handler.build_threadsafe_function().build_callback(
    |ctx: ThreadsafeCallContext<ToolInvocation>| {
      Ok(FnArgs::from((
        ctx.value.tool_name,
        ctx.value.input_json,
        ctx.value.agent_dir,
        ctx.value.workspace_dir,
        ctx.value.sessions_dir,
      )))
    },
  )
}

fn build_memory_handlers(callbacks: JsMemoryCallbackSet<'_>) -> napi::Result<JsMemoryHandlers> {
  Ok(JsMemoryHandlers {
    record: callbacks
      .record
      .build_threadsafe_function()
      .build_callback(|ctx: ThreadsafeCallContext<MemoryRecordInvocation>| {
        Ok(FnArgs::from((
          ctx.value.memory_name,
          ctx.value.session_id,
          ctx.value.user_msg,
          ctx.value.assistant_msg,
        )))
      })?,
    recall: callbacks
      .recall
      .build_threadsafe_function()
      .build_callback(|ctx: ThreadsafeCallContext<MemoryRecallInvocation>| {
        Ok(FnArgs::from((
          ctx.value.memory_name,
          ctx.value.session_id,
          ctx.value.query,
          ctx.value.max_entries,
        )))
      })?,
    flush: callbacks.flush.build_threadsafe_function().build_callback(
      |ctx: ThreadsafeCallContext<MemorySessionInvocation>| {
        Ok(FnArgs::from((ctx.value.memory_name, ctx.value.session_id)))
      },
    )?,
    consolidate: callbacks
      .consolidate
      .build_threadsafe_function()
      .build_callback(|ctx: ThreadsafeCallContext<MemorySessionInvocation>| {
        Ok(FnArgs::from((ctx.value.memory_name, ctx.value.session_id)))
      })?,
  })
}

fn build_loop_handler(loop_handler: LoopCallback<'_>) -> napi::Result<LoopHandler> {
  loop_handler
    .build_threadsafe_function()
    .build_callback(|ctx: ThreadsafeCallContext<String>| Ok(FnArgs::from((ctx.value,))))
}

fn resolve_tool_definitions(
  tools: Vec<Object<'_>>,
  tool_handler: Option<SharedToolCallback<'_>>,
) -> napi::Result<Vec<ResolvedToolDefinition>> {
  let mut resolved_tools = Vec::with_capacity(tools.len());
  let shared_handler = tool_handler
    .map(build_shared_tool_handler)
    .transpose()?
    .map(JsToolHandler::Shared)
    .map(Arc::new);

  for tool in tools {
    let name = get_tool_string_property(&tool, &["id", "name"])?;
    let description = tool.get_named_property::<String>("description")?;
    let parameters_json = get_tool_schema_json(&tool)?;
    let parameters = serde_json::from_str::<Value>(&parameters_json).map_err(|error| {
      napi::Error::from_reason(format!(
        "Invalid input schema JSON for tool '{}': {error}",
        name
      ))
    })?;
    let handler = if tool.has_named_property("execute")? {
      let execute =
        tool.get_named_property::<Function<'_, FnArgs<(String, String)>, String>>("execute")?;
      Arc::new(JsToolHandler::PerTool(build_tool_handler(execute)?))
    } else if let Some(handler) = shared_handler.as_ref() {
      Arc::clone(handler)
    } else {
      return Err(napi::Error::from_reason(format!(
        "Tool '{}' must define an execute function or use a shared toolHandler",
        name
      )));
    };

    resolved_tools.push(ResolvedToolDefinition {
      name,
      description,
      parameters,
      handler,
    });
  }

  Ok(resolved_tools)
}

fn get_tool_string_property(tool: &Object<'_>, names: &[&str]) -> napi::Result<String> {
  for name in names {
    if tool.has_named_property(name)? {
      return tool.get_named_property(name);
    }
  }

  let joined = names.join("' or '");
  Err(napi::Error::from_reason(format!(
    "Missing tool property '{joined}'"
  )))
}

fn get_tool_schema_json(tool: &Object<'_>) -> napi::Result<String> {
  if tool.has_named_property("inputSchemaJson")? {
    return tool.get_named_property("inputSchemaJson");
  }
  if tool.has_named_property("parametersJson")? {
    return tool.get_named_property("parametersJson");
  }
  if tool.has_named_property("inputSchema")? {
    return stringify_named_property(tool, "inputSchema");
  }
  if tool.has_named_property("parameters")? {
    return stringify_named_property(tool, "parameters");
  }

  Err(napi::Error::from_reason(
    "Missing tool property 'inputSchema', 'inputSchemaJson', 'parameters', or 'parametersJson'"
      .to_string(),
  ))
}

fn stringify_named_property(tool: &Object<'_>, property: &str) -> napi::Result<String> {
  let env = Env::from_raw(tool.value().env);
  let global = env.get_global()?;
  let json: JSON<'_> = global.get_named_property_unchecked("JSON")?;
  let value = tool.get_named_property::<Unknown<'_>>(property)?;
  json.stringify(value)
}

fn build_tool_registry(tools: Vec<ResolvedToolDefinition>) -> ToolRegistry {
  let mut registry = ToolRegistry::new();

  for tool in tools {
    let name = tool.name;
    registry.insert(
      name.clone(),
      Box::new(JsTool {
        name,
        description: tool.description,
        parameters: tool.parameters,
        handler: tool.handler,
      }),
    );
  }

  registry
}

fn build_memory_manager(
  memories: Vec<JsMemoryModule>,
  handlers: Arc<JsMemoryHandlers>,
) -> MemoryManager {
  let provider_names = memories
    .iter()
    .map(|memory| memory.name.clone())
    .collect::<Vec<_>>();
  let providers = memories
    .into_iter()
    .map(|memory| {
      Box::new(JsMemoryProvider {
        name: memory.name,
        handlers: Arc::clone(&handlers),
      }) as Box<dyn MemoryProvider>
    })
    .collect();

  MemoryManager::new(Box::new(JsMemoryRouter { provider_names }), providers)
}

fn error_run_result(message: impl Into<String>) -> CoreAgentRunResult {
  CoreAgentRunResult {
    content: message.into(),
    steps: Vec::new(),
  }
}

fn workflow_task_failure_message(result: &CoreAgentRunResult) -> Option<String> {
  if result
    .steps
    .last()
    .is_some_and(|step| matches!(step.kind.as_str(), "failed" | "error"))
  {
    return Some(result.content.trim().to_string());
  }

  if result.content.trim_start().starts_with("LLM error:") {
    return Some(result.content.trim().to_string());
  }

  None
}

fn spawn_agent_worker(
  definition: AgentDefinition,
  workspace_home: Option<String>,
  worker_config: WorkerConfig,
) -> napi::Result<mpsc::Sender<AgentWorkerMessage>> {
  let workspace_home = workspace_home.map(PathBuf::from);
  let (request_tx, request_rx) = mpsc::channel::<AgentWorkerMessage>();
  let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

  thread::spawn(move || {
    let runtime = match tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
    {
      Ok(runtime) => runtime,
      Err(error) => {
        let _ = ready_tx.send(Err(format!(
          "Initialization error: failed to create tokio runtime: {error}"
        )));
        for message_request in request_rx {
          if let AgentWorkerMessage::Run(request) = message_request {
            let _ = request.reply_tx.send(error_run_result(
              "Initialization error: failed to create tokio runtime",
            ));
          }
        }
        return;
      }
    };

    let tool_registry = build_tool_registry(worker_config.tools);
    let memory = worker_config
      .memory_handlers
      .map(|handlers| build_memory_manager(worker_config.memories, handlers));

    let mut agent = match runtime.block_on(
      CoreAgent::with_definition_tool_registry_executor_llm_and_workspace(
        definition,
        tool_registry,
        Box::new(RegistryToolExecutor),
        None,
        memory,
        workspace_home,
      ),
    ) {
      Ok(agent) => agent,
      Err(error) => {
        let message = format!("Initialization error: {error}");
        let _ = ready_tx.send(Err(message.clone()));
        for message_request in request_rx {
          if let AgentWorkerMessage::Run(request) = message_request {
            let _ = request.reply_tx.send(error_run_result(message.clone()));
          }
        }
        return;
      }
    };

    let _ = ready_tx.send(Ok(()));

    for message_request in request_rx {
      match message_request {
        AgentWorkerMessage::Run(request) => {
          let response = runtime.block_on(agent.run_detailed_with_context(
            &request.session_id,
            &request.user_message,
            request.exec_ctx,
            None,
          ));
          let _ = request.reply_tx.send(response);
        }
        AgentWorkerMessage::SetLoopHandler { handler, reply_tx } => {
          agent.agent_loop = Box::new(CallbackAgentLoop::new(handler));
          let _ = reply_tx.send(Ok(()));
        }
        AgentWorkerMessage::ClearLoopHandler { reply_tx } => {
          agent.agent_loop = Box::new(DefaultAgentLoop);
          let _ = reply_tx.send(Ok(()));
        }
      }
    }
  });

  ready_rx
    .recv()
    .map_err(|_| napi::Error::from_reason("Initialization error: agent worker exited".to_string()))?
    .map_err(napi::Error::from_reason)?;

  Ok(request_tx)
}

fn spawn_multi_agent_worker(
  members: Vec<JsMultiAgentMember>,
  workspace_home: Option<String>,
) -> napi::Result<mpsc::Sender<MultiAgentRequest>> {
  if members.is_empty() {
    return Err(napi::Error::from_reason(
      "Multi-agent runtime requires at least one member".to_string(),
    ));
  }

  let workspace_home = workspace_home.map(PathBuf::from);
  let (request_tx, request_rx) = mpsc::channel::<MultiAgentRequest>();
  let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

  thread::spawn(move || {
    let runtime = match tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
    {
      Ok(runtime) => runtime,
      Err(error) => {
        let _ = ready_tx.send(Err(format!(
          "Initialization error: failed to create tokio runtime: {error}"
        )));
        for request in request_rx {
          fail_multi_agent_request(
            request,
            "Initialization error: failed to create tokio runtime".to_string(),
          );
        }
        return;
      }
    };

    let mut builder = MultiAgentRuntime::builder();
    if let Some(home) = workspace_home {
      builder = builder.with_workspace_home(home);
    }

    for member in members {
      let (agent_id, definition, capabilities) = build_multi_agent_definition(member);
      builder = builder.add_agent(agent_id, definition, capabilities);
    }

    let runtime_instance = match runtime.block_on(builder.build()) {
      Ok(runtime_instance) => runtime_instance,
      Err(error) => {
        let message = format!("Initialization error: {error}");
        let _ = ready_tx.send(Err(message.clone()));
        for request in request_rx {
          fail_multi_agent_request(request, message.clone());
        }
        return;
      }
    };

    let _ = ready_tx.send(Ok(()));

    for request in request_rx {
      match request {
        MultiAgentRequest::Process {
          agent_id,
          session_id,
          user_message,
          reply_tx,
        } => {
          let response = runtime.block_on(runtime_instance.process_detailed(
            &agent_id,
            &session_id,
            &user_message,
            None,
          ));
          let _ = reply_tx.send(response);
        }
        MultiAgentRequest::Registry { reply_tx } => {
          let cards = runtime
            .block_on(runtime_instance.registry().list_all())
            .into_iter()
            .map(JsAgentCard::from)
            .collect();
          let _ = reply_tx.send(Ok(cards));
        }
        MultiAgentRequest::Discover {
          capability,
          status,
          reply_tx,
        } => {
          let mut query = DiscoverQuery::new();
          if let Some(capability) = capability {
            query = query.with_capability(capability);
          }
          if let Some(status) = status {
            query = query.with_status(status.into());
          }

          let cards = runtime
            .block_on(runtime_instance.registry().discover(&query))
            .into_iter()
            .map(JsAgentCard::from)
            .collect();
          let _ = reply_tx.send(Ok(cards));
        }
      }
    }
  });

  ready_rx
    .recv()
    .map_err(|_| {
      napi::Error::from_reason("Initialization error: multi-agent worker exited".to_string())
    })?
    .map_err(napi::Error::from_reason)?;

  Ok(request_tx)
}

fn spawn_workflow_worker(
  agents: Vec<Arc<AgentHandle>>,
  tasks_json: Vec<String>,
  workflows_json: Vec<String>,
  workspace_home: Option<String>,
) -> napi::Result<mpsc::Sender<WorkflowBindingRequest>> {
  if agents.is_empty() {
    return Err(napi::Error::from_reason(
      "Workflow runtime requires at least one agent".to_string(),
    ));
  }

  let workspace_home = workspace_home.map(PathBuf::from);
  let (request_tx, request_rx) = mpsc::channel::<WorkflowBindingRequest>();
  let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

  thread::spawn(move || {
    let runtime = match tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
    {
      Ok(runtime) => runtime,
      Err(error) => {
        let message = format!("Initialization error: failed to create tokio runtime: {error}");
        let _ = ready_tx.send(Err(message.clone()));
        fail_workflow_requests(request_rx, message);
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
          let message = error.to_string();
          let _ = ready_tx.send(Err(message.clone()));
          fail_workflow_requests(request_rx, message);
          return;
        }
      };

      if registration.agent_id.trim().is_empty() {
        let message = "Initialization error: workflow agent_id cannot be empty.".to_string();
        let _ = ready_tx.send(Err(message.clone()));
        fail_workflow_requests(request_rx, message);
        return;
      }

      if agents_by_id
        .insert(registration.agent_id.clone(), Arc::clone(&agent))
        .is_some()
      {
        let message = format!(
          "Initialization error: duplicate workflow agent_id '{}'.",
          registration.agent_id
        );
        let _ = ready_tx.send(Err(message.clone()));
        fail_workflow_requests(request_rx, message);
        return;
      }

      registrations.push(registration);
    }

    for task_json in tasks_json {
      let task = match parse_json_value::<TaskDefinition>(&task_json, "workflow task") {
        Ok(task) => task,
        Err(error) => {
          let message = format!("Initialization error: {error}");
          let _ = ready_tx.send(Err(message.clone()));
          fail_workflow_requests(request_rx, message);
          return;
        }
      };
      workflow_builder = workflow_builder.add_task(task);
    }

    for workflow_json in workflows_json {
      let workflow =
        match parse_json_value::<WorkflowDefinition>(&workflow_json, "workflow definition") {
          Ok(workflow) => workflow,
          Err(error) => {
            let message = format!("Initialization error: {error}");
            let _ = ready_tx.send(Err(message.clone()));
            fail_workflow_requests(request_rx, message);
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
          let _ = ready_tx.send(Err(message.clone()));
          fail_workflow_requests(request_rx, message);
          return;
        }
      };

    let _ = ready_tx.send(Ok(()));

    for request in request_rx {
      match request {
        WorkflowBindingRequest::ListWorkflows { reply_tx } => {
          let result = workflow_runtime
            .list_workflows()
            .into_iter()
            .map(|workflow| to_json_string(workflow, "workflow definition"))
            .collect::<Result<Vec<_>, _>>()
            .and_then(|workflows| to_json_string(&workflows, "workflow definition list"));
          let _ = reply_tx.send(result);
        }
        WorkflowBindingRequest::ListRuns { reply_tx } => {
          let result = runtime
            .block_on(workflow_runtime.list_runs())
            .and_then(|runs| to_json_string(&runs, "workflow run list"));
          let _ = reply_tx.send(result);
        }
        WorkflowBindingRequest::Inspect { run_id, reply_tx } => {
          let result = runtime
            .block_on(workflow_runtime.inspect(&run_id))
            .and_then(|state| to_json_string(&state, "workflow run state"));
          let _ = reply_tx.send(result);
        }
        WorkflowBindingRequest::Start {
          request_json,
          reply_tx,
        } => {
          let result = parse_json_value::<WorkflowRequest>(&request_json, "workflow request")
            .and_then(|request| runtime.block_on(workflow_runtime.start(request)))
            .and_then(|response| to_json_string(&response, "workflow response"));
          let _ = reply_tx.send(result);
        }
        WorkflowBindingRequest::Resume { run_id, reply_tx } => {
          let result = runtime
            .block_on(workflow_runtime.resume(&run_id))
            .and_then(|response| to_json_string(&response, "workflow response"));
          let _ = reply_tx.send(result);
        }
        WorkflowBindingRequest::SubmitIntervention {
          run_id,
          intervention_id,
          response,
          reply_tx,
        } => {
          let result = runtime
            .block_on(workflow_runtime.submit_intervention(&run_id, &intervention_id, response))
            .and_then(|state| to_json_string(&state, "workflow run state"));
          let _ = reply_tx.send(result);
        }
      }
    }
  });

  ready_rx
    .recv()
    .map_err(|_| {
      napi::Error::from_reason("Initialization error: workflow worker exited".to_string())
    })?
    .map_err(napi::Error::from_reason)?;

  Ok(request_tx)
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
      )
      .map_err(|error| error.to_string())?;

    if let Some(error) = workflow_task_failure_message(&result) {
      return Err(error);
    }

    Ok(WorkflowTaskResult {
      content: result.content.clone(),
      value: json!({
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

fn fail_multi_agent_request(request: MultiAgentRequest, message: String) {
  match request {
    MultiAgentRequest::Process { reply_tx, .. } => {
      let _ = reply_tx.send(Err(message));
    }
    MultiAgentRequest::Registry { reply_tx } => {
      let _ = reply_tx.send(Err(message));
    }
    MultiAgentRequest::Discover { reply_tx, .. } => {
      let _ = reply_tx.send(Err(message));
    }
  }
}

fn fail_workflow_requests(request_rx: mpsc::Receiver<WorkflowBindingRequest>, message: String) {
  for request in request_rx {
    match request {
      WorkflowBindingRequest::ListWorkflows { reply_tx }
      | WorkflowBindingRequest::ListRuns { reply_tx }
      | WorkflowBindingRequest::Inspect { reply_tx, .. }
      | WorkflowBindingRequest::Start { reply_tx, .. }
      | WorkflowBindingRequest::Resume { reply_tx, .. }
      | WorkflowBindingRequest::SubmitIntervention { reply_tx, .. } => {
        let _ = reply_tx.send(Err(message.clone()));
      }
    }
  }
}

impl From<JsAgentStatus> for AgentStatus {
  fn from(value: JsAgentStatus) -> Self {
    match value {
      JsAgentStatus::Online => AgentStatus::Online,
      JsAgentStatus::Busy => AgentStatus::Busy,
      JsAgentStatus::Offline => AgentStatus::Offline,
    }
  }
}

impl From<AgentStatus> for JsAgentStatus {
  fn from(value: AgentStatus) -> Self {
    match value {
      AgentStatus::Online => JsAgentStatus::Online,
      AgentStatus::Busy => JsAgentStatus::Busy,
      AgentStatus::Offline => JsAgentStatus::Offline,
    }
  }
}

impl From<AgentCard> for JsAgentCard {
  fn from(value: AgentCard) -> Self {
    Self {
      agent_id: value.agent_id,
      name: value.name,
      description: value.description,
      capabilities: value.capabilities,
      status: value.status.into(),
    }
  }
}

impl From<JsMemoryKind> for MemoryKind {
  fn from(value: JsMemoryKind) -> Self {
    match value {
      JsMemoryKind::RecentMessage => MemoryKind::RecentMessage,
      JsMemoryKind::Summary => MemoryKind::Summary,
      JsMemoryKind::Entity => MemoryKind::Entity,
      JsMemoryKind::Preference => MemoryKind::Preference,
    }
  }
}

impl From<MemoryKind> for JsMemoryKind {
  fn from(value: MemoryKind) -> Self {
    match value {
      MemoryKind::RecentMessage => JsMemoryKind::RecentMessage,
      MemoryKind::Summary => JsMemoryKind::Summary,
      MemoryKind::Entity => JsMemoryKind::Entity,
      MemoryKind::Preference => JsMemoryKind::Preference,
    }
  }
}

impl From<JsMemoryEntry> for MemoryEntry {
  fn from(value: JsMemoryEntry) -> Self {
    let timestamp_ns = value.timestamp_ns.parse::<u128>().unwrap_or(u128::MAX);

    Self {
      key: value.key,
      content: value.content,
      kind: value.kind.into(),
      relevance: value.relevance as f32,
      timestamp_ns,
    }
  }
}

impl From<MemoryEntry> for JsMemoryEntry {
  fn from(value: MemoryEntry) -> Self {
    Self {
      key: value.key,
      content: value.content,
      kind: value.kind.into(),
      relevance: value.relevance as f64,
      timestamp_ns: value.timestamp_ns.to_string(),
    }
  }
}

impl From<CoreExecutionStep> for JsExecutionStep {
  fn from(value: CoreExecutionStep) -> Self {
    Self {
      index: value.index.min(u32::MAX as usize) as u32,
      phase: value.phase,
      kind: value.kind,
      detail: value.detail,
    }
  }
}

impl From<CoreAgentRunResult> for JsAgentRunResult {
  fn from(value: CoreAgentRunResult) -> Self {
    Self {
      output: value.content,
      steps: value.steps.into_iter().map(JsExecutionStep::from).collect(),
    }
  }
}
