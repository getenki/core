pub mod cli_adapter;
pub mod event;

use crate::cli_adapter::CliAdapter;
use crate::event::{Event, EventAdapter};
use tokio::sync::mpsc;

#[derive(Debug)]
struct AgentState {
    event_count: u64,
}

impl AgentState {
    fn new() -> Self {
        Self { event_count: 0 }
    }
}

async fn observe(event: &Event, state: &AgentState) -> String {
    format!("event={event:?}, total_seen={}", state.event_count)
}

async fn think(observation: &str) -> String {
    println!("thinking: {observation}");
    "process".to_string()
}

async fn act(state: &mut AgentState, event: Event, action: &str) {
    match (action, event) {
        ("process", Event::CliInput(text)) => {
            println!("CLI => {text}");
            state.event_count += 1;
        }
        ("process", Event::FolderChange(path)) => {
            println!("Folder changed => {}", path.display());
            state.event_count += 1;
        }
        ("process", Event::HttpRequest(req)) => {
            println!("HTTP => {req}");
            state.event_count += 1;
        }
        ("process", Event::Tick) => {
            println!("Tick => autonomous check");
            state.event_count += 1;
        }
        _ => {
            println!("No-op");
        }
    }
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<Event>(128);

    let cli_tx = tx.clone();
    let folder_tx = tx.clone();
    let http_tx = tx.clone();
    let tick_tx = tx.clone();

    tokio::spawn(async move {
        CliAdapter.run(cli_tx).await;
    });

    // tokio::spawn(async move {
    //     FolderAdapter {
    //         path: PathBuf::from("./watched"),
    //     }
    //     .run(folder_tx)
    //     .await;
    // });
    //
    // tokio::spawn(async move {
    //     HttpAdapter.run(http_tx).await;
    // });
    //
    // tokio::spawn(async move {
    //     TickAdapter {
    //         interval: Duration::from_secs(5),
    //     }
    //     .run(tick_tx)
    //     .await;
    // });

    drop(tx);

    let mut state = AgentState::new();

    while let Some(event) = rx.recv().await {
        let observation = observe(&event, &state).await;
        let action = think(&observation).await;
        act(&mut state, event, &action).await;
    }
}
