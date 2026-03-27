use serde::Deserialize;
use std::path::Path;

/// Root of the `enki.toml` manifest.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectConfig,

    #[serde(default)]
    pub workspace: WorkspaceConfig,

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
}

fn default_max_iterations() -> usize {
    20
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

        let manifest: Manifest =
            toml::from_str(&content).map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

        if manifest.agents.is_empty() {
            return Err(format!(
                "No [[agent]] entries found in {}. Define at least one agent.",
                path.display()
            ));
        }

        Ok(manifest)
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

[[agent]]
id = "coder"
name = "Coder"
model = "openai::gpt-4o"
system_prompt = "You write code."
max_iterations = 10
capabilities = ["code-gen", "refactoring"]

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
        assert_eq!(manifest.agents.len(), 2);
        assert_eq!(manifest.agents[0].capabilities, vec!["code-gen", "refactoring"]);
        assert_eq!(manifest.agents[1].max_iterations, 5);
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
