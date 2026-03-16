use crate::memory::{
    DefaultMemoryRouter, MemoryEntry, MemoryKind, MemoryManager, MemoryProvider, MemoryRouter,
    SlidingWindowMemory, StructuredMemory,
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn router_selects_entity_for_recall_query() {
    let router = DefaultMemoryRouter::new(vec![
        "sliding_window".to_string(),
        "summary".to_string(),
        "structured".to_string(),
    ]);

    let strategy = router.select("what's my name?").await;

    assert_eq!(
        strategy.active_providers,
        vec!["structured".to_string(), "summary".to_string()]
    );
}

#[tokio::test]
async fn router_selects_window_for_followup() {
    let router = DefaultMemoryRouter::new(vec![
        "sliding_window".to_string(),
        "summary".to_string(),
        "structured".to_string(),
    ]);

    let strategy = router.select("ok do it").await;

    assert_eq!(
        strategy.active_providers,
        vec!["sliding_window".to_string()]
    );
}

#[tokio::test]
async fn sliding_window_respects_size() {
    let base_dir = temp_dir("sliding");
    let mut provider = SlidingWindowMemory::new(&base_dir, 5);

    for index in 0..20 {
        provider
            .record(
                "session-a",
                &format!("user {index}"),
                &format!("assistant {index}"),
            )
            .await
            .unwrap();
    }

    let entries = provider.recall("session-a", "anything", 20).await.unwrap();

    assert_eq!(entries.len(), 10);
    assert!(entries.iter().all(|entry| {
        entry.content.contains("user 15")
            || entry.content.contains("assistant 15")
            || entry.content.contains("user 16")
            || entry.content.contains("assistant 16")
            || entry.content.contains("user 17")
            || entry.content.contains("assistant 17")
            || entry.content.contains("user 18")
            || entry.content.contains("assistant 18")
            || entry.content.contains("user 19")
            || entry.content.contains("assistant 19")
    }));
}

#[tokio::test]
async fn structured_keyword_recall() {
    let base_dir = temp_dir("structured");
    let mut provider = StructuredMemory::new(&base_dir);

    provider
        .record("session-a", "My name is Taylor.", "Noted.")
        .await
        .unwrap();

    let entries = provider
        .recall("session-a", "what's my name", 5)
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, MemoryKind::Entity);
    assert_eq!(entries[0].content, "Taylor");
}

#[tokio::test]
async fn manager_routes_and_merges() {
    let provider_a = StubProvider::new(
        "structured",
        vec![MemoryEntry {
            key: "name".to_string(),
            content: "Taylor".to_string(),
            kind: MemoryKind::Entity,
            relevance: 1.0,
            timestamp_ns: 10,
        }],
    );
    let provider_b = StubProvider::new(
        "summary",
        vec![MemoryEntry {
            key: "summary".to_string(),
            content: "Working on memory routing.".to_string(),
            kind: MemoryKind::Summary,
            relevance: 0.8,
            timestamp_ns: 9,
        }],
    );

    let manager = MemoryManager::new(
        Box::new(FixedRouter),
        vec![Box::new(provider_a), Box::new(provider_b)],
    );

    let context = manager
        .build_context("session-a", "what's my name?")
        .await
        .unwrap();

    assert!(context.contains("[entity] Taylor"));
    assert!(context.contains("[summary] Working on memory routing."));
}

#[tokio::test]
async fn record_fans_out_to_all() {
    let first = StubProvider::empty("structured");
    let second = StubProvider::empty("summary");
    let first_calls = first.recorded.clone();
    let second_calls = second.recorded.clone();
    let manager = MemoryManager::new(
        Box::new(FixedRouter),
        vec![Box::new(first), Box::new(second)],
    );

    manager
        .record_all("session-a", "hello", "world")
        .await
        .unwrap();

    assert_eq!(first_calls.lock().unwrap().len(), 1);
    assert_eq!(second_calls.lock().unwrap().len(), 1);
}

struct FixedRouter;

#[async_trait(?Send)]
impl MemoryRouter for FixedRouter {
    async fn select(&self, _user_message: &str) -> crate::memory::MemoryStrategy {
        crate::memory::MemoryStrategy {
            active_providers: vec!["structured".to_string(), "summary".to_string()],
            max_context_entries: 4,
        }
    }
}

struct StubProvider {
    name: String,
    entries: Vec<MemoryEntry>,
    recorded: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl StubProvider {
    fn new(name: &str, entries: Vec<MemoryEntry>) -> Self {
        Self {
            name: name.to_string(),
            entries,
            recorded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn empty(name: &str) -> Self {
        Self::new(name, Vec::new())
    }
}

#[async_trait(?Send)]
impl MemoryProvider for StubProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String> {
        self.recorded.lock().unwrap().push((
            session_id.to_string(),
            user_msg.to_string(),
            assistant_msg.to_string(),
        ));
        Ok(())
    }

    async fn recall(
        &self,
        _session_id: &str,
        _query: &str,
        _max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        Ok(self.entries.clone())
    }

    async fn flush(&self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "core-next-memory-tests-{label}-{}",
        current_timestamp_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn current_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
