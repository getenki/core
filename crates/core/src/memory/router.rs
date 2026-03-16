use crate::memory::{MemoryRouter, MemoryStrategy};
use async_trait::async_trait;

pub struct DefaultMemoryRouter {
    provider_names: Vec<String>,
}

impl DefaultMemoryRouter {
    pub fn new(provider_names: Vec<String>) -> Self {
        Self { provider_names }
    }

    fn has_provider(&self, provider_name: &str) -> bool {
        self.provider_names.iter().any(|name| name == provider_name)
    }

    fn pick(&self, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .filter(|name| self.has_provider(name))
            .map(|name| (*name).to_string())
            .collect()
    }

    fn is_entity_query(message: &str) -> bool {
        let keywords = [
            "remember",
            "recall",
            "what is my",
            "what's my",
            "who is",
            "who am i",
            "my name",
            "my preference",
            "my favorite",
            "my favourite",
        ];

        keywords.iter().any(|keyword| message.contains(keyword))
    }

    fn is_summary_query(message: &str) -> bool {
        let keywords = [
            "summarize",
            "summary",
            "what have we done",
            "what did we do",
            "recap",
            "give me a summary",
        ];

        keywords.iter().any(|keyword| message.contains(keyword))
    }

    fn is_follow_up(message: &str) -> bool {
        let words: Vec<&str> = message.split_whitespace().collect();
        let short_follow_up = words.len() <= 4;
        let lead = words.first().copied().unwrap_or_default();
        short_follow_up
            && matches!(
                lead,
                "ok" | "okay" | "sure" | "do" | "go" | "continue" | "now" | "yes"
            )
    }
}

#[async_trait(?Send)]
impl MemoryRouter for DefaultMemoryRouter {
    async fn select(&self, user_message: &str) -> MemoryStrategy {
        let normalized = user_message.trim().to_ascii_lowercase();
        let active_providers = if Self::is_summary_query(&normalized) {
            self.pick(&["summary"])
        } else if Self::is_entity_query(&normalized) {
            self.pick(&["structured", "summary"])
        } else if Self::is_follow_up(&normalized) {
            self.pick(&["sliding_window"])
        } else {
            self.pick(&["sliding_window", "structured"])
        };

        MemoryStrategy {
            active_providers,
            max_context_entries: 6,
        }
    }
}
