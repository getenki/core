mod registry;
mod selector;
mod types;

pub use registry::AgentRegistry;
pub use selector::{FirstMatchSelector, PeerSelector};
pub use types::{AgentCard, AgentStatus, DiscoverQuery};

#[cfg(test)]
mod tests;
