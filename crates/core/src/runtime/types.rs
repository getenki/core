use async_trait::async_trait;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent::ExecutionStep;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequest {
    pub request_id: String,
    pub session_id: String,
    pub channel_id: String,
    pub user_id: Option<String>,
    pub content: String,
}

impl RuntimeRequest {
    pub fn new(
        session_id: impl Into<String>,
        channel_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            request_id: format!("rt-req-{}", current_timestamp_nanos()),
            session_id: session_id.into(),
            channel_id: channel_id.into(),
            user_id: None,
            content: content.into(),
        }
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionContext {
    pub session_id: String,
    pub channel_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResponse {
    pub request_id: String,
    pub session_id: String,
    pub channel_id: String,
    pub sequence: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDetailedResponse {
    pub response: RuntimeResponse,
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    Step {
        request_id: String,
        session_id: String,
        channel_id: String,
        sequence: u64,
        step: ExecutionStep,
    },
    /// The agent is paused and waiting for a human reply.
    HumanRequest {
        request_id: String,
        session_id: String,
        channel_id: String,
        query: String,
    },
    Final(RuntimeResponse),
}

#[async_trait(?Send)]
pub trait RuntimeHandler {
    async fn handle(
        &self,
        request: &RuntimeRequest,
        session: &SessionContext,
    ) -> Result<String, String>;

    async fn handle_detailed(
        &self,
        request: &RuntimeRequest,
        session: &SessionContext,
        _on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
    ) -> Result<(String, Vec<ExecutionStep>), String> {
        Ok((self.handle(request, session).await?, Vec::new()))
    }

    /// Like `handle_detailed`, but also accepts an optional `AskHumanFn` to
    /// inject into the agent's tool context for human-in-the-loop support.
    async fn handle_detailed_with_human(
        &self,
        request: &RuntimeRequest,
        session: &SessionContext,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
        _human: Option<std::sync::Arc<dyn crate::tooling::types::AskHumanFn>>,
    ) -> Result<(String, Vec<ExecutionStep>), String> {
        // Default: ignore the human context and delegate to handle_detailed.
        self.handle_detailed(request, session, on_step).await
    }
}

#[async_trait(?Send)]
pub trait InputChannel {
    async fn recv(&mut self) -> Option<RuntimeRequest>;
    async fn send(&mut self, event: RuntimeEvent) -> Result<(), String>;
}

pub(crate) fn current_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
