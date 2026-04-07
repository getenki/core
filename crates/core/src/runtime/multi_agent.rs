use crate::agent::{Agent, AgentDefinition, AgentExecutionContext, AgentRunResult};
use crate::llm::LlmProvider;
use crate::memory::MemoryManager;
use crate::registry::{AgentCard, AgentRegistry, AgentStatus, FirstMatchSelector, PeerSelector};
use crate::tooling::delegation_tools::{DelegateTaskTool, DiscoverAgentsTool};
use crate::tooling::tool_calling::{RegistryToolExecutor, ToolExecutor};
use crate::tooling::types::{DelegateFn, Tool, ToolRegistry, WorkflowToolContext};
use crate::workflow::{TaskTarget, WorkflowTaskResult, WorkflowTaskRunner};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// MultiAgentRuntime
// ---------------------------------------------------------------------------

/// A runtime that manages multiple named agents sharing a common
/// `AgentRegistry`.  Agents can discover each other and delegate tasks
/// through runtime-injected tools.
pub struct MultiAgentRuntime {
    registry: Arc<AgentRegistry>,
    agents: HashMap<String, Arc<Agent>>,
    #[allow(dead_code)]
    selector: Box<dyn PeerSelector>,
}

impl MultiAgentRuntime {
    pub fn builder() -> MultiAgentRuntimeBuilder {
        MultiAgentRuntimeBuilder::new()
    }

    /// Process a request targeting a specific agent by `agent_id`.
    pub async fn process(
        &self,
        agent_id: &str,
        session_id: &str,
        message: &str,
    ) -> Result<String, String> {
        Ok(self
            .process_detailed(agent_id, session_id, message, None)
            .await?
            .content)
    }

    pub async fn process_detailed(
        &self,
        agent_id: &str,
        session_id: &str,
        message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(crate::agent::types::ExecutionStep) + Send + Sync>>,
    ) -> Result<AgentRunResult, String> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| format!("Agent '{agent_id}' not found in runtime."))?;

        Ok(agent.run_detailed(session_id, message, on_step).await)
    }

    pub fn registry(&self) -> &Arc<AgentRegistry> {
        &self.registry
    }

    pub fn agent_ids(&self) -> Vec<&String> {
        self.agents.keys().collect()
    }
}

// ---------------------------------------------------------------------------
// RuntimeDelegateFn — the callback wired into each agent's DelegationContext
// ---------------------------------------------------------------------------

struct RuntimeDelegateFn {
    agents: HashMap<String, Arc<Agent>>,
}

#[async_trait(?Send)]
impl DelegateFn for RuntimeDelegateFn {
    async fn delegate(&self, target_agent_id: &str, task: &str) -> Result<String, String> {
        let agent = self
            .agents
            .get(target_agent_id)
            .ok_or_else(|| format!("Agent '{target_agent_id}' not found."))?;

        // Use a delegation-specific session so we don't pollute the caller's history
        let session_id = format!("delegation-{}-{}", target_agent_id, uuid_v4_simple());
        Ok(agent.run(&session_id, task).await)
    }
}

fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}")
}

#[async_trait(?Send)]
impl WorkflowTaskRunner for MultiAgentRuntime {
    async fn run_task(
        &self,
        target: &TaskTarget,
        metadata: &WorkflowToolContext,
        workspace_dir: &std::path::Path,
        prompt: &str,
    ) -> Result<WorkflowTaskResult, String> {
        let agent_id = match target {
            TaskTarget::AgentId(agent_id) => {
                if !self.agents.contains_key(agent_id) {
                    return Err(format!("Workflow target agent '{}' not found.", agent_id));
                }
                agent_id.clone()
            }
            TaskTarget::Capabilities(required) => {
                let cards = self.registry().list_all().await;
                let mut matches = cards
                    .into_iter()
                    .filter(|card| card.status == AgentStatus::Online)
                    .filter(|card| {
                        required.iter().all(|required| {
                            card.capabilities
                                .iter()
                                .any(|capability| capability == required)
                        })
                    })
                    .map(|card| card.agent_id)
                    .collect::<Vec<_>>();
                matches.sort();
                match matches.as_slice() {
                    [agent_id] => agent_id.clone(),
                    [] => {
                        return Err(format!(
                            "No online agent matched workflow capabilities: {}",
                            required.join(", ")
                        ));
                    }
                    _ => {
                        return Err(format!(
                            "Multiple agents matched workflow capabilities {}: {}",
                            required.join(", "),
                            matches.join(", ")
                        ));
                    }
                }
            }
        };

        let agent = self
            .agents
            .get(&agent_id)
            .ok_or_else(|| format!("Workflow target agent '{}' not found.", agent_id))?;
        let session_id = format!(
            "wf-{}-{}-attempt-{}",
            metadata.run_id, metadata.node_id, metadata.attempt
        );
        let result = agent
            .run_detailed_with_context(
                &session_id,
                prompt,
                AgentExecutionContext {
                    workspace_dir: Some(workspace_dir.to_path_buf()),
                    workflow: Some(metadata.clone()),
                },
                None,
            )
            .await;

        Ok(WorkflowTaskResult {
            content: result.content.clone(),
            value: json!({
                "content": result.content,
                "agent_id": agent_id,
                "session_id": session_id,
                "attempt": metadata.attempt,
            }),
            agent_id,
            steps: result.steps,
        })
    }
}
// ---------------------------------------------------------------------------
// MultiAgentRuntimeBuilder
// ---------------------------------------------------------------------------

struct AgentSpec {
    agent_id: String,
    definition: AgentDefinition,
    capabilities: Vec<String>,
    llm: Option<Box<dyn LlmProvider>>,
    memory: Option<MemoryManager>,
    tool_registry: ToolRegistry,
    tool_executor: Option<Box<dyn ToolExecutor>>,
}

pub struct MultiAgentRuntimeBuilder {
    specs: Vec<AgentSpec>,
    selector: Option<Box<dyn PeerSelector>>,
    workspace_home: Option<PathBuf>,
}

impl MultiAgentRuntimeBuilder {
    pub fn new() -> Self {
        Self {
            specs: Vec::new(),
            selector: None,
            workspace_home: None,
        }
    }

    /// Add an agent with a definition and declared capabilities.
    pub fn add_agent(
        mut self,
        agent_id: impl Into<String>,
        definition: AgentDefinition,
        capabilities: Vec<String>,
    ) -> Self {
        self.specs.push(AgentSpec {
            agent_id: agent_id.into(),
            definition,
            capabilities,
            llm: None,
            memory: None,
            tool_registry: ToolRegistry::new(),
            tool_executor: None,
        });
        self
    }

    /// Add an agent with a custom LLM provider.
    pub fn add_agent_with_llm(
        mut self,
        agent_id: impl Into<String>,
        definition: AgentDefinition,
        capabilities: Vec<String>,
        llm: Box<dyn LlmProvider>,
    ) -> Self {
        self.specs.push(AgentSpec {
            agent_id: agent_id.into(),
            definition,
            capabilities,
            llm: Some(llm),
            memory: None,
            tool_registry: ToolRegistry::new(),
            tool_executor: None,
        });
        self
    }

    /// Add an agent with a full custom spec including tools, executor, memory,
    /// and LLM.
    pub fn add_agent_full(
        mut self,
        agent_id: impl Into<String>,
        definition: AgentDefinition,
        capabilities: Vec<String>,
        llm: Option<Box<dyn LlmProvider>>,
        memory: Option<MemoryManager>,
        tool_registry: ToolRegistry,
        tool_executor: Option<Box<dyn ToolExecutor>>,
    ) -> Self {
        self.specs.push(AgentSpec {
            agent_id: agent_id.into(),
            definition,
            capabilities,
            llm,
            memory,
            tool_registry,
            tool_executor,
        });
        self
    }

    pub fn with_selector(mut self, selector: Box<dyn PeerSelector>) -> Self {
        self.selector = Some(selector);
        self
    }

    pub fn with_workspace_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.workspace_home = Some(home.into());
        self
    }

    pub async fn build(self) -> Result<MultiAgentRuntime, String> {
        let registry = Arc::new(AgentRegistry::new());
        let selector = self
            .selector
            .unwrap_or_else(|| Box::new(FirstMatchSelector));

        // Phase 1: Build all agents (without delegation context yet — we need
        // the full agent map for the delegate_fn).
        let mut agents: HashMap<String, Arc<Agent>> = HashMap::new();

        for spec in &self.specs {
            // Register the agent card in the registry
            let card = AgentCard::new(
                &spec.agent_id,
                &spec.definition.name,
                &spec.definition.system_prompt_preamble,
                spec.capabilities.clone(),
            )
            .with_status(AgentStatus::Online);
            registry.register(card).await;
        }

        // Phase 2: Build agents with delegation tools injected.
        for spec in self.specs {
            let mut tool_registry = spec.tool_registry;

            // We inject placeholder delegation tools here. They'll work via
            // the DelegationContext attached to each ToolContext at call time.
            // The actual delegate_fn is wired below in Phase 3.
            tool_registry.insert(
                "discover_agents".to_string(),
                Box::new(DiscoverAgentsTool) as Box<dyn Tool>,
            );
            tool_registry.insert(
                "delegate_task".to_string(),
                Box::new(DelegateTaskTool) as Box<dyn Tool>,
            );

            let tool_executor = spec
                .tool_executor
                .unwrap_or_else(|| Box::new(RegistryToolExecutor));

            let agent = Agent::with_definition_tool_registry_executor_llm_and_workspace(
                spec.definition,
                tool_registry,
                tool_executor,
                spec.llm,
                spec.memory,
                self.workspace_home.clone(),
            )
            .await?;

            agents.insert(spec.agent_id, Arc::new(agent));
        }

        Ok(MultiAgentRuntime {
            registry,
            agents,
            selector,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentDefinition;
    use crate::llm::{
        ChatMessage, LlmConfig, LlmError, LlmProvider, LlmResponse, Result as LlmResult,
        ToolDefinition,
    };
    use async_trait::async_trait;
    use futures::stream;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    struct RecordingLlm {
        responses: Arc<Mutex<VecDeque<LlmResponse>>>,
    }

    impl RecordingLlm {
        fn new(responses: Vec<LlmResponse>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
            }
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
            _tools: &[ToolDefinition],
            _config: &LlmConfig,
        ) -> LlmResult<LlmResponse> {
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
            "core-next-multi-agent-tests-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn multi_agent_runtime_registers_agents() {
        let home = temp_home("register");
        let runtime = MultiAgentRuntime::builder()
            .add_agent_with_llm(
                "coder",
                AgentDefinition {
                    name: "Coder".into(),
                    system_prompt_preamble: "You write code.".into(),
                    model: "recording".into(),
                    max_iterations: 2,
                },
                vec!["code-gen".into()],
                Box::new(RecordingLlm::new(vec![LlmResponse {
                    content: "ok".into(),
                    usage: None,
                    tool_calls: vec![],
                    model: "recording".into(),
                    finish_reason: Some("stop".into()),
                }])),
            )
            .add_agent_with_llm(
                "researcher",
                AgentDefinition {
                    name: "Researcher".into(),
                    system_prompt_preamble: "You do research.".into(),
                    model: "recording".into(),
                    max_iterations: 2,
                },
                vec!["research".into()],
                Box::new(RecordingLlm::new(vec![LlmResponse {
                    content: "ok".into(),
                    usage: None,
                    tool_calls: vec![],
                    model: "recording".into(),
                    finish_reason: Some("stop".into()),
                }])),
            )
            .with_workspace_home(home)
            .build()
            .await
            .unwrap();

        let all = runtime.registry().list_all().await;
        assert_eq!(all.len(), 2);

        let coder = runtime.registry().get("coder").await.unwrap();
        assert_eq!(coder.capabilities, vec!["code-gen"]);
        assert_eq!(coder.status, AgentStatus::Online);
    }

    #[tokio::test]
    async fn multi_agent_runtime_processes_request() {
        let home = temp_home("process");
        let runtime = MultiAgentRuntime::builder()
            .add_agent_with_llm(
                "helper",
                AgentDefinition {
                    name: "Helper".into(),
                    system_prompt_preamble: "You help.".into(),
                    model: "recording".into(),
                    max_iterations: 2,
                },
                vec![],
                Box::new(RecordingLlm::new(vec![LlmResponse {
                    content: "I helped!".into(),
                    usage: None,
                    tool_calls: vec![],
                    model: "recording".into(),
                    finish_reason: Some("stop".into()),
                }])),
            )
            .with_workspace_home(home)
            .build()
            .await
            .unwrap();

        let response = runtime
            .process("helper", "s1", "do something")
            .await
            .unwrap();
        assert_eq!(response, "I helped!");
    }

    #[tokio::test]
    async fn multi_agent_runtime_unknown_agent_returns_error() {
        let home = temp_home("unknown");
        let runtime = MultiAgentRuntime::builder()
            .add_agent_with_llm(
                "only-agent",
                AgentDefinition {
                    name: "Only".into(),
                    system_prompt_preamble: "...".into(),
                    model: "recording".into(),
                    max_iterations: 2,
                },
                vec![],
                Box::new(RecordingLlm::new(vec![LlmResponse {
                    content: "ok".into(),
                    usage: None,
                    tool_calls: vec![],
                    model: "recording".into(),
                    finish_reason: Some("stop".into()),
                }])),
            )
            .with_workspace_home(home)
            .build()
            .await
            .unwrap();

        let result = runtime.process("ghost", "s1", "hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
