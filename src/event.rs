use std::path::PathBuf;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone)]
pub enum Event {
    CliInput(String),
    FolderChange(PathBuf),
    HttpRequest(String),
    Tick,
}

#[derive(Debug)]
pub struct AgentState {
    event_count: u64,
}

impl AgentState {
    fn new() -> Self {
        Self { event_count: 0 }
    }
}

#[async_trait::async_trait]
pub trait EventAdapter {
    async fn run(self, tx: mpsc::Sender<Event>);
}
