use crate::agent::{Agent, AgentDefinition};
use crate::llm::LlmProvider;
use crate::memory::MemoryManager;
use crate::runtime::{Runtime, RuntimeHandler, RuntimeRequest, SessionContext};
use crate::tooling::tool_calling::ToolExecutor;
use crate::tooling::types::{Tool, ToolRegistry};
use async_trait::async_trait;
use std::path::PathBuf;

pub type AgentRuntime = Runtime<AgentRuntimeHandler>;

pub struct RuntimeBuilder {
    definition: AgentDefinition,
    llm: Option<Box<dyn LlmProvider>>,
    memory: Option<MemoryManager>,
    tool_registry: ToolRegistry,
    tool_executor: Option<Box<dyn ToolExecutor>>,
    workspace_home: Option<PathBuf>,
}

impl RuntimeBuilder {
    pub fn new(definition: AgentDefinition) -> Self {
        Self {
            definition,
            llm: None,
            memory: None,
            tool_registry: ToolRegistry::new(),
            tool_executor: None,
            workspace_home: None,
        }
    }

    pub fn for_default_agent() -> Self {
        Self::new(AgentDefinition::default())
    }

    pub fn with_tool_executor(mut self, tool_executor: Box<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(tool_executor);
        self
    }

    pub fn with_tool_registry(mut self, tool_registry: ToolRegistry) -> Self {
        self.tool_registry = tool_registry;
        self
    }

    pub fn register_tool<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tool_registry
            .insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_boxed_tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tool_registry.insert(tool.name().to_string(), tool);
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.definition.model = model.into();
        self
    }

    pub fn with_llm(mut self, llm: Box<dyn LlmProvider>) -> Self {
        self.llm = Some(llm);
        self
    }

    pub fn with_memory(mut self, memory: MemoryManager) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn with_workspace_home(mut self, workspace_home: impl Into<PathBuf>) -> Self {
        self.workspace_home = Some(workspace_home.into());
        self
    }

    pub async fn build(self) -> Result<AgentRuntime, String> {
        let RuntimeBuilder {
            definition,
            llm,
            memory,
            tool_registry,
            tool_executor,
            workspace_home,
        } = self;

        let tool_executor = tool_executor
            .unwrap_or_else(|| Box::new(crate::tooling::tool_calling::RegistryToolExecutor));
        let agent = Agent::with_definition_tool_registry_executor_llm_and_workspace(
            definition,
            tool_registry,
            tool_executor,
            llm,
            memory,
            workspace_home,
        )
        .await?;

        Ok(Runtime::new(AgentRuntimeHandler { agent }))
    }
}

pub struct AgentRuntimeHandler {
    agent: Agent,
}

#[async_trait(?Send)]
impl RuntimeHandler for AgentRuntimeHandler {
    async fn handle(
        &self,
        request: &RuntimeRequest,
        _session: &SessionContext,
    ) -> Result<String, String> {
        Ok(self.agent.run(&request.session_id, &request.content).await)
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeBuilder;
    use crate::agent::AgentDefinition;
    use crate::llm::{
        ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult,
        ToolDefinition,
    };
    use crate::runtime::RuntimeRequest;
    use crate::tooling::types::{Tool, ToolContext, parse_tool_args};
    use async_trait::async_trait;
    use futures::stream;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    struct RecordingLlm {
        responses: Arc<Mutex<VecDeque<LlmResponse>>>,
        requested_tools: Arc<Mutex<Vec<Vec<ToolDefinition>>>>,
    }

    impl RecordingLlm {
        fn new(responses: Vec<LlmResponse>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
                requested_tools: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn requested_tools(&self) -> Vec<Vec<ToolDefinition>> {
            self.requested_tools.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmProvider for RecordingLlm {
        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _config: &LlmConfig,
        ) -> LlmResult<LlmResponse> {
            Err(LlmError::Provider("not used".to_string()))
        }

        async fn complete_stream(
            &self,
            _messages: &[ChatMessage],
            _config: &LlmConfig,
        ) -> LlmResult<crate::llm::ResponseStream> {
            Ok(Box::pin(stream::empty()))
        }

        async fn complete_with_tools(
            &self,
            _messages: &[ChatMessage],
            tools: &[ToolDefinition],
            _config: &LlmConfig,
        ) -> LlmResult<LlmResponse> {
            self.requested_tools.lock().unwrap().push(tools.to_vec());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| LlmError::Provider("missing response".to_string()))
        }

        fn name(&self) -> &'static str {
            "recording"
        }

        fn available_models(&self) -> Vec<&'static str> {
            vec!["recording"]
        }
    }

    fn temp_home(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "core-next-runtime-builder-tests-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
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

    #[tokio::test]
    async fn runtime_builder_registers_dynamic_typed_tools() {
        let llm = RecordingLlm::new(vec![LlmResponse {
            content: "ok".to_string(),
            usage: None,
            tool_calls: Vec::new(),
            model: "recording".to_string(),
            finish_reason: Some("stop".to_string()),
        }]);

        let runtime = RuntimeBuilder::new(AgentDefinition::default())
            .with_llm(Box::new(llm.clone()))
            .with_workspace_home(temp_home("dynamic-tools"))
            .register_tool(EchoTool)
            .build()
            .await
            .unwrap();

        let response = runtime
            .process(RuntimeRequest::new("session-a", "cli", "hello"))
            .await
            .unwrap();

        assert_eq!(response.content, "ok");

        let requested_tools = llm.requested_tools();
        assert_eq!(requested_tools.len(), 1);
        let tool_names = requested_tools[0]
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names, vec!["echo", "exec", "read_file", "write_file"]);
    }
}
