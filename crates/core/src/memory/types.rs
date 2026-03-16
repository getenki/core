use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEntry {
    pub key: String,
    pub content: String,
    pub kind: MemoryKind,
    pub relevance: f32,
    pub timestamp_ns: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    RecentMessage,
    Summary,
    Entity,
    Preference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryStrategy {
    pub active_providers: Vec<String>,
    pub max_context_entries: usize,
}

#[async_trait(?Send)]
pub trait MemoryProvider {
    fn name(&self) -> &str;

    async fn record(
        &mut self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String>;

    async fn recall(
        &self,
        session_id: &str,
        query: &str,
        max_entries: usize,
    ) -> Result<Vec<MemoryEntry>, String>;

    async fn flush(&self, session_id: &str) -> Result<(), String>;

    async fn consolidate(&mut self, _session_id: &str) -> Result<(), String> {
        Ok(())
    }
}

#[async_trait(?Send)]
pub trait MemoryRouter {
    async fn select(&self, user_message: &str) -> MemoryStrategy;
}
