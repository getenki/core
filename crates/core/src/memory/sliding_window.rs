use crate::memory::{MemoryEntry, MemoryKind, MemoryProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROVIDER_NAME: &str = "sliding_window";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredMessage {
    role: String,
    content: String,
    timestamp_ns: u128,
}

pub struct SlidingWindowMemory {
    base_dir: PathBuf,
    window_size: usize,
}

impl SlidingWindowMemory {
    pub fn new(base_dir: impl Into<PathBuf>, window_size: usize) -> Self {
        Self {
            base_dir: base_dir.into(),
            window_size: window_size.max(1),
        }
    }

    fn provider_dir(&self) -> PathBuf {
        self.base_dir.join(PROVIDER_NAME)
    }

    fn session_file(&self, session_id: &str) -> PathBuf {
        self.provider_dir()
            .join(format!("{}.jsonl", slugify(session_id)))
    }

    async fn load_messages(&self, path: &Path) -> Result<Vec<StoredMessage>, String> {
        if !fs::try_exists(path).await.map_err(|e| e.to_string())? {
            return Ok(Vec::new());
        }

        let file = File::open(path)
            .await
            .map_err(|e| format!("Failed to open sliding window memory: {e}"))?;
        let mut lines = BufReader::new(file).lines();
        let mut messages = Vec::new();

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("Failed to read sliding window memory: {e}"))?
        {
            if let Ok(message) = serde_json::from_str::<StoredMessage>(&line) {
                messages.push(message);
            }
        }

        Ok(messages)
    }

    async fn save_messages(&self, path: &Path, messages: &[StoredMessage]) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create sliding window memory directory: {e}"))?;
        }

        let mut file = File::create(path)
            .await
            .map_err(|e| format!("Failed to write sliding window memory: {e}"))?;

        for message in messages {
            let line = serde_json::to_string(message)
                .map_err(|e| format!("Failed to serialize sliding window memory: {e}"))?;
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Failed to write sliding window memory: {e}"))?;
            file.write_all(b"\n")
                .await
                .map_err(|e| format!("Failed to write sliding window memory: {e}"))?;
        }

        Ok(())
    }
}

#[async_trait(?Send)]
impl MemoryProvider for SlidingWindowMemory {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String> {
        let path = self.session_file(session_id);
        let mut messages = self.load_messages(&path).await?;
        let timestamp_ns = current_timestamp_nanos();
        messages.push(StoredMessage {
            role: "user".to_string(),
            content: user_msg.to_string(),
            timestamp_ns,
        });
        messages.push(StoredMessage {
            role: "assistant".to_string(),
            content: assistant_msg.to_string(),
            timestamp_ns: timestamp_ns.saturating_add(1),
        });

        let max_messages = self.window_size.saturating_mul(2);
        if messages.len() > max_messages {
            let start = messages.len() - max_messages;
            messages = messages.split_off(start);
        }

        self.save_messages(&path, &messages).await
    }

    async fn recall(
        &self,
        session_id: &str,
        _query: &str,
        max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let path = self.session_file(session_id);
        let mut messages = self.load_messages(&path).await?;
        if messages.is_empty() {
            return Ok(Vec::new());
        }

        let keep = max_entries.min(messages.len());
        let start = messages.len().saturating_sub(keep);
        messages = messages.split_off(start);

        Ok(messages
            .into_iter()
            .enumerate()
            .map(|(index, message)| MemoryEntry {
                key: format!("recent-{index}"),
                content: format!("{}: {}", message.role, message.content),
                kind: MemoryKind::RecentMessage,
                relevance: 0.7,
                timestamp_ns: message.timestamp_ns,
            })
            .collect())
    }

    async fn flush(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
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
