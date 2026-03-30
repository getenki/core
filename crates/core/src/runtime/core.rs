use crate::runtime::{
    InputChannel, RuntimeDetailedResponse, RuntimeEvent, RuntimeHandler, RuntimeRequest,
    RuntimeResponse, SessionContext,
};
use crate::tooling::types::AskHumanFn;
use async_trait::async_trait;
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

// ---------------------------------------------------------------------------
// ChannelHumanFn — bridges AskHumanTool to the InputChannel via mpsc/oneshot
// ---------------------------------------------------------------------------

/// A query from the agent to the human, sent over the internal mpsc channel.
pub(crate) struct HumanQuery {
    pub query: String,
    pub reply_tx: tokio::sync::oneshot::Sender<String>,
}

/// Implementation of `AskHumanFn` that sends the question over an mpsc channel
/// and awaits the reply on a oneshot.
pub(crate) struct ChannelHumanFn {
    tx: tokio::sync::mpsc::Sender<HumanQuery>,
}

impl ChannelHumanFn {
    pub fn new(tx: tokio::sync::mpsc::Sender<HumanQuery>) -> Self {
        Self { tx }
    }
}

#[async_trait(?Send)]
impl AskHumanFn for ChannelHumanFn {
    async fn ask(&self, query: &str) -> Result<String, String> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(HumanQuery {
                query: query.to_string(),
                reply_tx,
            })
            .await
            .map_err(|_| "Human channel closed.".to_string())?;

        reply_rx
            .await
            .map_err(|_| "Human reply channel dropped.".to_string())
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

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

    /// Serve an `InputChannel`, multiplexing between agent execution and
    /// human-in-the-loop queries.
    ///
    /// When the agent calls `ask_human`, this method:
    /// 1. Emits a `RuntimeEvent::HumanRequest` on the channel.
    /// 2. Reads the next `RuntimeRequest` from the channel as the human reply.
    /// 3. Routes the reply back to the suspended tool via a oneshot channel.
    pub async fn serve_channel<C>(&self, channel: &mut C) -> Result<(), String>
    where
        C: InputChannel,
    {
        while let Some(request) = channel.recv().await {
            let request_id = request.request_id.clone();
            let session_id = request.session_id.clone();
            let channel_id = request.channel_id.clone();

            // Create the human query channel for this request.
            let (human_tx, mut human_rx) = tokio::sync::mpsc::channel::<HumanQuery>(1);

            let human_fn = Arc::new(ChannelHumanFn::new(human_tx));

            // Spawn agent execution as a local task so we can multiplex.
            // We use spawn_local because our traits are !Send.
            let handler = Arc::clone(&self.handler);
            let coordinator = Arc::clone(&self.coordinator);
            let req_clone = request.clone();
            let human_fn_clone: Arc<dyn AskHumanFn> = human_fn.clone();

            // We run the agent in a future that we poll alongside the human_rx.
            let agent_fut = async move {
                let lease = coordinator
                    .acquire(&req_clone.session_id, &req_clone.channel_id)
                    .await?;

                let (content, steps) = handler
                    .handle_detailed_with_human(
                        &req_clone,
                        &lease.context,
                        None,
                        Some(human_fn_clone),
                    )
                    .await?;

                let response = RuntimeResponse {
                    request_id: req_clone.request_id,
                    session_id: lease.context.session_id.clone(),
                    channel_id: lease.context.channel_id.clone(),
                    sequence: lease.context.sequence,
                    content,
                };

                Ok::<RuntimeDetailedResponse, String>(RuntimeDetailedResponse { response, steps })
            };

            // Pin the future so we can select! on it.
            tokio::pin!(agent_fut);

            let traced = loop {
                tokio::select! {
                    // The agent finished (or errored).
                    result = &mut agent_fut => {
                        break result?;
                    }
                    // The agent is asking the human a question.
                    Some(human_query) = human_rx.recv() => {
                        // Emit a HumanRequest event to the channel.
                        channel
                            .send(RuntimeEvent::HumanRequest {
                                request_id: request_id.clone(),
                                session_id: session_id.clone(),
                                channel_id: channel_id.clone(),
                                query: human_query.query,
                            })
                            .await?;

                        // Wait for the human's reply via the next InputChannel message.
                        let reply_request = channel.recv().await.ok_or_else(|| {
                            "Channel closed while waiting for human reply.".to_string()
                        })?;

                        // Send the reply back to the suspended AskHumanTool.
                        let _ = human_query.reply_tx.send(reply_request.content);
                    }
                }
            };

            // Emit execution steps.
            for step in traced.steps.iter().cloned() {
                channel
                    .send(RuntimeEvent::Step {
                        request_id: traced.response.request_id.clone(),
                        session_id: traced.response.session_id.clone(),
                        channel_id: traced.response.channel_id.clone(),
                        sequence: traced.response.sequence,
                        step,
                    })
                    .await?;
            }
            channel.send(RuntimeEvent::Final(traced.response)).await?;
        }

        Ok(())
    }
}
