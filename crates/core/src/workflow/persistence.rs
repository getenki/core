use crate::workflow::{WorkflowDefinition, WorkflowEvent, WorkflowRunState};
#[cfg(not(target_arch = "wasm32"))]
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct WorkflowWorkspace {
    root_dir: PathBuf,
    runs_dir: PathBuf,
}

impl WorkflowWorkspace {
    pub fn new(home_dir: impl Into<PathBuf>) -> Self {
        let root_dir = home_dir.into().join(".atomiagent").join("workflows");
        let runs_dir = root_dir.join("runs");
        Self { root_dir, runs_dir }
    }

    pub async fn ensure_dirs(&self) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::fs::create_dir_all(&self.runs_dir)
                .await
                .map_err(|e| format!("Failed to create workflow workspace: {e}"))
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn run_dir(&self, run_id: &str) -> PathBuf {
        self.runs_dir.join(run_id)
    }

    pub fn task_workspace(&self, run_id: &str, node_id: &str) -> PathBuf {
        self.run_dir(run_id).join("tasks").join(node_id)
    }

    pub fn state_file(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("state.json")
    }

    pub fn snapshot_file(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("workflow.json")
    }

    pub fn events_file(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("events.jsonl")
    }

    pub fn interventions_file(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("interventions.json")
    }

    pub async fn initialize_run_dir(&self, run_id: &str) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = run_id;
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let run_dir = self.run_dir(run_id);
            tokio::fs::create_dir_all(run_dir.join("tasks"))
                .await
                .map_err(|e| format!("Failed to create workflow run directory: {e}"))
        }
    }

    pub async fn save_definition(
        &self,
        run_id: &str,
        definition: &WorkflowDefinition,
    ) -> Result<(), String> {
        self.write_json(self.snapshot_file(run_id), definition)
            .await
    }

    pub async fn save_state(&self, run_id: &str, state: &WorkflowRunState) -> Result<(), String> {
        self.write_json(self.state_file(run_id), state).await?;
        self.write_json(
            self.interventions_file(run_id),
            &state.pending_interventions,
        )
        .await
    }

    pub async fn load_state(&self, run_id: &str) -> Result<WorkflowRunState, String> {
        self.read_json(self.state_file(run_id)).await
    }

    pub async fn load_definition(&self, run_id: &str) -> Result<WorkflowDefinition, String> {
        self.read_json(self.snapshot_file(run_id)).await
    }

    pub async fn append_event(&self, run_id: &str, event: &WorkflowEvent) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (run_id, event);
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self.events_file(run_id);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Failed to create workflow event directory: {e}"))?;
            }
            let mut line = serde_json::to_string(event)
                .map_err(|e| format!("Failed to serialize workflow event: {e}"))?;
            line.push('\n');
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .map_err(|e| format!("Failed to open workflow event log: {e}"))?;
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Failed to append workflow event: {e}"))
        }
    }

    pub async fn list_run_ids(&self) -> Result<Vec<String>, String> {
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Vec::new())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut runs = Vec::new();
            let mut entries = match tokio::fs::read_dir(&self.runs_dir).await {
                Ok(entries) => entries,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(runs),
                Err(err) => return Err(format!("Failed to read workflow runs: {err}")),
            };

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| format!("Failed to iterate workflow runs: {e}"))?
            {
                if entry
                    .file_type()
                    .await
                    .map_err(|e| format!("Failed to inspect workflow run entry: {e}"))?
                    .is_dir()
                {
                    runs.push(entry.file_name().to_string_lossy().to_string());
                }
            }

            runs.sort();
            Ok(runs)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn write_json<T>(&self, path: PathBuf, value: &T) -> Result<(), String>
    where
        T: Serialize,
    {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create workflow directory: {e}"))?;
        }
        let raw = serde_json::to_string_pretty(value)
            .map_err(|e| format!("Failed to serialize workflow state: {e}"))?;
        tokio::fs::write(path, raw)
            .await
            .map_err(|e| format!("Failed to write workflow state: {e}"))
    }

    #[cfg(target_arch = "wasm32")]
    async fn write_json<T>(&self, _path: PathBuf, _value: &T) -> Result<(), String> {
        Ok(())
    }

    async fn read_json<T>(&self, path: PathBuf) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Err("Workflow persistence is not available on wasm32.".to_string())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let raw = tokio::fs::read_to_string(path)
                .await
                .map_err(|e| format!("Failed to read workflow file: {e}"))?;
            serde_json::from_str(&raw).map_err(|e| format!("Failed to parse workflow file: {e}"))
        }
    }
}
