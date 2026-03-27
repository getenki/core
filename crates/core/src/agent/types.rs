use serde_json::Value;

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

pub enum StepOutcome {
    Continue,
    Final(String),
}

pub struct ToolInvocation {
    pub name: String,
    pub args: Value,
    pub call_id: Option<String>,
}
