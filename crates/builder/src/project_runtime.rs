use crate::manifest::Manifest;
use std::io;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const CAPABILITY_SEPARATOR: char = '\u{1f}';
const PYTHON_MANIFEST_RUNNER: &str = r#"
import sys

from enki_py import Agent, MultiAgentMember, MultiAgentRuntime


def main() -> None:
    workspace_home = sys.argv[1]
    agent_id = sys.argv[2]
    session_id = sys.argv[3]
    message = sys.argv[4]
    agent_count = int(sys.argv[5])

    members = []
    index = 6
    separator = chr(31)

    for _ in range(agent_count):
        member_id = sys.argv[index]
        name = sys.argv[index + 1]
        model = sys.argv[index + 2]
        instructions = sys.argv[index + 3]
        max_iterations = int(sys.argv[index + 4])
        capabilities = [value for value in sys.argv[index + 5].split(separator) if value]
        index += 6

        agent = Agent(
            model,
            name=name,
            instructions=instructions,
            max_iterations=max_iterations,
            workspace_home=workspace_home,
        )
        members.append(
            MultiAgentMember(
                agent_id=member_id,
                agent=agent,
                capabilities=capabilities,
                description=instructions,
            )
        )

    runtime = MultiAgentRuntime(members)
    result = runtime.process_sync(agent_id, message, session_id=session_id)
    print(result.output)


if __name__ == "__main__":
    main()
"#;

pub fn is_python_project(project_dir: &Path) -> bool {
    project_dir.join("pyproject.toml").exists()
}

pub async fn run_python_agent(
    manifest: &Manifest,
    project_dir: &Path,
    workspace_home: &str,
    agent_id: &str,
    session_id: &str,
    message: &str,
) -> Result<String, String> {
    let args = python_runner_args(manifest, workspace_home, agent_id, session_id, message);

    let mut last_not_found: Option<io::Error> = None;
    let mut output = None;

    for candidate in python_candidates(project_dir) {
        let mut candidate_args = candidate.prefix_args.clone();
        candidate_args.extend(args.iter().cloned());

        match spawn_python(project_dir, &candidate.program, &candidate_args).await {
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
            })
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit status {:?}", output.status.code())
        };
        return Err(format!("Python runtime failed: {details}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn python_runner_args(
    manifest: &Manifest,
    workspace_home: &str,
    agent_id: &str,
    session_id: &str,
    message: &str,
) -> Vec<String> {
    let mut args = vec![
        "-c".to_string(),
        PYTHON_MANIFEST_RUNNER.to_string(),
        workspace_home.to_string(),
        agent_id.to_string(),
        session_id.to_string(),
        message.to_string(),
        manifest.agents.len().to_string(),
    ];

    for agent_cfg in &manifest.agents {
        args.push(agent_cfg.id.clone());
        args.push(agent_cfg.name.clone());
        args.push(agent_cfg.model.clone());
        args.push(agent_cfg.system_prompt.clone());
        args.push(agent_cfg.max_iterations.to_string());
        args.push(agent_cfg.capabilities.join(&CAPABILITY_SEPARATOR.to_string()));
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
                if !candidates.iter().any(|candidate| candidate.program == program) {
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
) -> Result<std::process::Output, io::Error> {
    build_python_command(project_dir, program, args).output().await
}

fn build_python_command(project_dir: &Path, program: &str, args: &[String]) -> Command {
    let mut command = Command::new(program);
    command.current_dir(project_dir).args(args);
    command
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
