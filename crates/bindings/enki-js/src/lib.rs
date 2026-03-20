#![deny(clippy::all)]

use async_trait::async_trait;
use core_next::agent::{Agent as CoreAgent, AgentDefinition};
use core_next::memory::{
  MemoryEntry, MemoryKind, MemoryManager, MemoryProvider, MemoryRouter, MemoryStrategy,
};
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry};
use napi::bindgen_prelude::{AsyncTask, FnArgs, Function, JsObjectValue, Object, Unknown};
use napi::threadsafe_function::{ThreadsafeCallContext, ThreadsafeFunction};
use napi::{Env, JSON, JsValue, Task};
use napi_derive::napi;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const DEFAULT_NAME: &str = "Personal Assistant";
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful Personal Assistant agent.";
const DEFAULT_MAX_ITERATIONS: u32 = 20;

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

struct RunRequest {
  session_id: String,
  user_message: String,
  reply_tx: mpsc::Sender<String>,
}

struct AgentHandle {
  request_tx: Mutex<mpsc::Sender<RunRequest>>,
}

pub struct RunTask {
  inner: Arc<AgentHandle>,
  session_id: String,
  user_message: String,
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

struct WorkerConfig {
  tools: Vec<ResolvedToolDefinition>,
  memories: Vec<JsMemoryModule>,
  memory_handlers: Option<Arc<JsMemoryHandlers>>,
}

struct BuildOptions {
  name: Option<String>,
  system_prompt_preamble: Option<String>,
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

#[napi(string_enum)]
pub enum JsMemoryKind {
  RecentMessage,
  Summary,
  Entity,
  Preference,
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

#[napi(js_name = "NativeEnkiAgent")]
pub struct NativeEnkiAgent {
  inner: Arc<AgentHandle>,
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

#[napi]
impl NativeEnkiAgent {
  #[napi(constructor)]
  pub fn new(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
  ) -> napi::Result<Self> {
    Self::build(
      name,
      system_prompt_preamble,
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
  pub fn with_tools(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    tools: Vec<Object<'_>>,
    tool_handler: Option<SharedToolCallback<'_>>,
  ) -> napi::Result<Self> {
    let tools = resolve_tool_definitions(tools, tool_handler)?;

    Self::build(
      name,
      system_prompt_preamble,
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
  ) -> napi::Result<Self> {
    Self::build_with_memory(MemoryFactoryOptions {
      build: BuildOptions {
        name,
        system_prompt_preamble,
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
  ) -> napi::Result<Self> {
    Self::build_with_tools_and_memory(ToolAndMemoryFactoryOptions {
      build: BuildOptions {
        name,
        system_prompt_preamble,
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
  pub fn run(&self, session_id: String, user_message: String) -> AsyncTask<RunTask> {
    AsyncTask::new(RunTask {
      inner: Arc::clone(&self.inner),
      session_id,
      user_message,
    })
  }
}

impl NativeEnkiAgent {
  fn build_from_options(build: BuildOptions, worker_config: WorkerConfig) -> napi::Result<Self> {
    Self::build(
      build.name,
      build.system_prompt_preamble,
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
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
    worker_config: WorkerConfig,
  ) -> napi::Result<Self> {
    let definition = build_definition(name, system_prompt_preamble, model, max_iterations);
    let request_tx = spawn_agent_worker(definition, workspace_home, worker_config)?;

    Ok(Self {
      inner: Arc::new(AgentHandle {
        request_tx: Mutex::new(request_tx),
      }),
    })
  }
}

impl Task for RunTask {
  type Output = String;
  type JsValue = String;

  fn compute(&mut self) -> napi::Result<Self::Output> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = RunRequest {
      session_id: self.session_id.clone(),
      user_message: self.user_message.clone(),
      reply_tx,
    };

    let sender =
      self.inner.request_tx.lock().map_err(|_| {
        napi::Error::from_reason("Worker error: request mutex poisoned".to_string())
      })?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
    Ok(output)
  }
}

fn build_definition(
  name: Option<String>,
  system_prompt_preamble: Option<String>,
  model: Option<String>,
  max_iterations: Option<u32>,
) -> AgentDefinition {
  AgentDefinition {
    name: name.unwrap_or_else(|| DEFAULT_NAME.to_string()),
    system_prompt_preamble: system_prompt_preamble
      .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
    model: model.unwrap_or_default(),
    max_iterations: max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS).max(1) as usize,
  }
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
    record: callbacks.record.build_threadsafe_function().build_callback(
      |ctx: ThreadsafeCallContext<MemoryRecordInvocation>| {
        Ok(FnArgs::from((
          ctx.value.memory_name,
          ctx.value.session_id,
          ctx.value.user_msg,
          ctx.value.assistant_msg,
        )))
      },
    )?,
    recall: callbacks.recall.build_threadsafe_function().build_callback(
      |ctx: ThreadsafeCallContext<MemoryRecallInvocation>| {
        Ok(FnArgs::from((
          ctx.value.memory_name,
          ctx.value.session_id,
          ctx.value.query,
          ctx.value.max_entries,
        )))
      },
    )?,
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

fn spawn_agent_worker(
  definition: AgentDefinition,
  workspace_home: Option<String>,
  worker_config: WorkerConfig,
) -> napi::Result<mpsc::Sender<RunRequest>> {
  let workspace_home = workspace_home.map(PathBuf::from);
  let (request_tx, request_rx) = mpsc::channel::<RunRequest>();
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
          let _ = request
            .reply_tx
            .send("Initialization error: failed to create tokio runtime".to_string());
        }
        return;
      }
    };

    let tool_registry = build_tool_registry(worker_config.tools);
    let memory = worker_config
      .memory_handlers
      .map(|handlers| build_memory_manager(worker_config.memories, handlers));

    let agent = match runtime.block_on(
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
        for request in request_rx {
          let _ = request.reply_tx.send(message.clone());
        }
        return;
      }
    };

    let _ = ready_tx.send(Ok(()));

    for request in request_rx {
      let response = runtime.block_on(agent.run(&request.session_id, &request.user_message));
      let _ = request.reply_tx.send(response);
    }
  });

  ready_rx
    .recv()
    .map_err(|_| napi::Error::from_reason("Initialization error: agent worker exited".to_string()))?
    .map_err(napi::Error::from_reason)?;

  Ok(request_tx)
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
