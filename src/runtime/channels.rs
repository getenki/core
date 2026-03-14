use crate::runtime::{InputChannel, RuntimeRequest, RuntimeResponse};
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
        self.pending.take()
    }

    async fn send(&mut self, response: RuntimeResponse) -> Result<(), String> {
        println!("{}", response.content);
        Ok(())
    }
}
