use crate::tooling::types::ToolContext;
#[cfg(not(target_arch = "wasm32"))]
use std::env;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use tokio::fs;

pub struct AgentWorkspace {
    pub agent_dir: PathBuf,
    pub memory_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub tasks_dir: PathBuf,
}

impl AgentWorkspace {
    pub fn new(agent_name: &str, home_dir: Option<PathBuf>) -> Self {
        let root_dir = home_dir
            .unwrap_or_else(default_home_dir)
            .join(".atomiagent")
            .join("agents");
        let agent_dir = root_dir.join(slugify(agent_name));
        let memory_dir = agent_dir.join("memory");
        let sessions_dir = agent_dir.join("sessions");
        let tasks_dir = agent_dir.join("tasks");

        Self {
            agent_dir,
            memory_dir,
            sessions_dir,
            tasks_dir,
        }
    }

    pub async fn ensure_dirs(&self) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            fs::create_dir_all(&self.memory_dir)
                .await
                .map_err(|e| format!("Failed to create memory workspace: {e}"))?;
            fs::create_dir_all(&self.sessions_dir)
                .await
                .map_err(|e| format!("Failed to create session workspace: {e}"))?;
            fs::create_dir_all(&self.tasks_dir)
                .await
                .map_err(|e| format!("Failed to create task workspace: {e}"))?;
            Ok(())
        }
    }

    pub fn task_dir(&self, session_id: &str) -> PathBuf {
        self.tasks_dir.join(slugify(session_id))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn session_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir
            .join(format!("{}.json", slugify(session_id)))
    }

    pub fn tool_context(&self, session_id: &str) -> ToolContext {
        ToolContext {
            agent_dir: self.agent_dir.clone(),
            workspace_dir: self.task_dir(session_id),
            sessions_dir: self.sessions_dir.clone(),
        }
    }
}

fn default_home_dir() -> PathBuf {
    #[cfg(target_arch = "wasm32")]
    {
        PathBuf::new()
    }

    #[cfg(not(target_arch = "wasm32"))]
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "agent".to_string()
    } else {
        slug
    }
}
