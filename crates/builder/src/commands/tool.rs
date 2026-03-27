use crate::cli::NewToolArgs;
use crate::manifest::Manifest;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

pub fn run_new(args: NewToolArgs) -> Result<(), String> {
    let manifest_path = args.manifest;
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| format!("Invalid manifest path '{}'.", manifest_path.display()))?;

    ensure_python_project(project_dir)?;

    let manifest = Manifest::load(&manifest_path)?;
    let tool_name = normalize_name(&args.name);
    if tool_name.is_empty() {
        return Err("Tool name must contain letters or numbers.".to_string());
    }

    let module_name = to_snake_case(&tool_name);
    let default_id = format!("{}-tools", to_kebab_case(&tool_name));
    let tool_id = args.id.unwrap_or(default_id);
    let tool_path = PathBuf::from("src")
        .join("tools")
        .join(format!("{module_name}.py"));
    let symbol = format!("register_{}_tools", module_name);

    if manifest.tools.iter().any(|tool| tool.id == tool_id) {
        return Err(format!(
            "Tool id '{}' already exists in {}.",
            tool_id,
            manifest_path.display()
        ));
    }

    let attach_agent = resolve_agent_target(&manifest, args.agent.as_deref())?;
    let absolute_tool_path = project_dir.join(&tool_path);
    if absolute_tool_path.exists() {
        return Err(format!(
            "Tool file '{}' already exists.",
            absolute_tool_path.display()
        ));
    }

    write_tool_file(&absolute_tool_path, &tool_name, &symbol)?;
    update_manifest(
        &manifest_path,
        &tool_id,
        &tool_path,
        &symbol,
        attach_agent.as_deref(),
    )?;

    println!();
    println!("\x1b[1;32m✓ Tool created!\x1b[0m");
    println!();
    println!("  \x1b[2mFiles:\x1b[0m");
    println!("    {}", tool_path.display());
    println!("    {}", manifest_path.display());
    println!();
    println!("  \x1b[2mRegistered:\x1b[0m");
    println!("    id = \"{}\"", tool_id);
    println!("    kind = \"python\"");
    println!("    path = \"{}\"", to_posix_path(&tool_path));
    println!("    symbol = \"{}\"", symbol);
    if let Some(agent_id) = attach_agent {
        println!("    attached to agent = \"{}\"", agent_id);
    } else {
        println!("    not attached to any agent");
    }
    println!();

    Ok(())
}

fn ensure_python_project(project_dir: &Path) -> Result<(), String> {
    if project_dir.join("pyproject.toml").exists() {
        return Ok(());
    }

    Err("Project-local tool scaffolding currently supports Python projects only.".to_string())
}

fn resolve_agent_target(
    manifest: &Manifest,
    requested_agent: Option<&str>,
) -> Result<Option<String>, String> {
    match requested_agent {
        Some(agent_id) => {
            if manifest.agents.iter().any(|agent| agent.id == agent_id) {
                Ok(Some(agent_id.to_string()))
            } else {
                Err(format!("Agent '{}' was not found in enki.toml.", agent_id))
            }
        }
        None if manifest.agents.len() == 1 => Ok(Some(manifest.agents[0].id.clone())),
        None => Ok(None),
    }
}

fn write_tool_file(path: &Path, tool_name: &str, symbol: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
    }

    let body = render_python_tool(tool_name, symbol);
    fs::write(path, body).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn update_manifest(
    manifest_path: &Path,
    tool_id: &str,
    tool_path: &Path,
    symbol: &str,
    attach_agent: Option<&str>,
) -> Result<(), String> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read {}: {e}", manifest_path.display()))?;
    let mut document: Value = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {e}", manifest_path.display()))?;

    let root = document.as_table_mut().ok_or_else(|| {
        format!(
            "{} must contain a TOML table at the root.",
            manifest_path.display()
        )
    })?;

    let tools = root
        .entry("tool")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "Expected [[tool]] entries to be an array.".to_string())?;

    let mut tool = toml::map::Map::new();
    tool.insert("id".to_string(), Value::String(tool_id.to_string()));
    tool.insert("kind".to_string(), Value::String("python".to_string()));
    tool.insert("path".to_string(), Value::String(to_posix_path(tool_path)));
    tool.insert("symbol".to_string(), Value::String(symbol.to_string()));
    tools.push(Value::Table(tool));

    if let Some(agent_id) = attach_agent {
        let agents = root
            .get_mut("agent")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "Expected [[agent]] entries to be an array.".to_string())?;

        let mut attached = false;
        for agent in agents {
            let Some(agent_table) = agent.as_table_mut() else {
                continue;
            };

            let matches = agent_table
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id == agent_id)
                .unwrap_or(false);
            if !matches {
                continue;
            }

            let tool_list = agent_table
                .entry("tools")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| format!("Agent '{}' has an invalid tools entry.", agent_id))?;

            if !tool_list
                .iter()
                .any(|value| value.as_str() == Some(tool_id))
            {
                tool_list.push(Value::String(tool_id.to_string()));
            }
            attached = true;
            break;
        }

        if !attached {
            return Err(format!("Agent '{}' was not found in enki.toml.", agent_id));
        }
    }

    let rendered = toml::to_string_pretty(&document)
        .map_err(|e| format!("Failed to serialize {}: {e}", manifest_path.display()))?;
    fs::write(manifest_path, rendered)
        .map_err(|e| format!("Failed to write {}: {e}", manifest_path.display()))
}

fn render_python_tool(tool_name: &str, symbol: &str) -> String {
    format!(
        r#"import json
from typing import Any

from enki_py import Agent


def {symbol}(agent: Agent, config: dict[str, Any] | None = None) -> None:
    """Register tools for {tool_name}."""
    tool_config = config or {{}}

    @agent.tool_plain
    def {tool_func}() -> str:
        """Return runtime metadata for this tool."""
        return json.dumps(
            {{
                "tool": "{tool_name}",
                "agent_id": tool_config.get("id"),
                "agent_name": tool_config.get("name"),
                "model": tool_config.get("model"),
            }}
        )
"#,
        symbol = symbol,
        tool_name = tool_name,
        tool_func = format!("{}_info", to_snake_case(tool_name))
    )
}

fn normalize_name(value: &str) -> String {
    value.trim().to_string()
}

fn to_snake_case(value: &str) -> String {
    split_words(value).join("_")
}

fn to_kebab_case(value: &str) -> String {
    split_words(value).join("-")
}

fn split_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn to_posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_python_defaults_from_name() {
        assert_eq!(to_snake_case("Weather Search"), "weather_search");
        assert_eq!(to_kebab_case("Weather Search"), "weather-search");
    }

    #[test]
    fn appends_tool_and_attaches_agent() {
        let dir = std::env::temp_dir().join("enki-tool-command-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest_path = dir.join("enki.toml");
        fs::write(
            &manifest_path,
            r#"[project]
name = "demo"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
"#,
        )
        .unwrap();

        update_manifest(
            &manifest_path,
            "weather-tools",
            Path::new("src/tools/weather.py"),
            "register_weather_tools",
            Some("assistant"),
        )
        .unwrap();

        let updated = fs::read_to_string(&manifest_path).unwrap();
        assert!(updated.contains("[[tool]]"));
        assert!(updated.contains("id = \"weather-tools\""));
        assert!(updated.contains("path = \"src/tools/weather.py\""));
        assert!(updated.contains("tools = [\"weather-tools\"]"));

        let _ = fs::remove_dir_all(&dir);
    }
}
