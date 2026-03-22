use crate::registry::types::{AgentCard, AgentStatus, DiscoverQuery};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct AgentRegistry {
    agents: RwLock<HashMap<String, AgentCard>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, card: AgentCard) {
        let mut agents = self.agents.write().await;
        agents.insert(card.agent_id.clone(), card);
    }

    pub async fn deregister(&self, agent_id: &str) -> bool {
        let mut agents = self.agents.write().await;
        agents.remove(agent_id).is_some()
    }

    pub async fn update_status(&self, agent_id: &str, status: AgentStatus) -> bool {
        let mut agents = self.agents.write().await;
        if let Some(card) = agents.get_mut(agent_id) {
            card.status = status;
            true
        } else {
            false
        }
    }

    pub async fn get(&self, agent_id: &str) -> Option<AgentCard> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    pub async fn discover(&self, query: &DiscoverQuery) -> Vec<AgentCard> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|card| query.matches(card))
            .cloned()
            .collect()
    }

    pub async fn list_all(&self) -> Vec<AgentCard> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }
}
