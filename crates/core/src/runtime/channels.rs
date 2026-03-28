use crate::runtime::{InputChannel, RuntimeEvent, RuntimeRequest};
use async_trait::async_trait;

pub struct CliChannel {
    pending: Option<RuntimeRequest>,
}

impl CliChannel {
    pub fn from_args(args: Vec<String>) -> Result<Self, String> {
        if args.len() < 3 {
            let program = args.first().map(String::as_str).unwrap_or("core-next");
            return Err(format!("Usage: {program} <session_id> '<message>'"));
        }

        let session_id = args[1].clone();
        let message = args[2..].join(" ");

        Ok(Self {
            pending: Some(RuntimeRequest::new(session_id, "cli", message)),
        })
    }
}

#[async_trait(?Send)]
impl InputChannel for CliChannel {
    async fn recv(&mut self) -> Option<RuntimeRequest> {
        // If we have a pending initial request, return it.
        if let Some(req) = self.pending.take() {
            return Some(req);
        }
        // Otherwise, we're done (single-shot CLI).
        None
    }

    async fn send(&mut self, event: RuntimeEvent) -> Result<(), String> {
        match event {
            RuntimeEvent::Step { step, .. } => {
                println!("{}. [{}] {}: {}", step.index, step.phase, step.kind, step.detail);
            }
            RuntimeEvent::HumanRequest { query, .. } => {
                // Print the question and read from stdin.
                println!("\n🧑 Agent is asking for your input:");
                println!("   {query}");
                print!("> ");

                // Flush stdout so the prompt appears before reading.
                use std::io::Write;
                std::io::stdout().flush().ok();

                let mut reply = String::new();
                std::io::stdin()
                    .read_line(&mut reply)
                    .map_err(|e| format!("Failed to read stdin: {e}"))?;
                let reply = reply.trim().to_string();

                // Queue the reply so the next recv() returns it.
                self.pending = Some(RuntimeRequest::new("human-reply", "cli", reply));
            }
            RuntimeEvent::Final(response) => {
                println!("{}", response.content);
            }
        }
        Ok(())
    }
}
