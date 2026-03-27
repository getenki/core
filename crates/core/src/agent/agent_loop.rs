use async_trait::async_trait;
use serde_json::Value;

use crate::agent::core::Agent;
use crate::agent::types::StepOutcome;
use crate::message::{Message, next_request_id};

#[async_trait(?Send)]
pub trait AgentLoop {
    async fn run(&self, agent: &Agent, session_id: &str, user_message: &str) -> String;
}

pub struct DefaultAgentLoop;

#[async_trait(?Send)]
impl AgentLoop for DefaultAgentLoop {
    async fn run(&self, agent: &Agent, session_id: &str, user_message: &str) -> String {
        let ctx = agent.workspace.tool_context(session_id);
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = tokio::fs::create_dir_all(&ctx.workspace_dir).await {
            return format!("Workspace error: {e}");
        }

        let mut messages = match agent.load_messages(session_id).await {
            Ok(messages) => messages,
            Err(e) => return format!("Session state error: {e}"),
        };

        if messages.is_empty() {
            let memory_context = agent
                .memory
                .build_context(session_id, user_message)
                .await
                .unwrap_or_default();
            messages.push(Message::system(agent.system_prompt(&ctx, &memory_context)));
        }

        let request_id = next_request_id();
        let prev_message_id = messages.last().map(|message| message.message_id.clone());

        messages.push(Message::user(
            user_message.to_string(),
            request_id,
            prev_message_id,
            None,
        ));

        for _ in 0..agent.definition.max_iterations {
            match agent.step(&mut messages, &ctx).await {
                Ok(StepOutcome::Continue) => {
                    agent.persist_state(session_id, &messages).await;
                }
                Ok(StepOutcome::Final(content)) => {
                    let _ = agent
                        .memory
                        .record_all(session_id, user_message, &content)
                        .await;
                    let _ = agent.memory.consolidate_all(session_id).await;
                    agent.persist_state(session_id, &messages).await;
                    return content;
                }
                Err(e) => {
                    let content = format!("LLM error: {e}");
                    Agent::push_out_message(
                        &mut messages,
                        serde_json::json!({
                            "role": "assistant",
                            "content": content,
                        }),
                    );
                    agent.persist_state(session_id, &messages).await;
                    return messages
                        .last()
                        .and_then(|message| {
                            let value = Value::from(message);
                            value
                                .get("content")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| "LLM error".to_string());
                }
            }
        }

        let content = "Max iterations reached.".to_string();
        Agent::push_out_message(
            &mut messages,
            serde_json::json!({
                "role": "assistant",
                "content": content.clone(),
            }),
        );
        let _ = agent
            .memory
            .record_all(session_id, user_message, &content)
            .await;
        let _ = agent.memory.consolidate_all(session_id).await;
        agent.persist_state(session_id, &messages).await;
        content
    }
}
