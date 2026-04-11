use crate::agent::ExecutionStep;
use crate::tooling::types::WorkflowToolContext;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFailurePolicy {
    ContinueBestEffort,
    FailWorkflow,
    PauseForIntervention,
}

impl Default for WorkflowFailurePolicy {
    fn default() -> Self {
        Self::ContinueBestEffort
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: usize,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TaskTarget {
    AgentId(String),
    Capabilities(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskDefinition {
    pub id: String,
    pub target: TaskTarget,
    pub prompt: String,
    #[serde(default)]
    pub input_bindings: BTreeMap<String, String>,
    pub input_transform: Option<String>,
    pub output_transform: Option<String>,
    pub output_key: Option<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub failure_policy: Option<WorkflowFailurePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Task {
        task_id: Option<String>,
        task: Option<TaskDefinition>,
    },
    Decision {
        condition: String,
    },
    HumanGate {
        prompt: String,
    },
    Transform {
        transform_id: String,
        input_key: Option<String>,
    },
    Join,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowNodeDefinition {
    pub id: String,
    #[serde(flatten)]
    pub kind: WorkflowNodeKind,
    pub output_key: Option<String>,
    pub retry_policy: Option<RetryPolicy>,
    pub failure_policy: Option<WorkflowFailurePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum WorkflowEdgeTransition {
    Always,
    OnSuccess,
    OnFailure,
    Condition(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowEdgeDefinition {
    pub from: String,
    pub to: String,
    pub transition: WorkflowEdgeTransition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<WorkflowNodeDefinition>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdgeDefinition>,
    pub retry_policy: Option<RetryPolicy>,
    pub failure_policy: Option<WorkflowFailurePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkflowContext {
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

impl WorkflowContext {
    pub fn insert(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        for (key, value) in &self.values {
            map.insert(key.clone(), value.clone());
        }
        Value::Object(map)
    }

    pub fn lookup_path(&self, path: &str) -> Option<Value> {
        if path.is_empty() {
            return None;
        }

        let mut segments = path.split('.');
        let first = segments.next()?;
        let mut current = self.values.get(first)?.clone();
        for segment in segments {
            current = match current {
                Value::Object(map) => map.get(segment)?.clone(),
                _ => return None,
            };
        }
        Some(current)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Paused,
}

impl NodeStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Skipped)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeRunState {
    pub node_id: String,
    pub status: NodeStatus,
    pub attempts: usize,
    pub started_at: Option<u128>,
    pub completed_at: Option<u128>,
    pub last_error: Option<String>,
    pub output_key: String,
    pub output: Option<Value>,
    #[serde(default)]
    pub activated_incoming: Vec<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterventionStatus {
    Pending,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterventionRequest {
    pub id: String,
    pub workflow_id: String,
    pub run_id: String,
    pub node_id: String,
    pub prompt: String,
    pub reason: String,
    pub response: Option<String>,
    pub status: InterventionStatus,
    pub created_at: u128,
    pub resolved_at: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    Failed,
    Completed,
    CompletedWithFailures,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRunState {
    pub workflow_id: String,
    pub run_id: String,
    pub status: WorkflowStatus,
    pub created_at: u128,
    pub updated_at: u128,
    pub input: Value,
    pub context: WorkflowContext,
    pub node_states: BTreeMap<String, NodeRunState>,
    #[serde(default)]
    pub pending_interventions: Vec<InterventionRequest>,
    #[serde(default)]
    pub failed_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRequest {
    pub workflow_id: String,
    #[serde(default = "default_input")]
    pub input: Value,
}

fn default_input() -> Value {
    json!({})
}

impl WorkflowRequest {
    pub fn new(workflow_id: impl Into<String>, input: Value) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            input,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowResponse {
    pub workflow_id: String,
    pub run_id: String,
    pub status: WorkflowStatus,
    pub context: WorkflowContext,
    #[serde(default)]
    pub events: Vec<WorkflowEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEvent {
    WorkflowStarted {
        workflow_id: String,
        run_id: String,
    },
    NodeReady {
        node_id: String,
    },
    NodeStarted {
        node_id: String,
        attempt: usize,
    },
    NodeCompleted {
        node_id: String,
        output_key: String,
    },
    NodeFailed {
        node_id: String,
        error: String,
    },
    NodeRetryScheduled {
        node_id: String,
        attempt: usize,
        error: String,
    },
    NodeSkipped {
        node_id: String,
    },
    InterventionRequested {
        intervention_id: String,
        node_id: String,
        reason: String,
    },
    InterventionResolved {
        intervention_id: String,
        node_id: String,
    },
    WorkflowPaused {
        run_id: String,
        reason: String,
    },
    WorkflowCompleted {
        run_id: String,
        status: WorkflowStatus,
    },
}

#[async_trait(?Send)]
pub trait WorkflowEventListener: 'static {
    async fn on_event(&self, event: &WorkflowEvent) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowTaskResult {
    pub content: String,
    pub value: Value,
    pub agent_id: String,
    pub steps: Vec<ExecutionStep>,
}

#[async_trait(?Send)]
pub trait WorkflowTaskRunner: 'static {
    async fn run_task(
        &self,
        target: &TaskTarget,
        metadata: &WorkflowToolContext,
        workspace_dir: &std::path::Path,
        prompt: &str,
    ) -> Result<WorkflowTaskResult, String>;
}

#[async_trait(?Send)]
pub trait WorkflowTransform: 'static {
    async fn apply(&self, input: &Value, context: &WorkflowContext) -> Result<Value, String>;
}

pub type TransformRegistry = BTreeMap<String, Arc<dyn WorkflowTransform>>;
