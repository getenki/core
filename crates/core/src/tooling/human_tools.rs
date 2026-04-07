use crate::tooling::types::{Tool, ToolContext, parse_tool_args};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// AskHumanTool
// ---------------------------------------------------------------------------

/// An intrinsic tool that pauses agent execution and asks the human user a
/// question via the runtime channel.  The agent loop suspends in the Observe
/// phase until the human replies.
pub struct AskHumanTool;

#[derive(Deserialize)]
struct AskHumanParams {
    query: String,
}

#[async_trait(?Send)]
impl Tool for AskHumanTool {
    fn name(&self) -> &str {
        "ask_human"
    }

    fn description(&self) -> &str {
        "Ask the human user a question or request confirmation. \
         Agent execution will pause until the human responds. \
         Use this when you need clarification, approval, or any input \
         that only a human can provide."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The question or prompt to present to the human user."
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let human = match &ctx.human {
            Some(h) => h,
            None => {
                return "Error: human-in-the-loop is not available. \
                        No human channel is configured for this agent."
                    .to_string();
            }
        };

        let params: AskHumanParams = match parse_tool_args(args) {
            Ok(p) => p,
            Err(e) => return format!("Error: failed to parse arguments: {e}"),
        };

        match human.ask(&params.query).await {
            Ok(reply) => reply,
            Err(e) => format!("Error: failed to get human response: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooling::types::AskHumanFn;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct MockHuman {
        reply: String,
    }

    #[async_trait(?Send)]
    impl AskHumanFn for MockHuman {
        async fn ask(&self, _query: &str) -> Result<String, String> {
            Ok(self.reply.clone())
        }
    }

    struct FailingHuman;

    #[async_trait(?Send)]
    impl AskHumanFn for FailingHuman {
        async fn ask(&self, _query: &str) -> Result<String, String> {
            Err("channel closed".to_string())
        }
    }

    fn ctx_with_human(human: Option<Arc<dyn AskHumanFn>>) -> ToolContext {
        ToolContext {
            agent_dir: PathBuf::from("agent"),
            workspace_dir: PathBuf::from("workspace"),
            sessions_dir: PathBuf::from("sessions"),
            delegation: None,
            human,
            workflow: None,
        }
    }

    #[tokio::test]
    async fn ask_human_without_context_returns_error() {
        let tool = AskHumanTool;
        let result = tool
            .execute(&json!({"query": "hello?"}), &ctx_with_human(None))
            .await;
        assert!(result.contains("not available"));
    }

    #[tokio::test]
    async fn ask_human_returns_reply() {
        let human = Arc::new(MockHuman {
            reply: "yes, go ahead".to_string(),
        });
        let tool = AskHumanTool;
        let result = tool
            .execute(
                &json!({"query": "Should I proceed?"}),
                &ctx_with_human(Some(human)),
            )
            .await;
        assert_eq!(result, "yes, go ahead");
    }

    #[tokio::test]
    async fn ask_human_returns_error_on_failure() {
        let human: Arc<dyn AskHumanFn> = Arc::new(FailingHuman);
        let tool = AskHumanTool;
        let result = tool
            .execute(&json!({"query": "hello?"}), &ctx_with_human(Some(human)))
            .await;
        assert!(result.contains("channel closed"));
    }

    #[tokio::test]
    async fn ask_human_missing_query_returns_error() {
        let human = Arc::new(MockHuman {
            reply: "ok".to_string(),
        });
        let tool = AskHumanTool;
        let result = tool.execute(&json!({}), &ctx_with_human(Some(human))).await;
        assert!(result.starts_with("Error: failed to parse arguments:"));
    }
}
