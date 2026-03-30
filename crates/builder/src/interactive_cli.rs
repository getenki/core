use crate::manifest::AgentConfig;
use std::io::{self, BufRead, Write};

pub struct InteractiveCliClient {
    default_agent_id: String,
    agent_ids: Vec<String>,
    session_counter: u64,
}

pub enum ClientEvent {
    Quit,
    ShowAgents,
    ShowHelp,
    SendMessage { agent_id: String, message: String },
}

impl InteractiveCliClient {
    pub fn new(default_agent_id: impl Into<String>, agents: &[AgentConfig]) -> Self {
        Self {
            default_agent_id: default_agent_id.into(),
            agent_ids: agents.iter().map(|agent| agent.id.clone()).collect(),
            session_counter: 0,
        }
    }

    pub fn print_welcome(&self, project_name: &str) {
        println!();
        println!("\x1b[1;36mEnki Interactive Session\x1b[0m");
        println!("  Project: {}", project_name);
        println!("  Default agent: {}", self.default_agent_id);
        println!("  Commands: /help, /agents, /use <agent-id>, quit");
        println!("  Send to a specific agent with @agent-id <message>");
        println!();
    }

    pub fn print_help(&self) {
        println!();
        println!("  \x1b[1;36mCommands\x1b[0m");
        println!("  /help            Show this help");
        println!("  /agents          List available agents");
        println!("  /use <agent-id>  Change the default target agent");
        println!("  @agent-id <msg>  Send a message to a specific agent");
        println!("  quit             Exit the session");
        println!();
    }

    pub fn print_agents(&self, agents: &[AgentConfig]) {
        println!();
        for agent_cfg in agents {
            let marker = if agent_cfg.id == self.default_agent_id {
                " \x1b[33m(default)\x1b[0m"
            } else {
                ""
            };
            println!(
                "  \x1b[36m*\x1b[0m {} ({}) [{}]{}",
                agent_cfg.id,
                agent_cfg.name,
                agent_cfg.capabilities.join(", "),
                marker
            );
        }
        println!();
    }

    pub fn next_event<R: BufRead>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<ClientEvent>, String> {
        print!("\x1b[1;33m>\x1b[0m ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(err) => return Err(err.to_string()),
        }

        let input = line.trim();
        if input.is_empty() {
            return Ok(Some(ClientEvent::ShowHelp));
        }

        if matches!(input.to_ascii_lowercase().as_str(), "quit" | "exit" | "q") {
            return Ok(Some(ClientEvent::Quit));
        }

        if input == "/agents" {
            return Ok(Some(ClientEvent::ShowAgents));
        }

        if input == "/help" {
            return Ok(Some(ClientEvent::ShowHelp));
        }

        if let Some(agent_id) = input.strip_prefix("/use ") {
            let agent_id = agent_id.trim();
            if agent_id.is_empty() {
                return Err("usage: /use <agent-id>".to_string());
            }
            self.ensure_agent_exists(agent_id)?;
            self.default_agent_id = agent_id.to_string();
            println!(
                "\n  \x1b[2mDefault agent set to {}\x1b[0m\n",
                self.default_agent_id
            );
            return Ok(Some(ClientEvent::ShowAgents));
        }

        let (agent_id, message) = self.parse_message_target(input)?;
        Ok(Some(ClientEvent::SendMessage { agent_id, message }))
    }

    pub fn next_session_id(&mut self, agent_id: &str) -> String {
        self.session_counter += 1;
        format!("join-{}-{}", agent_id, self.session_counter)
    }

    fn parse_message_target(&self, input: &str) -> Result<(String, String), String> {
        if input.starts_with('@') {
            let Some(space_idx) = input.find(' ') else {
                return Err("usage: @agent-id <message>".to_string());
            };
            let agent_id = &input[1..space_idx];
            let message = input[space_idx + 1..].trim();
            self.ensure_agent_exists(agent_id)?;
            if message.is_empty() {
                return Err("message cannot be empty".to_string());
            }
            return Ok((agent_id.to_string(), message.to_string()));
        }

        Ok((self.default_agent_id.clone(), input.to_string()))
    }

    fn ensure_agent_exists(&self, agent_id: &str) -> Result<(), String> {
        if self.agent_ids.iter().any(|id| id == agent_id) {
            Ok(())
        } else {
            Err(format!(
                "unknown agent '{}'. Type /agents to see available agents.",
                agent_id
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents() -> Vec<AgentConfig> {
        vec![
            AgentConfig {
                id: "assistant".into(),
                name: "Assistant".into(),
                model: "ollama::qwen3.5".into(),
                system_prompt: String::new(),
                max_iterations: 20,
                capabilities: vec!["chat".into()],
                tools: Vec::new(),
                script: None,
            },
            AgentConfig {
                id: "reviewer".into(),
                name: "Reviewer".into(),
                model: "ollama::qwen3.5".into(),
                system_prompt: String::new(),
                max_iterations: 20,
                capabilities: vec!["review".into()],
                tools: Vec::new(),
                script: None,
            },
        ]
    }

    #[test]
    fn parses_direct_message_for_default_agent() {
        let client = InteractiveCliClient::new("assistant", &agents());
        let event = client.parse_message_target("hello").unwrap();
        assert_eq!(event.0, "assistant");
        assert_eq!(event.1, "hello");
    }

    #[test]
    fn parses_explicit_agent_message() {
        let client = InteractiveCliClient::new("assistant", &agents());
        let event = client
            .parse_message_target("@reviewer review this")
            .unwrap();
        assert_eq!(event.0, "reviewer");
        assert_eq!(event.1, "review this");
    }

    #[test]
    fn rejects_unknown_agent() {
        let client = InteractiveCliClient::new("assistant", &agents());
        let error = client.parse_message_target("@missing hi").unwrap_err();
        assert!(error.contains("unknown agent"));
    }

    #[test]
    fn increments_session_ids() {
        let mut client = InteractiveCliClient::new("assistant", &agents());
        assert_eq!(client.next_session_id("assistant"), "join-assistant-1");
        assert_eq!(client.next_session_id("assistant"), "join-assistant-2");
    }
}
