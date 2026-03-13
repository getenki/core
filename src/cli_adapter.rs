use crate::event::{Event, EventAdapter};
use tokio::io;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
pub struct CliAdapter;

#[async_trait::async_trait]
impl EventAdapter for CliAdapter {
    async fn run(self, tx: mpsc::Sender<Event>) {
        let reader = BufReader::new(io::stdin());
        let mut lines = reader.lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if tx.send(Event::CliInput(line)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    eprintln!("cli adapter error: {err}");
                    break;
                }
            }
        }
    }
}
