use enki_next::workflow::{
    RetryPolicy, TaskDefinition, TaskTarget, WorkflowDefinition, WorkflowEdgeDefinition,
    WorkflowEdgeTransition, WorkflowFailurePolicy, WorkflowNodeDefinition, WorkflowNodeKind,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Root of the `enki.toml` manifest.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectConfig,

    #[serde(default)]
    pub workspace: WorkspaceConfig,

    #[serde(default, alias = "workflow_tomls", alias = "workflow_paths")]
    pub workflow_files: Vec<String>,

    #[serde(rename = "tool", default)]
    pub tools: Vec<ToolConfig>,

    #[serde(rename = "transform", default)]
    pub transforms: Vec<TransformConfig>,

    #[serde(rename = "task", default)]
    pub tasks: Vec<TaskConfig>,

    #[serde(rename = "workflow", default)]
    pub workflows: Vec<WorkflowConfig>,

    #[serde(rename = "agent", default)]
    pub agents: Vec<AgentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,

    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default, alias = "workflow_tomls", alias = "workflow_paths")]
    pub workflow_files: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default = "default_home")]
    pub home: String,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            home: default_home(),
        }
    }
}

fn default_home() -> String {
    "./.enki".to_string()
}

#[derive(Debug, Deserialize, Default)]
struct WorkflowFileConfig {
    #[serde(rename = "transform", default)]
    transforms: Vec<TransformConfig>,

    #[serde(rename = "task", default)]
    tasks: Vec<TaskConfig>,

    #[serde(rename = "workflow", default)]
    workflows: Vec<WorkflowConfig>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ToolConfig {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub symbol: String,
}

impl ToolConfig {
    pub fn is_python(&self) -> bool {
        self.kind.eq_ignore_ascii_case("python")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TransformConfig {
    pub id: String,
    #[serde(default = "default_builtin_kind")]
    pub kind: String,
    pub path: Option<String>,
    pub symbol: Option<String>,
}

fn default_builtin_kind() -> String {
    "builtin".to_string()
}

#[derive(Clone, Debug, Deserialize)]
pub struct TaskConfig {
    pub id: String,
    pub agent: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub prompt: String,
    #[serde(default)]
    pub input_bindings: BTreeMap<String, String>,
    pub input_transform: Option<String>,
    pub output_transform: Option<String>,
    pub output_key: Option<String>,
    pub max_attempts: Option<usize>,
    pub failure_policy: Option<String>,
}

impl TaskConfig {
    pub fn to_core(&self) -> Result<TaskDefinition, String> {
        Ok(TaskDefinition {
            id: self.id.clone(),
            target: target_from_parts(self.agent.as_deref(), &self.capabilities)?,
            prompt: self.prompt.clone(),
            input_bindings: self.input_bindings.clone(),
            input_transform: self.input_transform.clone(),
            output_transform: self.output_transform.clone(),
            output_key: self.output_key.clone(),
            retry_policy: self
                .max_attempts
                .map(|max_attempts| RetryPolicy { max_attempts }),
            failure_policy: parse_failure_policy(self.failure_policy.as_deref())?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkflowConfig {
    pub id: String,
    pub name: Option<String>,
    pub max_attempts: Option<usize>,
    pub failure_policy: Option<String>,
    #[serde(rename = "node", default)]
    pub nodes: Vec<WorkflowNodeConfig>,
    #[serde(rename = "edge", default)]
    pub edges: Vec<WorkflowEdgeConfig>,
}

impl WorkflowConfig {
    pub fn to_core(&self) -> Result<WorkflowDefinition, String> {
        Ok(WorkflowDefinition {
            id: self.id.clone(),
            name: self.name.clone().unwrap_or_else(|| self.id.clone()),
            nodes: self
                .nodes
                .iter()
                .map(|node| node.to_core(&self.id))
                .collect::<Result<Vec<_>, _>>()?,
            edges: self
                .edges
                .iter()
                .map(WorkflowEdgeConfig::to_core)
                .collect::<Result<Vec<_>, _>>()?,
            retry_policy: self
                .max_attempts
                .map(|max_attempts| RetryPolicy { max_attempts }),
            failure_policy: parse_failure_policy(self.failure_policy.as_deref())?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkflowNodeConfig {
    pub id: String,
    pub kind: String,
    pub task: Option<String>,
    pub agent: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub prompt: Option<String>,
    pub condition: Option<String>,
    pub transform: Option<String>,
    pub input_key: Option<String>,
    pub output_key: Option<String>,
    pub max_attempts: Option<usize>,
    pub failure_policy: Option<String>,
}

impl WorkflowNodeConfig {
    pub fn to_core(&self, workflow_id: &str) -> Result<WorkflowNodeDefinition, String> {
        let kind = match self.kind.as_str() {
            "task" => WorkflowNodeKind::Task {
                task_id: self.task.clone(),
                task: if self.task.is_some() {
                    None
                } else {
                    Some(TaskDefinition {
                        id: format!("{workflow_id}.{}", self.id),
                        target: target_from_parts(self.agent.as_deref(), &self.capabilities)?,
                        prompt: self.prompt.clone().ok_or_else(|| {
                            format!(
                                "Workflow node '{}' must define prompt for inline task.",
                                self.id
                            )
                        })?,
                        input_bindings: BTreeMap::new(),
                        input_transform: None,
                        output_transform: None,
                        output_key: self.output_key.clone(),
                        retry_policy: self
                            .max_attempts
                            .map(|max_attempts| RetryPolicy { max_attempts }),
                        failure_policy: parse_failure_policy(self.failure_policy.as_deref())?,
                    })
                },
            },
            "decision" => WorkflowNodeKind::Decision {
                condition: self.condition.clone().ok_or_else(|| {
                    format!(
                        "Workflow decision node '{}' must define condition.",
                        self.id
                    )
                })?,
            },
            "human_gate" => WorkflowNodeKind::HumanGate {
                prompt: self.prompt.clone().ok_or_else(|| {
                    format!("Workflow human_gate node '{}' must define prompt.", self.id)
                })?,
            },
            "transform" => WorkflowNodeKind::Transform {
                transform_id: self.transform.clone().ok_or_else(|| {
                    format!(
                        "Workflow transform node '{}' must define transform.",
                        self.id
                    )
                })?,
                input_key: self.input_key.clone(),
            },
            "join" => WorkflowNodeKind::Join,
            other => return Err(format!("Unknown workflow node kind '{}'.", other)),
        };

        Ok(WorkflowNodeDefinition {
            id: self.id.clone(),
            kind,
            output_key: self.output_key.clone(),
            retry_policy: self
                .max_attempts
                .map(|max_attempts| RetryPolicy { max_attempts }),
            failure_policy: parse_failure_policy(self.failure_policy.as_deref())?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkflowEdgeConfig {
    pub from: String,
    pub to: String,
    pub on: Option<String>,
    pub condition: Option<String>,
}

impl WorkflowEdgeConfig {
    pub fn to_core(&self) -> Result<WorkflowEdgeDefinition, String> {
        let transition = match (self.on.as_deref(), self.condition.as_deref()) {
            (_, Some(condition)) => WorkflowEdgeTransition::Condition(condition.to_string()),
            (None, None) => WorkflowEdgeTransition::OnSuccess,
            (Some("always"), None) => WorkflowEdgeTransition::Always,
            (Some("success"), None) => WorkflowEdgeTransition::OnSuccess,
            (Some("failure"), None) => WorkflowEdgeTransition::OnFailure,
            (Some(value), None) if value.starts_with("condition:") => {
                WorkflowEdgeTransition::Condition(
                    value.trim_start_matches("condition:").trim().to_string(),
                )
            }
            (Some(value), None) => {
                return Err(format!("Unknown workflow edge transition '{}'.", value));
            }
        };

        Ok(WorkflowEdgeDefinition {
            from: self.from.clone(),
            to: self.to.clone(),
            transition,
        })
    }
}
#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub model: String,

    #[serde(default)]
    pub system_prompt: String,

    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,

    #[serde(default)]
    pub capabilities: Vec<String>,

    #[serde(default)]
    pub tools: Vec<String>,

    pub script: Option<String>,
}

fn default_max_iterations() -> usize {
    20
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        let mut manifest: Manifest = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

        manifest.load_workflow_files(path)?;
        manifest.validate_unique_workflow_items()?;

        if manifest.agents.is_empty() {
            return Err(format!(
                "No [[agent]] entries found in {}. Define at least one agent.",
                path.display()
            ));
        }

        for agent in &manifest.agents {
            for tool_id in &agent.tools {
                if manifest.tools.iter().all(|tool| tool.id != *tool_id) {
                    return Err(format!(
                        "Agent '{}' references unknown tool '{}'.",
                        agent.id, tool_id
                    ));
                }
            }
        }

        for workflow in &manifest.workflows {
            for node in &workflow.nodes {
                if let Some(task_id) = &node.task {
                    if manifest.tasks.iter().all(|task| task.id != *task_id) {
                        return Err(format!(
                            "Workflow '{}' node '{}' references unknown task '{}'.",
                            workflow.id, node.id, task_id
                        ));
                    }
                }
                if let Some(transform_id) = &node.transform {
                    if !is_builtin_transform(transform_id)
                        && manifest
                            .transforms
                            .iter()
                            .all(|transform| transform.id != *transform_id)
                    {
                        return Err(format!(
                            "Workflow '{}' node '{}' references unknown transform '{}'.",
                            workflow.id, node.id, transform_id
                        ));
                    }
                }
            }
        }

        for task in &manifest.tasks {
            if let Some(transform_id) = &task.input_transform {
                manifest.validate_transform_reference(transform_id)?;
            }
            if let Some(transform_id) = &task.output_transform {
                manifest.validate_transform_reference(transform_id)?;
            }
        }

        Ok(manifest)
    }

    pub fn resolve_tools(&self, agent: &AgentConfig) -> Vec<ToolConfig> {
        agent
            .tools
            .iter()
            .filter_map(|tool_id| self.tools.iter().find(|tool| tool.id == *tool_id))
            .cloned()
            .collect()
    }

    pub fn workflow_tasks(&self) -> Result<Vec<TaskDefinition>, String> {
        self.tasks.iter().map(TaskConfig::to_core).collect()
    }

    pub fn workflow_definitions(&self) -> Result<Vec<WorkflowDefinition>, String> {
        self.workflows.iter().map(WorkflowConfig::to_core).collect()
    }
    fn load_workflow_files(&mut self, path: &Path) -> Result<(), String> {
        let mut workflow_files = self.workflow_files.clone();
        workflow_files.extend(self.project.workflow_files.iter().cloned());

        for workflow_file in workflow_files {
            if workflow_file.trim().is_empty() {
                return Err("Workflow include path cannot be empty.".to_string());
            }
            let include_path = resolve_include_path(path, &workflow_file);
            let content = std::fs::read_to_string(&include_path).map_err(|e| {
                format!(
                    "Failed to read workflow include {}: {e}",
                    include_path.display()
                )
            })?;
            let mut include: WorkflowFileConfig = toml::from_str(&content).map_err(|e| {
                format!(
                    "Failed to parse workflow include {}: {e}",
                    include_path.display()
                )
            })?;

            self.transforms.append(&mut include.transforms);
            self.tasks.append(&mut include.tasks);
            self.workflows.append(&mut include.workflows);
        }

        Ok(())
    }

    fn validate_unique_workflow_items(&self) -> Result<(), String> {
        let mut transform_ids = BTreeSet::new();
        for transform in &self.transforms {
            if !transform_ids.insert(transform.id.clone()) {
                return Err(format!(
                    "Duplicate workflow transform id '{}'.",
                    transform.id
                ));
            }
        }

        let mut task_ids = BTreeSet::new();
        for task in &self.tasks {
            if !task_ids.insert(task.id.clone()) {
                return Err(format!("Duplicate workflow task id '{}'.", task.id));
            }
        }

        let mut workflow_ids = BTreeSet::new();
        for workflow in &self.workflows {
            if !workflow_ids.insert(workflow.id.clone()) {
                return Err(format!("Duplicate workflow id '{}'.", workflow.id));
            }
        }

        Ok(())
    }

    fn validate_transform_reference(&self, transform_id: &str) -> Result<(), String> {
        if is_builtin_transform(transform_id)
            || self
                .transforms
                .iter()
                .any(|transform| transform.id == transform_id)
        {
            return Ok(());
        }
        Err(format!("Unknown workflow transform '{}'.", transform_id))
    }
}

fn resolve_include_path(manifest_path: &Path, include: &str) -> PathBuf {
    let include_path = Path::new(include);
    if include_path.is_absolute() {
        include_path.to_path_buf()
    } else {
        manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(include_path)
    }
}

fn target_from_parts(agent: Option<&str>, capabilities: &[String]) -> Result<TaskTarget, String> {
    if let Some(agent_id) = agent {
        if !agent_id.trim().is_empty() {
            return Ok(TaskTarget::AgentId(agent_id.to_string()));
        }
    }
    if !capabilities.is_empty() {
        return Ok(TaskTarget::Capabilities(capabilities.to_vec()));
    }
    Err("Task target must define either `agent` or `capabilities`.".to_string())
}

fn parse_failure_policy(raw: Option<&str>) -> Result<Option<WorkflowFailurePolicy>, String> {
    match raw {
        None => Ok(None),
        Some("continue_best_effort") => Ok(Some(WorkflowFailurePolicy::ContinueBestEffort)),
        Some("fail") | Some("fail_workflow") => Ok(Some(WorkflowFailurePolicy::FailWorkflow)),
        Some("pause") | Some("pause_for_intervention") => {
            Ok(Some(WorkflowFailurePolicy::PauseForIntervention))
        }
        Some(value) => Err(format!("Unknown workflow failure policy '{}'.", value)),
    }
}

fn is_builtin_transform(transform_id: &str) -> bool {
    matches!(transform_id, "identity" | "extract_content")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[project]
name = "test-project"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "test-project");
        assert_eq!(manifest.project.version, "0.1.0");
        assert_eq!(manifest.agents.len(), 1);
        assert_eq!(manifest.agents[0].id, "assistant");
        assert_eq!(manifest.agents[0].max_iterations, 20);
        assert!(manifest.agents[0].tools.is_empty());
        assert!(manifest.tasks.is_empty());
        assert!(manifest.workflows.is_empty());
        assert!(manifest.workflow_files.is_empty());
        assert!(manifest.project.workflow_files.is_empty());
        assert_eq!(manifest.workspace.home, "./.enki");
    }

    #[test]
    fn parse_workflow_manifest() {
        let toml_str = r#"
[project]
name = "workflow-project"

[[agent]]
id = "researcher"
name = "Researcher"
model = "ollama::qwen3.5"
capabilities = ["research"]

[[task]]
id = "research-topic"
capabilities = ["research"]
prompt = "Research {{input.topic}}"
output_key = "research"
failure_policy = "continue_best_effort"

[[workflow]]
id = "research-flow"
name = "Research Flow"
max_attempts = 2
failure_policy = "pause_for_intervention"

[[workflow.node]]
id = "research"
kind = "task"
task = "research-topic"

[[workflow.node]]
id = "approval"
kind = "human_gate"
prompt = "Approve research output?"

[[workflow.edge]]
from = "research"
to = "approval"
on = "success"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        let tasks = manifest.workflow_tasks().unwrap();
        let workflows = manifest.workflow_definitions().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].nodes.len(), 2);
        assert_eq!(workflows[0].edges.len(), 1);
    }

    fn temp_manifest_dir(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}", label, unique))
    }

    #[test]
    fn load_workflow_tomls_from_project_manifest() {
        let root = temp_manifest_dir("enki-workflow-include");
        std::fs::create_dir_all(root.join("workflows")).unwrap();

        let manifest_path = root.join("enki.toml");
        std::fs::write(
            &manifest_path,
            r#"
[project]
name = "workflow-project"
workflow_files = ["workflows/release.toml"]

[[agent]]
id = "researcher"
name = "Researcher"
model = "ollama::qwen3.5"
capabilities = ["research"]
"#,
        )
        .unwrap();

        std::fs::write(
            root.join("workflows").join("release.toml"),
            r#"
[[task]]
id = "research-topic"
capabilities = ["research"]
prompt = "Research {{input.topic}}"
output_key = "research"

[[workflow]]
id = "research-flow"
name = "Research Flow"

[[workflow.node]]
id = "research"
kind = "task"
task = "research-topic"
"#,
        )
        .unwrap();

        let manifest = Manifest::load(&manifest_path).unwrap();
        let tasks = manifest.workflow_tasks().unwrap();
        let workflows = manifest.workflow_definitions().unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "research-topic");
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].id, "research-flow");
        assert_eq!(workflows[0].nodes.len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reject_unknown_tool_reference() {
        let toml_str = r#"
[project]
name = "bad-tool"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
tools = ["missing"]
"#;
        let tmp = std::env::temp_dir().join("enki-test-missing-tool.toml");
        std::fs::write(&tmp, toml_str).unwrap();
        let result = Manifest::load(&tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tool"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn reject_empty_agents() {
        let toml_str = r#"
[project]
name = "empty"
"#;
        let tmp = std::env::temp_dir().join("enki-test-empty.toml");
        std::fs::write(&tmp, toml_str).unwrap();
        let result = Manifest::load(&tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No [[agent]]"));
        let _ = std::fs::remove_file(&tmp);
    }
}
