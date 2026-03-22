use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Online,
    Busy,
    Offline,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Online => write!(f, "online"),
            AgentStatus::Busy => write!(f, "busy"),
            AgentStatus::Offline => write!(f, "offline"),
        }
    }
}

impl AgentStatus {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "online" => Some(Self::Online),
            "busy" => Some(Self::Busy),
            "offline" => Some(Self::Offline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    pub metadata: HashMap<String, String>,
}

impl AgentCard {
    pub fn new(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        capabilities: Vec<String>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            description: description.into(),
            capabilities,
            status: AgentStatus::Online,
            metadata: HashMap::new(),
        }
    }

    pub fn with_status(mut self, status: AgentStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case(capability))
    }
}

#[derive(Debug, Default)]
pub struct DiscoverQuery {
    pub capability: Option<String>,
    pub status: Option<AgentStatus>,
}

impl DiscoverQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capability = Some(capability.into());
        self
    }

    pub fn with_status(mut self, status: AgentStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn matches(&self, card: &AgentCard) -> bool {
        if let Some(ref capability) = self.capability {
            if !card.has_capability(capability) {
                return false;
            }
        }
        if let Some(ref status) = self.status {
            if &card.status != status {
                return false;
            }
        }
        true
    }
}
