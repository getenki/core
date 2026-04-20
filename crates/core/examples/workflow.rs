//! Workflow runtime example.
//!
//! This example uses a mock task runner so it can be run without configuring an
//! LLM provider:
//!
//! ```powershell
//! cargo run -p core --example workflow
//! ```

use async_trait::async_trait;
use enki_next::tooling::types::WorkflowToolContext;
use enki_next::workflow::{
    TaskDefinition, TaskTarget, WorkflowDefinition, WorkflowEdgeDefinition, WorkflowEdgeTransition,
    WorkflowFailurePolicy, WorkflowNodeDefinition, WorkflowNodeKind, WorkflowRequest,
    WorkflowRuntime, WorkflowTaskResult, WorkflowTaskRunner,
};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

struct DemoTaskRunner;

#[async_trait(?Send)]
impl WorkflowTaskRunner for DemoTaskRunner {
    async fn run_task(
        &self,
        target: &TaskTarget,
        metadata: &WorkflowToolContext,
        workspace_dir: &Path,
        prompt: &str,
    ) -> Result<WorkflowTaskResult, String> {
        let agent_id = match target {
            TaskTarget::AgentId(agent_id) => agent_id.clone(),
            TaskTarget::Capabilities(capabilities) => {
                format!("agent-with-{}", capabilities.join("-"))
            }
        };

        Ok(WorkflowTaskResult {
            content: format!(
                "[{agent_id}] handled node '{}' in {}\n\n{}",
                metadata.node_id,
                workspace_dir.display(),
                prompt
            ),
            value: json!({
                "agent_id": agent_id,
                "node_id": metadata.node_id,
                "content": prompt,
                "workspace_dir": workspace_dir.display().to_string(),
            }),
            agent_id,
            steps: Vec::new(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let reusable_task = TaskDefinition {
        id: "draft_release_note".to_string(),
        target: TaskTarget::Capabilities(vec!["writing".to_string()]),
        prompt: "Draft a concise release note for {{input.topic}}.".to_string(),
        input_bindings: Default::default(),
        input_transform: None,
        output_transform: None,
        output_key: Some("draft".to_string()),
        retry_policy: None,
        failure_policy: None,
    };

    let workflow = WorkflowDefinition {
        id: "release-note-review".to_string(),
        name: "Release Note Review".to_string(),
        retry_policy: None,
        failure_policy: Some(WorkflowFailurePolicy::ContinueBestEffort),
        nodes: vec![
            WorkflowNodeDefinition {
                id: "draft".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: Some("draft_release_note".to_string()),
                    task: None,
                },
                output_key: None,
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "review".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: None,
                    task: Some(TaskDefinition {
                        id: "review_inline".to_string(),
                        target: TaskTarget::AgentId("reviewer".to_string()),
                        prompt:
                            "Review the draft and suggest improvements:\n{{context.draft.content}}"
                                .to_string(),
                        input_bindings: Default::default(),
                        input_transform: None,
                        output_transform: None,
                        output_key: Some("review".to_string()),
                        retry_policy: None,
                        failure_policy: None,
                    }),
                },
                output_key: None,
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "fact_check".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: None,
                    task: Some(TaskDefinition {
                        id: "fact_check_inline".to_string(),
                        target: TaskTarget::Capabilities(vec!["analysis".to_string()]),
                        prompt:
                            "Fact-check this draft for {{input.topic}}:\n{{context.draft.content}}"
                                .to_string(),
                        input_bindings: Default::default(),
                        input_transform: None,
                        output_transform: None,
                        output_key: Some("fact_check".to_string()),
                        retry_policy: None,
                        failure_policy: None,
                    }),
                },
                output_key: None,
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "review_text".to_string(),
                kind: WorkflowNodeKind::Transform {
                    transform_id: "extract_content".to_string(),
                    input_key: Some("review".to_string()),
                },
                output_key: Some("review_text".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "review_ready".to_string(),
                kind: WorkflowNodeKind::Decision {
                    condition: "context.review_text != null".to_string(),
                },
                output_key: Some("review_ready".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "merge".to_string(),
                kind: WorkflowNodeKind::Join,
                output_key: Some("merged".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
        ],
        edges: vec![
            WorkflowEdgeDefinition {
                from: "draft".to_string(),
                to: "review".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "draft".to_string(),
                to: "fact_check".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "review".to_string(),
                to: "review_text".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "review_text".to_string(),
                to: "review_ready".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "review_ready".to_string(),
                to: "merge".to_string(),
                transition: WorkflowEdgeTransition::Condition(
                    "context.review_ready.matched == true".to_string(),
                ),
            },
            WorkflowEdgeDefinition {
                from: "fact_check".to_string(),
                to: "merge".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
        ],
    };

    let workspace_home = std::env::temp_dir().join(format!(
        "enki-workflow-example-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos()
    ));

    let runtime = WorkflowRuntime::builder()
        .with_workspace_home(&workspace_home)
        .with_task_runner(Arc::new(DemoTaskRunner))
        .add_task(reusable_task)
        .add_workflow(workflow)
        .build()
        .await?;

    let response = runtime
        .start(WorkflowRequest::new(
            "release-note-review",
            json!({ "topic": "runtime-managed workflows" }),
        ))
        .await?;

    println!("Workflow: {}", response.workflow_id);
    println!("Run: {}", response.run_id);
    println!("Status: {:?}", response.status);
    println!("Workspace: {}", workspace_home.display());
    println!(
        "Context:\n{}",
        serde_json::to_string_pretty(&response.context.to_value()).map_err(|err| err.to_string())?
    );

    Ok(())
}
