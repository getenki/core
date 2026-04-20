use crate::tooling::types::WorkflowToolContext;
use crate::tooling::types::DelegationContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// Config
const DEFAULT_MAX_ITERATIONS: usize = 20;

pub struct AgentDefinition {
    pub name: String,
    pub system_prompt_preamble: String,
    pub model: String,
    pub max_iterations: usize,
}

impl Default for AgentDefinition {
    fn default() -> Self {
        Self {
            name: "Personal Assistant".to_string(),
            system_prompt_preamble: "You are a helpful Personal Assistant agent.".to_string(),
            model: String::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }
}

#[derive(Clone, Default)]
pub struct AgentExecutionContext {
    pub workspace_dir: Option<PathBuf>,
    pub workflow: Option<WorkflowToolContext>,
    pub delegation: Option<DelegationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub index: usize,
    pub phase: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResult {
    pub content: String,
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallTrace {
    pub name: String,
    pub args: Value,
    pub call_id: Option<String>,
    pub result: String,
}

pub enum StepOutcome {
    Continue {
        tool_names: Vec<String>,
        tool_traces: Vec<ToolCallTrace>,
    },
    Final(String),
}

pub struct ToolInvocation {
    pub name: String,
    pub args: Value,
    pub call_id: Option<String>,
}
