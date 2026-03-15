use crate::memory::{
    DefaultMemoryRouter, MemoryEntry, MemoryKind, MemoryProvider, MemoryRouter,
    SlidingWindowMemory, StructuredMemory, SummaryMemory,
};
use tokio::sync::Mutex;

pub struct MemoryManager {
    router: Box<dyn MemoryRouter>,
    providers: Mutex<Vec<Box<dyn MemoryProvider>>>,
}

impl MemoryManager {
    pub fn new(router: Box<dyn MemoryRouter>, providers: Vec<Box<dyn MemoryProvider>>) -> Self {
        Self {
            router,
            providers: Mutex::new(providers),
        }
    }

    pub fn with_defaults(memory_dir: impl Into<std::path::PathBuf>) -> Self {
        let memory_dir = memory_dir.into();
        let providers: Vec<Box<dyn MemoryProvider>> = vec![
            Box::new(SlidingWindowMemory::new(&memory_dir, 10)),
            Box::new(SummaryMemory::new(&memory_dir, 4, None)),
            Box::new(StructuredMemory::new(&memory_dir)),
        ];
        let provider_names = providers
            .iter()
            .map(|provider| provider.name().to_string())
            .collect();
        Self::new(
            Box::new(DefaultMemoryRouter::new(provider_names)),
            providers,
        )
    }

    pub async fn build_context(
        &self,
        session_id: &str,
        user_message: &str,
    ) -> Result<String, String> {
        let strategy = self.router.select(user_message);
        if strategy.active_providers.is_empty() || strategy.max_context_entries == 0 {
            return Ok(String::new());
        }

        let providers = self.providers.lock().await;
        let mut entries = Vec::new();
        let provider_budget = strategy
            .max_context_entries
            .max(1)
            .div_ceil(strategy.active_providers.len().max(1));

        for provider_name in &strategy.active_providers {
            let Some(provider) = providers
                .iter()
                .find(|provider| provider.name() == provider_name)
            else {
                continue;
            };
            let recalled = provider
                .recall(session_id, user_message, provider_budget)
                .await?;
            entries.extend(recalled);
        }

        Ok(Self::format_entries(Self::merge_entries(
            entries,
            strategy.max_context_entries,
        )))
    }

    pub async fn record_all(
        &self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<(), String> {
        let mut providers = self.providers.lock().await;
        for provider in providers.iter_mut() {
            provider.record(session_id, user_msg, assistant_msg).await?;
        }
        Ok(())
    }

    pub async fn consolidate_all(&self, session_id: &str) -> Result<(), String> {
        let mut providers = self.providers.lock().await;
        for provider in providers.iter_mut() {
            provider.consolidate(session_id).await?;
        }
        Ok(())
    }

    pub async fn flush_all(&self, session_id: &str) -> Result<(), String> {
        let providers = self.providers.lock().await;
        for provider in providers.iter() {
            provider.flush(session_id).await?;
        }
        Ok(())
    }

    fn merge_entries(mut entries: Vec<MemoryEntry>, max_entries: usize) -> Vec<MemoryEntry> {
        entries.sort_by(|left, right| {
            right
                .relevance
                .partial_cmp(&left.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.timestamp_ns.cmp(&left.timestamp_ns))
        });

        let mut deduped = Vec::new();
        for entry in entries {
            let seen = deduped.iter().any(|existing: &MemoryEntry| {
                existing.key == entry.key && existing.content == entry.content
            });
            if !seen {
                deduped.push(entry);
            }
            if deduped.len() >= max_entries {
                break;
            }
        }

        deduped
    }

    fn format_entries(entries: Vec<MemoryEntry>) -> String {
        if entries.is_empty() {
            return String::new();
        }

        let lines = entries
            .into_iter()
            .map(|entry| format!("- [{}] {}", kind_label(entry.kind), entry.content))
            .collect::<Vec<_>>()
            .join("\n");

        format!("## Memory Context\n{lines}")
    }
}

fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::RecentMessage => "recent",
        MemoryKind::Summary => "summary",
        MemoryKind::Entity => "entity",
        MemoryKind::Preference => "preference",
    }
}
