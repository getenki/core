use crate::cli::AddAgentArgs;
use std::fs;
use std::path::Path;

const PY_AGENT_TEMPLATE: &str = r#"from dataclasses import dataclass
from enki_py import Agent, RunContext

@dataclass
class AgentDeps:
    api_key: str = "dummy_key"

class {AGENT_CLASS_NAME}(Agent[AgentDeps]):
    def __init__(self, model: str, name: str):
        super().__init__(model, name=name, deps_type=AgentDeps)
        self.tool(self.dummy_tool)

    def dummy_tool(self, ctx: RunContext[AgentDeps], query: str) -> str:
        """A dummy tool to demonstrate tool calling and dependencies."""
        return f"Dummy result for '{query}' using key '{ctx.deps.api_key}'"

deps = AgentDeps()
agent = {AGENT_CLASS_NAME}("ollama::qwen3.5", name="{AGENT_NAME}")
"#;

fn to_pascal_case(s: &str) -> String {
    let mut pascal = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c.is_whitespace() {
            capitalize_next = true;
        } else if capitalize_next {
            pascal.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            pascal.push(c.to_ascii_lowercase());
        }
    }
    pascal
}

pub fn run(args: AddAgentArgs) -> Result<(), String> {
    if !args.manifest.exists() {
        return Err(format!("Manifest file not found at {}.", args.manifest.display()));
    }

    let mut content = fs::read_to_string(&args.manifest)
        .map_err(|e| format!("Failed to read manifest: {e}"))?;

    let agent_id = args.name.clone(); // In real world it could be slugified
    let agent_name = args.name.clone();

    let script_path = if args.script {
        let path = format!("src/agents/{}.py", agent_id);
        Some(path)
    } else {
        None
    };

    let mut new_block = format!(
        "\n[[agent]]\nid = \"{}\"\nname = \"{}\"\nmodel = \"ollama::qwen3.5\"\nsystem_prompt = \"You are a helpful assistant.\"\n",
        agent_id, agent_name
    );

    if let Some(ref path) = script_path {
        new_block.push_str(&format!("script = \"{}\"\n", path));
    }

    content.push_str(&new_block);

    fs::write(&args.manifest, content)
        .map_err(|e| format!("Failed to update manifest: {e}"))?;

    println!("\x1b[1;32m✓\x1b[0m Registered agent '{}' in {}.", agent_name, args.manifest.display());

    if let Some(path_str) = script_path {
        let manifest_dir = args.manifest.parent().unwrap_or(Path::new("."));
        let full_path = manifest_dir.join(&path_str);
        
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create agent directory: {e}"))?;
        }

        let agent_class_base = to_pascal_case(&agent_name);
        let agent_class_name = format!("{}Agent", agent_class_base);

        let script_content = PY_AGENT_TEMPLATE
            .replace("{AGENT_NAME}", &agent_name)
            .replace("{AGENT_CLASS_NAME}", &agent_class_name);
        fs::write(&full_path, script_content)
            .map_err(|e| format!("Failed to write agent script: {e}"))?;
            
        println!("\x1b[1;32m✓\x1b[0m Created boilerplate agent script at {}.", full_path.display());
    }

    Ok(())
}
