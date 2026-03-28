use crate::runtime::{
    InputChannel, RuntimeDetailedResponse, RuntimeHandler, RuntimeRequest, RuntimeResponse,
    SessionContext,
};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

#[derive(Default)]
struct SessionCoordinator {
    sessions: Mutex<HashMap<String, Arc<SessionState>>>,
}

struct SessionState {
    gate: Arc<Semaphore>,
    next_sequence: AtomicU64,
}

impl SessionState {
    fn new() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(1)),
            next_sequence: AtomicU64::new(1),
        }
    }
}

struct SessionLease {
    context: SessionContext,
    _permit: OwnedSemaphorePermit,
}

impl SessionCoordinator {
    async fn acquire(&self, session_id: &str, channel_id: &str) -> Result<SessionLease, String> {
        let state = {
            let mut sessions = self.sessions.lock().await;
            sessions
                .entry(session_id.to_string())
                .or_insert_with(|| Arc::new(SessionState::new()))
                .clone()
        };

        let sequence = state.next_sequence.fetch_add(1, Ordering::SeqCst);
        let permit = state
            .gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| format!("Session coordinator closed for `{session_id}`"))?;

        Ok(SessionLease {
            context: SessionContext {
                session_id: session_id.to_string(),
                channel_id: channel_id.to_string(),
                sequence,
            },
            _permit: permit,
        })
    }
}

pub struct Runtime<H> {
    handler: Arc<H>,
    coordinator: Arc<SessionCoordinator>,
}

impl<H> Clone for Runtime<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            coordinator: Arc::clone(&self.coordinator),
        }
    }
}

impl<H> Runtime<H>
where
    H: RuntimeHandler,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            coordinator: Arc::new(SessionCoordinator::default()),
        }
    }

    pub async fn process(&self, request: RuntimeRequest) -> Result<RuntimeResponse, String> {
        Ok(self.process_detailed(request, None).await?.response)
    }

    pub async fn process_detailed(
        &self,
        request: RuntimeRequest,
        on_step: Option<std::sync::Arc<dyn Fn(crate::agent::ExecutionStep) + Send + Sync>>,
    ) -> Result<RuntimeDetailedResponse, String> {
        let lease = self
            .coordinator
            .acquire(&request.session_id, &request.channel_id)
            .await?;
        let (content, steps) = self
            .handler
            .handle_detailed(&request, &lease.context, on_step)
            .await?;

        let response = RuntimeResponse {
            request_id: request.request_id,
            session_id: lease.context.session_id.clone(),
            channel_id: lease.context.channel_id.clone(),
            sequence: lease.context.sequence,
            content,
        };

        Ok(RuntimeDetailedResponse { response, steps })
    }

    pub async fn serve_channel<C>(&self, channel: &mut C) -> Result<(), String>
    where
        C: InputChannel,
    {
        while let Some(request) = channel.recv().await {
            let response = self.process(request).await?;
            channel.send(response).await?;
        }

        Ok(())
    }
}
