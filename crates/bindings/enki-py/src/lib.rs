use async_trait::async_trait;
use core_next::agent::{Agent, AgentDefinition};
use core_next::tooling::tool_calling::RegistryToolExecutor;
use core_next::tooling::types::{Tool, ToolContext, ToolRegistry};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

#[derive(Clone, Debug)]
pub struct EnkiToolSpec {
    pub name: String,
    pub description: String,
    pub parameters_json: String,
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

struct PythonTool {
    name: String,
    description: String,
    parameters: Value,
    handler: Arc<dyn EnkiToolHandler>,
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
    tools: Vec<EnkiToolSpec>,
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

struct RunRequest {
    session_id: String,
    user_message: String,
    reply_tx: tokio::sync::oneshot::Sender<String>,
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
        tools: Vec<EnkiToolSpec>,
        handler: Box<dyn EnkiToolHandler>,
    ) -> Self {
        let definition = AgentDefinition {
            name,
            system_prompt_preamble,
            model,
            max_iterations: max_iterations as usize,
        };

        Self::from_custom_tools(
            definition,
            workspace_home,
            tools,
            handler,
        )
    }

    fn from_registry(
        definition: AgentDefinition,
        workspace_home: Option<String>,
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
                        let _ = request.reply_tx.send(message.clone());
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
                            let _ = request.reply_tx.send(message.clone());
                        }
                        return;
                    }
                };

            for request in request_rx {
                let response =
                    runtime.block_on(agent.run(&request.session_id, &request.user_message));
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
        tools: Vec<EnkiToolSpec>,
        handler: Box<dyn EnkiToolHandler>,
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
                        let _ = request.reply_tx.send(message.clone());
                    }
                    return;
                }
            };

            let tool_registry =
                match build_tool_registry(tools, Arc::from(handler)) {
                    Ok(tool_registry) => tool_registry,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        for request in request_rx {
                            let _ = request.reply_tx.send(message.clone());
                        }
                        return;
                    }
                };

            let agent =
                match runtime.block_on(Agent::with_definition_tool_registry_executor_and_workspace(
                    definition,
                    tool_registry,
                    Box::new(RegistryToolExecutor),
                    workspace_home,
                )) {
                    Ok(agent) => agent,
                    Err(error) => {
                        let message = format!("Initialization error: {error}");
                        for request in request_rx {
                            let _ = request.reply_tx.send(message.clone());
                        }
                        return;
                    }
                };

            for request in request_rx {
                let response =
                    runtime.block_on(agent.run(&request.session_id, &request.user_message));
                let _ = request.reply_tx.send(response);
            }
        });

        Self {
            request_tx: Mutex::new(request_tx),
        }
    }

    pub async fn run(&self, session_id: String, user_message: String) -> String {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = RunRequest {
            session_id,
            user_message,
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
            return message;
        }

        reply_rx
            .await
            .unwrap_or_else(|_| "Worker error: agent worker dropped reply channel".to_string())
    }
}

uniffi::include_scaffolding!("enki");
