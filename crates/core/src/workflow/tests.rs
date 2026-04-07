use super::*;
use async_trait::async_trait;
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

struct MockRunner {
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait(?Send)]
impl WorkflowTaskRunner for MockRunner {
    async fn run_task(
        &self,
        target: &TaskTarget,
        metadata: &crate::tooling::types::WorkflowToolContext,
        workspace_dir: &Path,
        prompt: &str,
    ) -> Result<WorkflowTaskResult, String> {
        self.prompts.lock().unwrap().push(prompt.to_string());
        let agent_id = match target {
            TaskTarget::AgentId(agent_id) => agent_id.clone(),
            TaskTarget::Capabilities(capabilities) => capabilities.join("+"),
        };
        Ok(WorkflowTaskResult {
            content: format!("done:{}:{}", metadata.node_id, workspace_dir.display()),
            value: json!({
                "content": format!("done:{}", metadata.node_id),
                "agent_id": agent_id,
            }),
            agent_id,
            steps: Vec::new(),
        })
    }
}

fn temp_home(label: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!("core-next-workflow-{label}-{suffix}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn task(id: &str) -> TaskDefinition {
    TaskDefinition {
        id: id.to_string(),
        target: TaskTarget::AgentId("helper".to_string()),
        prompt: "Handle {{input.topic}}".to_string(),
        input_bindings: Default::default(),
        input_transform: None,
        output_transform: None,
        output_key: None,
        retry_policy: None,
        failure_policy: None,
    }
}

#[tokio::test]
async fn workflow_runtime_runs_task_and_persists_state() {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let runtime = WorkflowRuntime::builder()
        .with_workspace_home(temp_home("linear"))
        .with_task_runner(Arc::new(MockRunner {
            prompts: prompts.clone(),
        }))
        .add_task(task("research"))
        .add_workflow(WorkflowDefinition {
            id: "flow".to_string(),
            name: "Flow".to_string(),
            nodes: vec![WorkflowNodeDefinition {
                id: "research".to_string(),
                kind: WorkflowNodeKind::Task {
                    task_id: Some("research".to_string()),
                    task: None,
                },
                output_key: Some("research_output".to_string()),
                retry_policy: None,
                failure_policy: None,
            }],
            edges: Vec::new(),
            retry_policy: None,
            failure_policy: None,
        })
        .build()
        .await
        .unwrap();

    let response = runtime
        .start(WorkflowRequest::new("flow", json!({ "topic": "runtime" })))
        .await
        .unwrap();

    assert_eq!(response.status, WorkflowStatus::Completed);
    assert!(response.context.get("research_output").is_some());
    assert_eq!(prompts.lock().unwrap()[0], "Handle runtime");

    let persisted = runtime.inspect(&response.run_id).await.unwrap();
    assert_eq!(persisted.status, WorkflowStatus::Completed);
}

#[tokio::test]
async fn human_gate_pauses_and_resumes() {
    let runtime = WorkflowRuntime::builder()
        .with_workspace_home(temp_home("human"))
        .with_task_runner(Arc::new(MockRunner {
            prompts: Arc::new(Mutex::new(Vec::new())),
        }))
        .add_workflow(WorkflowDefinition {
            id: "approval-flow".to_string(),
            name: "Approval".to_string(),
            nodes: vec![WorkflowNodeDefinition {
                id: "approval".to_string(),
                kind: WorkflowNodeKind::HumanGate {
                    prompt: "Approve?".to_string(),
                },
                output_key: Some("approval".to_string()),
                retry_policy: None,
                failure_policy: None,
            }],
            edges: Vec::new(),
            retry_policy: None,
            failure_policy: None,
        })
        .build()
        .await
        .unwrap();

    let response = runtime
        .start(WorkflowRequest::new("approval-flow", json!({})))
        .await
        .unwrap();
    assert_eq!(response.status, WorkflowStatus::Paused);
    let pending = runtime
        .list_pending_interventions(&response.run_id)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    runtime
        .submit_intervention(&response.run_id, &pending[0].id, Some("yes".to_string()))
        .await
        .unwrap();
    let resumed = runtime.resume(&response.run_id).await.unwrap();
    assert_eq!(resumed.status, WorkflowStatus::Completed);
    assert_eq!(
        resumed.context.lookup_path("approval.approved").unwrap(),
        json!(true)
    );
}
