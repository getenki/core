use crate::registry::types::{AgentCard, AgentStatus};
use async_trait::async_trait;

/// Strategy for selecting a peer agent from a set of candidates.
///
/// Implementations can use round-robin, capability scoring, latency-based
/// selection, or even LLM-driven routing.
#[async_trait(?Send)]
pub trait PeerSelector {
    async fn select(&self, candidates: &[AgentCard], task_description: &str) -> Option<String>;
}

/// Selects the first online agent whose capabilities match (or the first
/// online agent if no capability filtering was applied upstream).
pub struct FirstMatchSelector;

#[async_trait(?Send)]
impl PeerSelector for FirstMatchSelector {
    async fn select(&self, candidates: &[AgentCard], _task_description: &str) -> Option<String> {
        candidates
            .iter()
            .find(|card| card.status == AgentStatus::Online)
            .map(|card| card.agent_id.clone())
    }
}
