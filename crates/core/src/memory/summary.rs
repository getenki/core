use crate::llm::{ChatMessage, LlmConfig, LlmProvider, MessageRole};
use crate::memory::{MemoryEntry, MemoryKind, MemoryProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

const PROVIDER_NAME: &str = "summary";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SummaryState {
    summary: String,
    pending: Vec<Exchange>,
    updated_at_ns: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Exchange {
    user: String,
    assistant: String,
    timestamp_ns: u128,
}

pub struct SummaryMemory {
    base_dir: PathBuf,
    pending_threshold: usize,
    llm: Option<Box<dyn LlmProvider>>,
}

impl SummaryMemory {
    pub fn new(
        base_dir: impl Into<PathBuf>,
        pending_threshold: usize,
        llm: Option<Box<dyn LlmProvider>>,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            pending_threshold: pending_threshold.max(1),
            llm,
        }
    }

    fn session_file(&self, session_id: &str) -> PathBuf {
        self.base_dir
            .join(PROVIDER_NAME)
            .join(format!("{}.json", slugify(session_id)))
    }

    async fn load_state(&self, session_id: &str) -> Result<SummaryState, String> {
        let path = self.session_file(session_id);
        if !fs::try_exists(&path).await.map_err(|e| e.to_string())? {
            return Ok(SummaryState::default());
        }

        let raw = fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read summary memory: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse summary memory: {e}"))
    }

    async fn save_state(&self, session_id: &str, state: &SummaryState) -> Result<(), String> {
        let path = self.session_file(session_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create summary memory directory: {e}"))?;
        }

        let raw = serde_json::to_string_pretty(state)
            .map_err(|e| format!("Failed to serialize summary memory: {e}"))?;
        fs::write(path, raw)
            .await
            .map_err(|e| format!("Failed to write summary memory: {e}"))
    }

    async fn generate_summary(
        &self,
        previous_summary: &str,
        pending: &[Exchange],
    ) -> Result<String, String> {
        if let Some(llm) = &self.llm {
            let pending_text = pending
                .iter()
                .map(|exchange| {
                    format!("User: {}\nAssistant: {}", exchange.user, exchange.assistant)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            let prompt = format!(
                "Update the running summary with the new exchanges.\nCurrent summary:\n{}\n\nNew exchanges:\n{}\n\nReturn a concise summary only.",
                previous_summary, pending_text
            );
            let response = llm
                .complete(
                    &[ChatMessage {
                        role: MessageRole::User,
                        content: prompt,
                        tool_call_id: None,
                    }],
                    &LlmConfig::default(),
                )
                .await
                .map_err(|e| e.to_string())?;
            return Ok(response.content.trim().to_string());
        }

        let mut fragments = Vec::new();
        if !previous_summary.trim().is_empty() {
            fragments.push(previous_summary.trim().to_string());
        }
        for exchange in pending {
            fragments.push(format!(
                "User asked: {} Assistant replied: {}",
                exchange.user, exchange.assistant
            ));
        }

        let summary = fragments.join(" ");
        let trimmed = summary.trim();
        if trimmed.len() <= 600 {
            Ok(trimmed.to_string())
        } else {
            Ok(trimmed[..600].to_string())
        }
    }
}

#[async_trait(?Send)]
impl MemoryProvider for SummaryMemory {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String> {
        let mut state = self.load_state(session_id).await?;
        state.pending.push(Exchange {
            user: user_msg.to_string(),
            assistant: assistant_msg.to_string(),
            timestamp_ns: current_timestamp_nanos(),
        });
        state.updated_at_ns = current_timestamp_nanos();
        self.save_state(session_id, &state).await
    }

    async fn recall(
        &self,
        session_id: &str,
        _query: &str,
        max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        if max_entries == 0 {
            return Ok(Vec::new());
        }

        let state = self.load_state(session_id).await?;
        if state.summary.trim().is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![MemoryEntry {
            key: "summary".to_string(),
            content: state.summary,
            kind: MemoryKind::Summary,
            relevance: 0.9,
            timestamp_ns: state.updated_at_ns,
        }])
    }

    async fn flush(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn consolidate(&mut self, session_id: &str) -> Result<(), String> {
        let mut state = self.load_state(session_id).await?;
        if state.pending.len() < self.pending_threshold {
            return Ok(());
        }

        state.summary = self
            .generate_summary(&state.summary, &state.pending)
            .await?;
        state.pending.clear();
        state.updated_at_ns = current_timestamp_nanos();
        self.save_state(session_id, &state).await
    }
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn current_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
