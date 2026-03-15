use crate::agent::{Agent, AgentDefinition};
use crate::llm::LlmProvider;
use crate::memory::MemoryManager;
use crate::runtime::{Runtime, RuntimeHandler, RuntimeRequest, SessionContext};
use crate::tooling::tool_calling::ToolExecutor;
use async_trait::async_trait;
use std::path::PathBuf;

pub type AgentRuntime = Runtime<AgentRuntimeHandler>;

pub struct RuntimeBuilder {
    definition: AgentDefinition,
    llm: Option<Box<dyn LlmProvider>>,
    memory: Option<MemoryManager>,
    tool_executor: Option<Box<dyn ToolExecutor>>,
    workspace_home: Option<PathBuf>,
}

impl RuntimeBuilder {
    pub fn new(definition: AgentDefinition) -> Self {
        Self {
            definition,
            llm: None,
            memory: None,
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
            tool_executor,
            workspace_home,
        } = self;

        let tool_executor = tool_executor
            .unwrap_or_else(|| Box::new(crate::tooling::tool_calling::RegistryToolExecutor));
        let agent = Agent::with_definition_executor_llm_and_workspace(
            definition,
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
