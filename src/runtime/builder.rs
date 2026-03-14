use crate::agent::{Agent, AgentDefinition};
use crate::runtime::{Runtime, RuntimeHandler, RuntimeRequest, SessionContext};
use crate::tooling::tool_calling::ToolExecutor;
use async_trait::async_trait;

pub type AgentRuntime = Runtime<AgentRuntimeHandler>;

pub struct RuntimeBuilder {
    definition: AgentDefinition,
    tool_executor: Option<Box<dyn ToolExecutor>>,
}

impl RuntimeBuilder {
    pub fn new(definition: AgentDefinition) -> Self {
        Self {
            definition,
            tool_executor: None,
        }
    }

    pub fn for_default_agent() -> Self {
        Self::new(AgentDefinition::default())
    }

    pub fn with_tool_executor(mut self, tool_executor: Box<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(tool_executor);
        self
    }

    pub async fn build(self) -> Result<AgentRuntime, String> {
        let agent = match self.tool_executor {
            Some(tool_executor) => {
                Agent::with_definition_and_executor(self.definition, tool_executor).await?
            }
            None => Agent::with_definition(self.definition).await?,
        };

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
