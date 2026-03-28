use crate::agent::ExecutionStep;
use crate::runtime::{Runtime, RuntimeHandler, RuntimeRequest, SessionContext};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

struct RecordingHandler {
    events: Arc<Mutex<Vec<String>>>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

#[async_trait(?Send)]
impl RuntimeHandler for RecordingHandler {
    async fn handle(
        &self,
        request: &RuntimeRequest,
        session: &SessionContext,
    ) -> Result<String, String> {
        let concurrent = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(concurrent, Ordering::SeqCst);

        {
            let mut events = self.events.lock().await;
            events.push(format!("start:{}:{}", request.content, session.sequence));
        }

        if request.content.contains("slow") {
            sleep(Duration::from_millis(40)).await;
        } else {
            sleep(Duration::from_millis(5)).await;
        }

        {
            let mut events = self.events.lock().await;
            events.push(format!("end:{}:{}", request.content, session.sequence));
        }

        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("ok:{}", request.content))
    }
}

#[tokio::test]
async fn serializes_requests_within_the_same_session() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let handler = RecordingHandler {
        events: Arc::clone(&events),
        active: Arc::new(AtomicUsize::new(0)),
        peak: Arc::new(AtomicUsize::new(0)),
    };

    let runtime = Runtime::new(handler);
    let slow = RuntimeRequest::new("session-a", "cli", "slow");
    let fast = RuntimeRequest::new("session-a", "http", "fast");

    let runtime_a = runtime.clone();
    let runtime_b = runtime.clone();

    let (first, second) = tokio::join!(runtime_a.process(slow), runtime_b.process(fast));
    let first = first.unwrap();
    let second = second.unwrap();

    let recorded = events.lock().await.clone();
    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);
    assert_eq!(
        recorded,
        vec![
            "start:slow:1".to_string(),
            "end:slow:1".to_string(),
            "start:fast:2".to_string(),
            "end:fast:2".to_string(),
        ]
    );
}

#[tokio::test]
async fn allows_parallel_work_across_different_sessions() {
    let handler = RecordingHandler {
        events: Arc::new(Mutex::new(Vec::new())),
        active: Arc::new(AtomicUsize::new(0)),
        peak: Arc::new(AtomicUsize::new(0)),
    };

    let peak = Arc::clone(&handler.peak);
    let runtime = Runtime::new(handler);
    let left = RuntimeRequest::new("session-a", "cli", "slow-left");
    let right = RuntimeRequest::new("session-b", "cli", "slow-right");

    let runtime_a = runtime.clone();
    let runtime_b = runtime.clone();

    let (first, second) = tokio::join!(runtime_a.process(left), runtime_b.process(right));
    first.unwrap();
    second.unwrap();

    assert!(peak.load(Ordering::SeqCst) >= 2);
}

struct DetailedHandler;

#[async_trait(?Send)]
impl RuntimeHandler for DetailedHandler {
    async fn handle(
        &self,
        request: &RuntimeRequest,
        _session: &SessionContext,
    ) -> Result<String, String> {
        Ok(format!("plain:{}", request.content))
    }

    async fn handle_detailed(
        &self,
        request: &RuntimeRequest,
        session: &SessionContext,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
    ) -> Result<(String, Vec<ExecutionStep>), String> {
        let step = ExecutionStep {
            index: 1,
            phase: "Act".to_string(),
            kind: "trace".to_string(),
            detail: format!("{}:{}", request.content, session.sequence),
        };
        if let Some(on_step) = on_step {
            on_step(step.clone());
        }
        Ok((format!("detailed:{}", request.content), vec![step]))
    }
}

#[tokio::test]
async fn process_detailed_routes_trace_through_runtime_handler() {
    let runtime = Runtime::new(DetailedHandler);
    let seen = Arc::new(StdMutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    let on_step = Arc::new(move |step: ExecutionStep| {
        seen_clone.lock().unwrap().push(step.detail);
    });

    let result = runtime
        .process_detailed(
            RuntimeRequest::new("session-a", "cli", "hello"),
            Some(on_step),
        )
        .await
        .unwrap();

    assert_eq!(result.response.content, "detailed:hello");
    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].detail, "hello:1");
    assert_eq!(seen.lock().unwrap().as_slice(), ["hello:1"]);
}
