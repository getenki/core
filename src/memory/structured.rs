use crate::memory::{MemoryEntry, MemoryKind, MemoryProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

const PROVIDER_NAME: &str = "structured";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StructuredItem {
    key: String,
    content: String,
    kind: MemoryKindSerde,
    timestamp_ns: u128,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum MemoryKindSerde {
    Entity,
    Preference,
}

impl MemoryKindSerde {
    fn as_memory_kind(self) -> MemoryKind {
        match self {
            Self::Entity => MemoryKind::Entity,
            Self::Preference => MemoryKind::Preference,
        }
    }
}

pub struct StructuredMemory {
    base_dir: PathBuf,
}

impl StructuredMemory {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn session_file(&self, session_id: &str) -> PathBuf {
        self.base_dir
            .join(PROVIDER_NAME)
            .join(format!("{}.json", slugify(session_id)))
    }

    async fn load_store(
        &self,
        session_id: &str,
    ) -> Result<HashMap<String, StructuredItem>, String> {
        let path = self.session_file(session_id);
        if !fs::try_exists(&path).await.map_err(|e| e.to_string())? {
            return Ok(HashMap::new());
        }

        let raw = fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read structured memory: {e}"))?;
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse structured memory: {e}"))
    }

    async fn save_store(
        &self,
        session_id: &str,
        store: &HashMap<String, StructuredItem>,
    ) -> Result<(), String> {
        let path = self.session_file(session_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create structured memory directory: {e}"))?;
        }

        let raw = serde_json::to_string_pretty(store)
            .map_err(|e| format!("Failed to serialize structured memory: {e}"))?;
        fs::write(path, raw)
            .await
            .map_err(|e| format!("Failed to write structured memory: {e}"))
    }

    fn extract_facts(&self, user_msg: &str) -> Vec<StructuredItem> {
        let normalized = user_msg.trim();
        let lower = normalized.to_ascii_lowercase();
        let timestamp_ns = current_timestamp_nanos();
        let mut items = Vec::new();

        if let Some(value) = capture_after_prefix(
            normalized,
            &lower,
            &["my name is ", "i am ", "i'm ", "call me "],
        ) {
            items.push(StructuredItem {
                key: "name".to_string(),
                content: value,
                kind: MemoryKindSerde::Entity,
                timestamp_ns,
            });
        }

        if let Some(value) = capture_after_prefix(
            normalized,
            &lower,
            &["i like ", "i prefer ", "my favorite ", "my favourite "],
        ) {
            items.push(StructuredItem {
                key: format!("preference:{}", first_token(&value)),
                content: value,
                kind: MemoryKindSerde::Preference,
                timestamp_ns: timestamp_ns.saturating_add(1),
            });
        }

        items
    }
}

#[async_trait(?Send)]
impl MemoryProvider for StructuredMemory {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        _assistant_msg: &str,
    ) -> Result<(), String> {
        let facts = self.extract_facts(user_msg);
        if facts.is_empty() {
            return Ok(());
        }

        let mut store = self.load_store(session_id).await?;
        for item in facts {
            store.insert(item.key.clone(), item);
        }
        self.save_store(session_id, &store).await
    }

    async fn recall(
        &self,
        session_id: &str,
        query: &str,
        max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let store = self.load_store(session_id).await?;
        let query_terms: Vec<String> = query
            .to_ascii_lowercase()
            .split_whitespace()
            .map(|term| term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
            .filter(|term| !term.is_empty())
            .map(str::to_string)
            .collect();

        let mut entries: Vec<MemoryEntry> = store
            .values()
            .filter_map(|item| {
                let haystack = format!("{} {}", item.key, item.content).to_ascii_lowercase();
                let matches = query_terms
                    .iter()
                    .filter(|term| haystack.contains(term.as_str()))
                    .count();
                if matches == 0 {
                    return None;
                }

                Some(MemoryEntry {
                    key: item.key.clone(),
                    content: item.content.clone(),
                    kind: item.kind.as_memory_kind(),
                    relevance: (matches as f32 / query_terms.len().max(1) as f32).min(1.0),
                    timestamp_ns: item.timestamp_ns,
                })
            })
            .collect();

        entries.sort_by(|left, right| {
            right
                .relevance
                .partial_cmp(&left.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.timestamp_ns.cmp(&left.timestamp_ns))
        });
        entries.truncate(max_entries);
        Ok(entries)
    }

    async fn flush(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }
}

fn capture_after_prefix(source: &str, lower: &str, prefixes: &[&str]) -> Option<String> {
    prefixes.iter().find_map(|prefix| {
        lower.find(prefix).map(|idx| {
            source[idx + prefix.len()..]
                .trim()
                .trim_end_matches(['.', '!', '?'])
                .to_string()
        })
    })
}

fn first_token(value: &str) -> String {
    value
        .split_whitespace()
        .next()
        .unwrap_or("value")
        .to_string()
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
