use async_trait::async_trait;
use enki_next::tooling::types::WorkflowToolContext;
use enki_next::workflow::{
    TaskDefinition, TaskTarget, WorkflowContext, WorkflowDefinition, WorkflowEdgeDefinition,
    WorkflowEdgeTransition, WorkflowEvent, WorkflowEventListener, WorkflowFailurePolicy,
    WorkflowNodeDefinition, WorkflowNodeKind, WorkflowRequest, WorkflowRuntime, WorkflowTaskResult,
    WorkflowTaskRunner, WorkflowTransform,
};
use enki_rs_examples::temp_workspace;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

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
        let target_name = match target {
            TaskTarget::AgentId(agent_id) => format!("agent:{agent_id}"),
            TaskTarget::Capabilities(capabilities) => {
                format!("capabilities:{}", capabilities.join("+"))
            }
        };

        let content = if metadata.node_id == "research" {
            "Library consumers need examples that stay aligned with exported runtime and workflow types."
                .to_string()
        } else {
            format!(
                "Drafted release summary for {target_name}: {}",
                prompt.lines().next().unwrap_or(prompt)
            )
        };

        Ok(WorkflowTaskResult {
            content: content.clone(),
            value: json!({
                "content": content,
                "target": target_name,
                "node_id": metadata.node_id,
                "workspace_dir": workspace_dir.display().to_string(),
            }),
            agent_id: "demo-runner".to_string(),
            steps: Vec::new(),
        })
    }
}

struct ReleasePacketTransform;

#[async_trait(?Send)]
impl WorkflowTransform for ReleasePacketTransform {
    async fn apply(&self, input: &Value, context: &WorkflowContext) -> Result<Value, String> {
        Ok(json!({
            "approved": context.lookup_path("approval.approved").unwrap_or(Value::Bool(false)),
            "summary": input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "research_note": context.lookup_path("research.content").unwrap_or(Value::Null),
        }))
    }
}

struct LoggingListener;

#[async_trait(?Send)]
impl WorkflowEventListener for LoggingListener {
    async fn on_event(&self, event: &WorkflowEvent) -> Result<(), String> {
        println!("event => {:?}", event);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let workspace_home = temp_workspace("workflow")?;

    let research_task = TaskDefinition {
        id: "research_release_scope".to_string(),
        target: TaskTarget::Capabilities(vec!["research".to_string()]),
        prompt: "Research the release topic: {{input.topic}}".to_string(),
        input_bindings: Default::default(),
        input_transform: None,
        output_transform: None,
        output_key: Some("research".to_string()),
        retry_policy: None,
        failure_policy: None,
    };

    let workflow = WorkflowDefinition {
        id: "library-release-review".to_string(),
        name: "Library Release Review".to_string(),
        retry_policy: None,
        failure_policy: Some(WorkflowFailurePolicy::ContinueBestEffort),
        nodes: vec![
            WorkflowNodeDefinition {
                id: "research".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: Some("research_release_scope".to_string()),
                    task: None,
                },
                output_key: None,
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "draft".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: None,
                    task: Some(TaskDefinition {
                        id: "draft_inline".to_string(),
                        target: TaskTarget::AgentId("release-writer".to_string()),
                        prompt: "Draft a consumer-facing release summary.\n\nResearch:\n{{context.research.content}}".to_string(),
                        input_bindings: Default::default(),
                        input_transform: None,
                        output_transform: None,
                        output_key: Some("draft".to_string()),
                        retry_policy: None,
                        failure_policy: None,
                    }),
                },
                output_key: None,
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "approval".to_string(),
                kind: WorkflowNodeKind::HumanGate {
                    prompt: "Approve the generated release summary?".to_string(),
                },
                output_key: Some("approval".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "approval_decision".to_string(),
                kind: WorkflowNodeKind::Decision {
                    condition: "context.approval.approved == true".to_string(),
                },
                output_key: Some("approval_decision".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
            WorkflowNodeDefinition {
                id: "release_packet".to_string(),
                kind: WorkflowNodeKind::Transform {
                    transform_id: "release_packet".to_string(),
                    input_key: Some("draft".to_string()),
                },
                output_key: Some("release_packet".to_string()),
                retry_policy: None,
                failure_policy: None,
            },
        ],
        edges: vec![
            WorkflowEdgeDefinition {
                from: "research".to_string(),
                to: "draft".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "draft".to_string(),
                to: "approval".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "approval".to_string(),
                to: "approval_decision".to_string(),
                transition: WorkflowEdgeTransition::OnSuccess,
            },
            WorkflowEdgeDefinition {
                from: "approval_decision".to_string(),
                to: "release_packet".to_string(),
                transition: WorkflowEdgeTransition::Condition(
                    "context.approval_decision.matched == true".to_string(),
                ),
            },
        ],
    };

    let runtime = WorkflowRuntime::builder()
        .with_workspace_home(workspace_home.clone())
        .with_task_runner(Arc::new(DemoTaskRunner))
        .with_event_listener(Arc::new(LoggingListener))
        .register_transform("release_packet", Arc::new(ReleasePacketTransform))
        .add_task(research_task)
        .add_workflow(workflow)
        .build()
        .await?;

    println!("Workspace: {}", workspace_home.display());
    println!(
        "Registered workflows: {:?}",
        runtime
            .list_workflows()
            .into_iter()
            .map(|workflow| workflow.id.clone())
            .collect::<Vec<_>>()
    );

    let initial = runtime
        .start(WorkflowRequest::new(
            "library-release-review",
            json!({ "topic": "embedded Rust examples" }),
        ))
        .await?;

    println!("\nInitial status: {:?}", initial.status);
    println!(
        "Initial context:\n{}",
        serde_json::to_string_pretty(&initial.context.to_value()).map_err(|error| error.to_string())?
    );

    let pending = runtime.list_pending_interventions(&initial.run_id).await?;
    println!(
        "\nPending interventions:\n{}",
        serde_json::to_string_pretty(&pending).map_err(|error| error.to_string())?
    );

    let approval = pending
        .first()
        .ok_or_else(|| "Expected a pending human gate intervention.".to_string())?;
    runtime
        .submit_intervention(&initial.run_id, &approval.id, Some("yes".to_string()))
        .await?;

    let resumed = runtime.resume(&initial.run_id).await?;
    println!("\nResumed status: {:?}", resumed.status);
    println!(
        "Release packet:\n{}",
        serde_json::to_string_pretty(
            &resumed
                .context
                .lookup_path("release_packet")
                .unwrap_or(Value::Null)
        )
        .map_err(|error| error.to_string())?
    );

    let persisted = runtime.inspect(&initial.run_id).await?;
    println!("\nPersisted run status: {:?}", persisted.status);
    println!(
        "All runs in workspace: {}",
        runtime.list_runs().await?.len()
    );

    Ok(())
}
