use crate::registry::AgentStatus;
use crate::registry::DiscoverQuery;
use crate::tooling::types::{Tool, ToolContext, parse_tool_args};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// DiscoverAgentsTool
// ---------------------------------------------------------------------------

pub struct DiscoverAgentsTool;

#[derive(Deserialize)]
struct DiscoverParams {
    capability: Option<String>,
    status: Option<String>,
}

#[async_trait(?Send)]
impl Tool for DiscoverAgentsTool {
    fn name(&self) -> &str {
        "discover_agents"
    }

    fn description(&self) -> &str {
        "Discover peer agents registered in the runtime. \
         Returns a JSON array of agent cards matching the query."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "capability": {
                    "type": "string",
                    "description": "Optional capability to filter by, e.g. 'code-gen' or 'research'."
                },
                "status": {
                    "type": "string",
                    "description": "Optional status filter: 'online', 'busy', or 'offline'."
                }
            }
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let delegation = match &ctx.delegation {
            Some(d) => d,
            None => return "Error: agent is not part of a multi-agent runtime.".to_string(),
        };

        let params: DiscoverParams = match parse_tool_args(args) {
            Ok(p) => p,
            Err(e) => return format!("Error: failed to parse arguments: {e}"),
        };

        let mut query = DiscoverQuery::new();
        if let Some(cap) = params.capability {
            query = query.with_capability(cap);
        }
        if let Some(status_str) = params.status {
            if let Some(status) = AgentStatus::from_str_loose(&status_str) {
                query = query.with_status(status);
            } else {
                return format!(
                    "Error: unknown status '{status_str}'. Use 'online', 'busy', or 'offline'."
                );
            }
        }

        let cards = delegation.registry.discover(&query).await;

        // Exclude self from results
        let cards: Vec<_> = cards
            .into_iter()
            .filter(|c| c.agent_id != delegation.self_agent_id)
            .collect();

        match serde_json::to_string_pretty(&cards) {
            Ok(json) => json,
            Err(e) => format!("Error: failed to serialize results: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// DelegateTaskTool
// ---------------------------------------------------------------------------

pub struct DelegateTaskTool;

#[derive(Deserialize)]
struct DelegateParams {
    agent_id: String,
    task: String,
}

#[async_trait(?Send)]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "Delegate a task to another agent by its agent_id. \
         Returns the peer agent's response."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "ID of the target agent to delegate the task to."
                },
                "task": {
                    "type": "string",
                    "description": "The task description / prompt to send to the peer agent."
                }
            },
            "required": ["agent_id", "task"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let delegation = match &ctx.delegation {
            Some(d) => d,
            None => return "Error: agent is not part of a multi-agent runtime.".to_string(),
        };

        let params: DelegateParams = match parse_tool_args(args) {
            Ok(p) => p,
            Err(e) => return format!("Error: failed to parse arguments: {e}"),
        };

        if params.agent_id == delegation.self_agent_id {
            return "Error: cannot delegate a task to yourself.".to_string();
        }

        // Check that the target agent exists in the registry
        let target_exists = delegation.registry.get(&params.agent_id).await.is_some();
        if !target_exists {
            return format!("Error: agent '{}' not found in registry.", params.agent_id);
        }

        match delegation.delegate(&params.agent_id, &params.task).await {
            Ok(response) => response,
            Err(e) => format!("Error: delegation failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{AgentCard, AgentRegistry};
    use crate::tooling::types::{DelegateFn, DelegationContext, ToolContext};
    use std::path::PathBuf;
    use std::sync::Arc;

    struct MockDelegateFn;

    #[async_trait(?Send)]
    impl DelegateFn for MockDelegateFn {
        async fn delegate(&self, target_agent_id: &str, task: &str) -> Result<String, String> {
            Ok(format!("delegated:{target_agent_id}:{task}"))
        }
    }

    fn test_ctx(delegation: Option<DelegationContext>) -> ToolContext {
        ToolContext {
            agent_dir: PathBuf::from("agent"),
            workspace_dir: PathBuf::from("workspace"),
            sessions_dir: PathBuf::from("sessions"),
            delegation,
            human: None,
            workflow: None,
        }
    }

    async fn populated_delegation_ctx() -> DelegationContext {
        let registry = Arc::new(AgentRegistry::new());
        registry
            .register(AgentCard::new(
                "self-agent",
                "Self",
                "Me",
                vec!["code-gen".into()],
            ))
            .await;
        registry
            .register(AgentCard::new(
                "peer-a",
                "Peer A",
                "Research agent",
                vec!["research".into()],
            ))
            .await;
        registry
            .register(
                AgentCard::new("peer-b", "Peer B", "Offline", vec!["code-gen".into()])
                    .with_status(AgentStatus::Offline),
            )
            .await;

        DelegationContext::new(registry, "self-agent", Arc::new(MockDelegateFn))
    }

    #[tokio::test]
    async fn discover_without_delegation_returns_error() {
        let tool = DiscoverAgentsTool;
        let result = tool.execute(&json!({}), &test_ctx(None)).await;
        assert!(result.contains("not part of a multi-agent runtime"));
    }

    #[tokio::test]
    async fn discover_returns_peers_excluding_self() {
        let delegation = populated_delegation_ctx().await;
        let tool = DiscoverAgentsTool;
        let result = tool.execute(&json!({}), &test_ctx(Some(delegation))).await;
        assert!(result.contains("peer-a"));
        assert!(result.contains("peer-b"));
        assert!(!result.contains("self-agent"));
    }

    #[tokio::test]
    async fn discover_filters_by_capability() {
        let delegation = populated_delegation_ctx().await;
        let tool = DiscoverAgentsTool;
        let result = tool
            .execute(
                &json!({"capability": "research"}),
                &test_ctx(Some(delegation)),
            )
            .await;
        assert!(result.contains("peer-a"));
        assert!(!result.contains("peer-b"));
    }

    #[tokio::test]
    async fn discover_filters_by_status() {
        let delegation = populated_delegation_ctx().await;
        let tool = DiscoverAgentsTool;
        let result = tool
            .execute(&json!({"status": "offline"}), &test_ctx(Some(delegation)))
            .await;
        assert!(result.contains("peer-b"));
        assert!(!result.contains("peer-a"));
    }

    #[tokio::test]
    async fn delegate_without_delegation_returns_error() {
        let tool = DelegateTaskTool;
        let result = tool
            .execute(&json!({"agent_id": "x", "task": "y"}), &test_ctx(None))
            .await;
        assert!(result.contains("not part of a multi-agent runtime"));
    }

    #[tokio::test]
    async fn delegate_to_self_returns_error() {
        let delegation = populated_delegation_ctx().await;
        let tool = DelegateTaskTool;
        let result = tool
            .execute(
                &json!({"agent_id": "self-agent", "task": "do stuff"}),
                &test_ctx(Some(delegation)),
            )
            .await;
        assert!(result.contains("cannot delegate a task to yourself"));
    }

    #[tokio::test]
    async fn delegate_to_unknown_agent_returns_error() {
        let delegation = populated_delegation_ctx().await;
        let tool = DelegateTaskTool;
        let result = tool
            .execute(
                &json!({"agent_id": "ghost", "task": "do stuff"}),
                &test_ctx(Some(delegation)),
            )
            .await;
        assert!(result.contains("not found in registry"));
    }

    #[tokio::test]
    async fn delegate_invokes_delegate_fn_and_returns_result() {
        let delegation = populated_delegation_ctx().await;
        let tool = DelegateTaskTool;
        let result = tool
            .execute(
                &json!({"agent_id": "peer-a", "task": "summarize docs"}),
                &test_ctx(Some(delegation)),
            )
            .await;
        assert_eq!(result, "delegated:peer-a:summarize docs");
    }
}
