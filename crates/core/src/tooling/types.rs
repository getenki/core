use crate::registry::AgentRegistry;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Async function that the runtime provides for asking a human user a question.
/// The implementation suspends the calling task until the human replies.
#[async_trait(?Send)]
pub trait AskHumanFn: 'static {
    async fn ask(&self, query: &str) -> Result<String, String>;
}

pub type ToolRegistry = BTreeMap<String, Box<dyn Tool>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowToolContext {
    pub workflow_id: String,
    pub run_id: String,
    pub node_id: String,
    pub attempt: usize,
}

#[derive(Clone)]
pub struct ToolContext {
    pub agent_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub delegation: Option<DelegationContext>,
    pub human: Option<Arc<dyn AskHumanFn>>,
    pub workflow: Option<WorkflowToolContext>,
}

/// Provides delegation capabilities to tools in a multi-agent runtime.
///
/// When an agent is part of a `MultiAgentRuntime`, the runtime injects a
/// `DelegationContext` so that delegation tools can discover peers and
/// route requests without the agent holding direct references to others.
#[derive(Clone)]
pub struct DelegationContext {
    pub registry: Arc<AgentRegistry>,
    pub self_agent_id: String,
    delegate_fn: Arc<dyn DelegateFn>,
}

/// Async function that the runtime provides for sending a task to a peer.
#[async_trait(?Send)]
pub trait DelegateFn: 'static {
    async fn delegate(&self, target_agent_id: &str, task: &str) -> Result<String, String>;
}

impl DelegationContext {
    pub fn new(
        registry: Arc<AgentRegistry>,
        self_agent_id: impl Into<String>,
        delegate_fn: Arc<dyn DelegateFn>,
    ) -> Self {
        Self {
            registry,
            self_agent_id: self_agent_id.into(),
            delegate_fn,
        }
    }

    pub async fn delegate(&self, target_agent_id: &str, task: &str) -> Result<String, String> {
        self.delegate_fn.delegate(target_agent_id, task).await
    }
}

pub trait IntoToolOutput {
    fn into_tool_output(self) -> String;
}

impl IntoToolOutput for String {
    fn into_tool_output(self) -> String {
        self
    }
}

impl IntoToolOutput for &str {
    fn into_tool_output(self) -> String {
        self.to_string()
    }
}

impl<T, E> IntoToolOutput for Result<T, E>
where
    T: IntoToolOutput,
    E: std::fmt::Display,
{
    fn into_tool_output(self) -> String {
        match self {
            Ok(value) => value.into_tool_output(),
            Err(error) => format!("Error: {error}"),
        }
    }
}

pub fn parse_tool_args<T>(args: &Value) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(args.clone()).map_err(|e| e.to_string())
}

#[async_trait(?Send)]
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String;

    fn as_tool_payload(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters(),
            }
        })
    }

    fn as_catalog_entry(&self) -> Value {
        json!({
            "description": self.description(),
            "parameters": self.parameters(),
        })
    }
}

#[derive(Default)]
pub struct ToolRegistryBuilder {
    tools: ToolRegistry,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_boxed(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn build(self) -> ToolRegistry {
        self.tools
    }
}

#[cfg(test)]
mod tests {
    use super::{Tool, ToolContext, ToolRegistryBuilder, parse_tool_args};
    use async_trait::async_trait;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    fn tool_context() -> ToolContext {
        ToolContext {
            agent_dir: PathBuf::from("agent"),
            workspace_dir: PathBuf::from("workspace"),
            sessions_dir: PathBuf::from("sessions"),
            delegation: None,
            human: None,
            workflow: None,
        }
    }

    #[derive(Deserialize)]
    struct EchoParams {
        value: String,
    }

    struct EchoTool;

    #[async_trait(?Send)]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo a value"
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"]
            })
        }

        async fn execute(&self, args: &Value, _ctx: &ToolContext) -> String {
            let params: EchoParams = match parse_tool_args(args) {
                Ok(params) => params,
                Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
            };

            format!("echo:{}", params.value)
        }
    }

    struct EchoWithWorkspaceTool;

    #[async_trait(?Send)]
    impl Tool for EchoWithWorkspaceTool {
        fn name(&self) -> &str {
            "echo_workspace"
        }

        fn description(&self) -> &str {
            "Echo a value with workspace context"
        }

        fn parameters(&self) -> Value {
            EchoTool.parameters()
        }

        async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
            let params: EchoParams = match parse_tool_args(args) {
                Ok(params) => params,
                Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
            };

            format!("{}:{}", ctx.workspace_dir.display(), params.value)
        }
    }

    struct EchoAsyncTool;

    #[async_trait(?Send)]
    impl Tool for EchoAsyncTool {
        fn name(&self) -> &str {
            "echo_async"
        }

        fn description(&self) -> &str {
            "Echo a value asynchronously"
        }

        fn parameters(&self) -> Value {
            EchoTool.parameters()
        }

        async fn execute(&self, args: &Value, _ctx: &ToolContext) -> String {
            let params: EchoParams = match parse_tool_args(args) {
                Ok(params) => params,
                Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
            };

            format!("async:{}", params.value)
        }
    }

    #[tokio::test]
    async fn registry_builder_registers_concrete_tools() {
        let registry = ToolRegistryBuilder::new().register(EchoTool).build();

        let result = registry
            .get("echo")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "echo:hello");
    }

    #[tokio::test]
    async fn concrete_tools_can_use_context() {
        let registry = ToolRegistryBuilder::new()
            .register(EchoWithWorkspaceTool)
            .build();

        let result = registry
            .get("echo_workspace")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert!(result.ends_with("workspace:hello"));
    }

    #[tokio::test]
    async fn concrete_tools_can_execute_async_logic() {
        let registry = ToolRegistryBuilder::new().register(EchoAsyncTool).build();

        let result = registry
            .get("echo_async")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "async:hello");
    }

    #[tokio::test]
    async fn parse_tool_args_reports_invalid_arguments() {
        let result = EchoTool
            .execute(&json!({ "other": "hello" }), &tool_context())
            .await;

        assert!(result.starts_with("Error: failed to parse tool arguments:"));
    }
}
