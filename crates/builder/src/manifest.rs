use serde::Deserialize;
use std::path::Path;

/// Root of the `enki.toml` manifest.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectConfig,

    #[serde(default)]
    pub workspace: WorkspaceConfig,

    #[serde(rename = "tool", default)]
    pub tools: Vec<ToolConfig>,

    #[serde(rename = "agent", default)]
    pub agents: Vec<AgentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,

    #[serde(default = "default_version")]
    pub version: String,
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
}

fn default_max_iterations() -> usize {
    20
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        let manifest: Manifest = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

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
        assert_eq!(manifest.workspace.home, "./.enki");
    }

    #[test]
    fn parse_full_manifest() {
        let toml_str = r#"
[project]
name = "my-agents"
version = "0.2.0"

[workspace]
home = "./workspace"

[[tool]]
id = "coder-tools"
kind = "python"
path = "src/tools/coder.py"
symbol = "register_coder_tools"

[[agent]]
id = "coder"
name = "Coder"
model = "openai::gpt-4o"
system_prompt = "You write code."
max_iterations = 10
capabilities = ["code-gen", "refactoring"]
tools = ["coder-tools"]

[[agent]]
id = "researcher"
name = "Researcher"
model = "anthropic::claude-3-opus-20240229"
system_prompt = "You do research."
max_iterations = 5
capabilities = ["research"]
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "my-agents");
        assert_eq!(manifest.project.version, "0.2.0");
        assert_eq!(manifest.workspace.home, "./workspace");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.agents.len(), 2);
        assert_eq!(
            manifest.agents[0].capabilities,
            vec!["code-gen", "refactoring"]
        );
        assert_eq!(manifest.agents[0].tools, vec!["coder-tools"]);
        assert_eq!(
            manifest.resolve_tools(&manifest.agents[0]),
            vec![ToolConfig {
                id: "coder-tools".into(),
                kind: "python".into(),
                path: "src/tools/coder.py".into(),
                symbol: "register_coder_tools".into(),
            }]
        );
        assert_eq!(manifest.agents[1].max_iterations, 5);
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
