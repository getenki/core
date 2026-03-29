use crate::runtime::{InputChannel, RuntimeEvent, RuntimeRequest};
use async_trait::async_trait;

// ---------------------------------------------------------------------------
// CliChannel — single-shot, for `enki run "do something"`
// ---------------------------------------------------------------------------

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
        if let Some(req) = self.pending.take() {
            return Some(req);
        }
        // Single-shot: done after first request.
        None
    }

    async fn send(&mut self, event: RuntimeEvent) -> Result<(), String> {
        send_cli_event(event, &mut self.pending)
    }
}

// ---------------------------------------------------------------------------
// InteractiveChannel — REPL mode, for `enki chat`
// ---------------------------------------------------------------------------

/// A channel that keeps the agent session alive by reading from stdin in a
/// loop.  After each agent response, `recv()` blocks waiting for the next
/// human message instead of returning `None`.
///
/// The session stays open until the user types "exit", "quit", or sends EOF
/// (Ctrl-D / Ctrl-Z).
pub struct InteractiveChannel {
    session_id: String,
    /// Holds one queued request (initial message or human-reply).
    pending: Option<RuntimeRequest>,
    /// Set to `true` to stop the loop (user typed "exit" / EOF).
    done: bool,
}

impl InteractiveChannel {
    /// Create a new interactive channel.  If `initial_message` is `Some`, the
    /// agent starts working on it immediately; otherwise it prints a prompt and
    /// waits.
    pub fn new(session_id: impl Into<String>, initial_message: Option<String>) -> Self {
        let session_id = session_id.into();
        let pending = initial_message.map(|msg| {
            RuntimeRequest::new(session_id.clone(), "cli", msg)
        });
        Self {
            session_id,
            pending,
            done: false,
        }
    }
}

#[async_trait(?Send)]
impl InputChannel for InteractiveChannel {
    async fn recv(&mut self) -> Option<RuntimeRequest> {
        // If we already have a queued request (initial or human-reply), use it.
        if let Some(req) = self.pending.take() {
            return Some(req);
        }

        if self.done {
            return None;
        }

        // Otherwise, block on stdin for the next user message.
        loop {
            use std::io::Write;
            print!("\n📝 You: ");
            std::io::stdout().flush().ok();

            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(0) => {
                    // EOF (Ctrl-D / Ctrl-Z)
                    println!("\n👋 Goodbye!");
                    self.done = true;
                    return None;
                }
                Ok(_) => {
                    let trimmed = input.trim();
                    if trimmed.is_empty() {
                        continue; // skip blank lines
                    }
                    if matches!(trimmed, "exit" | "quit" | "/exit" | "/quit") {
                        println!("👋 Goodbye!");
                        self.done = true;
                        return None;
                    }
                    return Some(RuntimeRequest::new(
                        self.session_id.clone(),
                        "cli",
                        trimmed.to_string(),
                    ));
                }
                Err(e) => {
                    eprintln!("Error reading stdin: {e}");
                    self.done = true;
                    return None;
                }
            }
        }
    }

    async fn send(&mut self, event: RuntimeEvent) -> Result<(), String> {
        send_cli_event(event, &mut self.pending)
    }
}

// ---------------------------------------------------------------------------
// Shared event handling for both channels
// ---------------------------------------------------------------------------

fn send_cli_event(
    event: RuntimeEvent,
    pending: &mut Option<RuntimeRequest>,
) -> Result<(), String> {
    match event {
        RuntimeEvent::Step { step, .. } => {
            println!(
                "{}. [{}] {}: {}",
                step.index, step.phase, step.kind, step.detail
            );
        }
        RuntimeEvent::HumanRequest { query, .. } => {
            println!("\n🧑 Agent is asking for your input:");
            println!("   {query}");
            print!("> ");

            use std::io::Write;
            std::io::stdout().flush().ok();

            let mut reply = String::new();
            std::io::stdin()
                .read_line(&mut reply)
                .map_err(|e| format!("Failed to read stdin: {e}"))?;
            let reply = reply.trim().to_string();

            *pending = Some(RuntimeRequest::new("human-reply", "cli", reply));
        }
        RuntimeEvent::Final(response) => {
            println!("\n🤖 Agent: {}", response.content);
        }
    }
    Ok(())
}
