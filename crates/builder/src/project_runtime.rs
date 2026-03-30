use crate::manifest::{Manifest, ToolConfig};
use std::io;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const CAPABILITY_SEPARATOR: char = '\u{1f}';
const TOOL_SEPARATOR: char = '\u{1e}';
pub fn is_python_project(project_dir: &Path) -> bool {
    project_dir.join("pyproject.toml").exists()
}

pub fn validate_python_tools(manifest: &Manifest, project_dir: &Path) -> Result<(), String> {
    let configured_agents = manifest
        .agents
        .iter()
        .filter(|agent| !manifest.resolve_tools(agent).is_empty())
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();

    if configured_agents.is_empty() {
        return Ok(());
    }

    if !is_python_project(project_dir) {
        return Err(format!(
            "`tools` is only supported for Python projects right now. Remove it from agent(s): {}",
            configured_agents.join(", ")
        ));
    }

    for tool in &manifest.tools {
        if !tool.is_python() {
            return Err(format!(
                "Unsupported tool kind '{}' for tool '{}'.",
                tool.kind, tool.id
            ));
        }

        let entry = project_dir.join(&tool.path);
        if !entry.exists() {
            return Err(format!(
                "Configured Python tool '{}' requires a module at {}.",
                tool.id,
                entry.display()
            ));
        }
    }

    Ok(())
}

pub async fn run_python_agent(
    manifest: &Manifest,
    project_dir: &Path,
    workspace_home: &str,
    agent_id: &str,
    session_id: &str,
    message: &str,
    stream: bool,
) -> Result<String, String> {
    let args = python_runner_args(
        manifest,
        project_dir,
        workspace_home,
        agent_id,
        session_id,
        message,
    );

    let mut last_not_found: Option<io::Error> = None;
    let mut output = None;

    for candidate in python_candidates(project_dir) {
        let mut candidate_args = candidate.prefix_args.clone();
        candidate_args.extend(args.iter().cloned());

        match spawn_python(project_dir, &candidate.program, &candidate_args, stream).await {
            Ok(result) => {
                output = Some(result);
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                last_not_found = Some(err);
            }
            Err(err) => return Err(format!("Failed to start Python runtime: {err}")),
        }
    }

    let output = match output {
        Some(output) => output,
        None => {
            return Err(match last_not_found {
                Some(err) => format!("Failed to locate a Python runtime for this project: {err}"),
                None => "Failed to locate a Python runtime for this project.".to_string(),
            });
        }
    };

    Ok(output)
}

pub async fn run_script_agent(
    project_dir: &Path,
    script_path: &str,
    session_id: &str,
    message: &str,
    stream: bool,
) -> Result<String, String> {
    let mut last_not_found: Option<io::Error> = None;
    let mut output = None;

    let args = vec![
        script_path.to_string(),
        session_id.to_string(),
        message.to_string(),
    ];

    for candidate in python_candidates(project_dir) {
        let mut candidate_args = candidate.prefix_args.clone();
        candidate_args.extend(args.iter().cloned());

        match spawn_python(project_dir, &candidate.program, &candidate_args, stream).await {
            Ok(result) => {
                output = Some(result);
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                last_not_found = Some(err);
            }
            Err(err) => return Err(format!("Failed to start Python runtime: {err}")),
        }
    }

    let output = match output {
        Some(output) => output,
        None => {
            return Err(match last_not_found {
                Some(err) => format!("Failed to locate a Python runtime for this project: {err}"),
                None => "Failed to locate a Python runtime for this project.".to_string(),
            });
        }
    };

    Ok(output)
}

fn python_runner_args(
    manifest: &Manifest,
    project_dir: &Path,
    workspace_home: &str,
    agent_id: &str,
    session_id: &str,
    message: &str,
) -> Vec<String> {
    let mut args = vec![
        "-m".to_string(),
        "enki_py.builder".to_string(),
        project_dir.to_string_lossy().to_string(),
        workspace_home.to_string(),
        agent_id.to_string(),
        session_id.to_string(),
        message.to_string(),
        manifest.agents.len().to_string(),
    ];

    for agent_cfg in &manifest.agents {
        let tools = manifest.resolve_tools(agent_cfg);
        args.push(agent_cfg.id.clone());
        args.push(agent_cfg.name.clone());
        args.push(agent_cfg.model.clone());
        args.push(agent_cfg.system_prompt.clone());
        args.push(agent_cfg.max_iterations.to_string());
        args.push(
            agent_cfg
                .capabilities
                .join(&CAPABILITY_SEPARATOR.to_string()),
        );
        args.push(serialize_tools(&tools));
        args.push(agent_cfg.script.clone().unwrap_or_default());
    }

    args
}

fn python_candidates(project_dir: &Path) -> Vec<PythonCandidate> {
    let mut candidates = Vec::new();
    let search_root = canonical_project_dir(project_dir);

    if let Some(venv_home) = std::env::var_os("VIRTUAL_ENV") {
        if let Some(program) = venv_python_path(Path::new(&venv_home)) {
            candidates.push(PythonCandidate::new(program));
        }
    }

    for ancestor in search_root.ancestors() {
        for dir_name in [".venv", "venv"] {
            if let Some(program) = venv_python_path(&ancestor.join(dir_name)) {
                if !candidates
                    .iter()
                    .any(|candidate| candidate.program == program)
                {
                    candidates.push(PythonCandidate::new(program));
                }
            }
        }
    }

    candidates.push(PythonCandidate::new("python"));
    candidates.push(PythonCandidate::with_prefix("py", vec!["-3".to_string()]));
    candidates
}

fn canonical_project_dir(project_dir: &Path) -> PathBuf {
    project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf())
}

fn venv_python_path(venv_dir: &Path) -> Option<String> {
    let windows = venv_dir.join("Scripts").join("python.exe");
    if windows.exists() {
        return Some(windows.to_string_lossy().to_string());
    }

    let unix = venv_dir.join("bin").join("python");
    if unix.exists() {
        return Some(unix.to_string_lossy().to_string());
    }

    None
}

async fn spawn_python(
    project_dir: &Path,
    program: &str,
    args: &[String],
    stream: bool,
) -> Result<String, io::Error> {
    use std::io::Write;
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut command = build_python_command(project_dir, program, args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn()?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut output = String::new();
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        if stream {
            print!("{}", line);
            std::io::stdout().flush().unwrap_or(());
        }
        output.push_str(&line);
        line.clear();
    }

    let status = child.wait().await?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Python failed with exit status: {}", status),
        ));
    }

    Ok(output.trim().to_string())
}

fn build_python_command(project_dir: &Path, program: &str, args: &[String]) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(project_dir)
        .args(args)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    command
}

fn serialize_tools(tools: &[ToolConfig]) -> String {
    tools
        .iter()
        .map(|tool| {
            [tool.kind.as_str(), tool.path.as_str(), tool.symbol.as_str()]
                .join(&TOOL_SEPARATOR.to_string())
        })
        .collect::<Vec<_>>()
        .join(&CAPABILITY_SEPARATOR.to_string())
}

struct PythonCandidate {
    program: String,
    prefix_args: Vec<String>,
}

impl PythonCandidate {
    fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            prefix_args: Vec::new(),
        }
    }

    fn with_prefix(program: impl Into<String>, prefix_args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            prefix_args,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_python_tools_requires_python_project() {
        let manifest: Manifest = toml::from_str(
            r#"
[project]
name = "demo"

[[tool]]
id = "assistant-tools"
kind = "python"
path = "src/tools/assistant.py"
symbol = "project_runtime_info"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
tools = ["assistant-tools"]
"#,
        )
        .unwrap();

        let temp_dir = std::env::temp_dir().join("enki-non-python-project-runtime-test");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let result = validate_python_tools(&manifest, &temp_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("`tools` is only supported for Python projects right now")
        );
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn python_runner_args_include_tools() {
        let manifest: Manifest = toml::from_str(
            r#"
[project]
name = "demo"

[[tool]]
id = "assistant-tools"
kind = "python"
path = "src/tools/assistant.py"
symbol = "project_runtime_info"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
tools = ["assistant-tools"]
"#,
        )
        .unwrap();

        let args = python_runner_args(
            &manifest,
            Path::new("demo"),
            ".\\.enki",
            "assistant",
            "session-1",
            "hello",
        );

        assert_eq!(args[2], "demo");
        assert_eq!(
            args[args.len() - 2],
            "python\u{1e}src/tools/assistant.py\u{1e}project_runtime_info"
        );
    }
}
