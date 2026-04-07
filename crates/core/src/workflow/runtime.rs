use crate::tooling::types::WorkflowToolContext;
use crate::workflow::persistence::WorkflowWorkspace;
use crate::workflow::types::{
    InterventionRequest, InterventionStatus, NodeRunState, NodeStatus, RetryPolicy, TaskDefinition,
    TaskTarget, TransformRegistry, WorkflowContext, WorkflowDefinition, WorkflowEdgeDefinition,
    WorkflowEdgeTransition, WorkflowEvent, WorkflowFailurePolicy, WorkflowNodeDefinition,
    WorkflowNodeKind, WorkflowRequest, WorkflowResponse, WorkflowRunState, WorkflowStatus,
    WorkflowTaskRunner, WorkflowTransform,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct WorkflowRuntimeBuilder {
    tasks: BTreeMap<String, TaskDefinition>,
    workflows: BTreeMap<String, WorkflowDefinition>,
    transforms: TransformRegistry,
    workspace_home: Option<PathBuf>,
    task_runner: Option<Arc<dyn WorkflowTaskRunner>>,
}

impl WorkflowRuntimeBuilder {
    pub fn new() -> Self {
        let mut transforms: TransformRegistry = BTreeMap::new();
        transforms.insert("identity".to_string(), Arc::new(IdentityTransform));
        transforms.insert(
            "extract_content".to_string(),
            Arc::new(ExtractContentTransform),
        );
        Self {
            tasks: BTreeMap::new(),
            workflows: BTreeMap::new(),
            transforms,
            workspace_home: None,
            task_runner: None,
        }
    }

    pub fn with_workspace_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.workspace_home = Some(home.into());
        self
    }

    pub fn with_task_runner(mut self, runner: Arc<dyn WorkflowTaskRunner>) -> Self {
        self.task_runner = Some(runner);
        self
    }

    pub fn add_task(mut self, task: TaskDefinition) -> Self {
        self.tasks.insert(task.id.clone(), task);
        self
    }

    pub fn add_workflow(mut self, workflow: WorkflowDefinition) -> Self {
        self.workflows.insert(workflow.id.clone(), workflow);
        self
    }

    pub fn register_transform(
        mut self,
        transform_id: impl Into<String>,
        transform: Arc<dyn WorkflowTransform>,
    ) -> Self {
        self.transforms.insert(transform_id.into(), transform);
        self
    }

    pub async fn build(self) -> Result<WorkflowRuntime, String> {
        let task_runner = self
            .task_runner
            .ok_or_else(|| "WorkflowRuntimeBuilder requires a task runner.".to_string())?;
        let workspace =
            WorkflowWorkspace::new(self.workspace_home.unwrap_or_else(|| PathBuf::from(".")));
        workspace.ensure_dirs().await?;

        let runtime = WorkflowRuntime {
            tasks: self.tasks,
            workflows: self.workflows,
            transforms: self.transforms,
            workspace,
            task_runner,
        };
        runtime.validate_all()?;
        Ok(runtime)
    }
}

impl Default for WorkflowRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WorkflowRuntime {
    tasks: BTreeMap<String, TaskDefinition>,
    workflows: BTreeMap<String, WorkflowDefinition>,
    transforms: TransformRegistry,
    workspace: WorkflowWorkspace,
    task_runner: Arc<dyn WorkflowTaskRunner>,
}

impl WorkflowRuntime {
    pub fn builder() -> WorkflowRuntimeBuilder {
        WorkflowRuntimeBuilder::new()
    }

    pub fn list_workflows(&self) -> Vec<&WorkflowDefinition> {
        self.workflows.values().collect()
    }

    pub async fn list_runs(&self) -> Result<Vec<WorkflowRunState>, String> {
        let mut runs = Vec::new();
        for run_id in self.workspace.list_run_ids().await? {
            if let Ok(state) = self.workspace.load_state(&run_id).await {
                runs.push(state);
            }
        }
        runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
        Ok(runs)
    }

    pub async fn inspect(&self, run_id: &str) -> Result<WorkflowRunState, String> {
        self.workspace.load_state(run_id).await
    }

    pub async fn start(&self, request: WorkflowRequest) -> Result<WorkflowResponse, String> {
        let workflow = self
            .workflows
            .get(&request.workflow_id)
            .cloned()
            .ok_or_else(|| format!("Workflow '{}' not found.", request.workflow_id))?;
        self.validate_workflow(&workflow)?;

        let run_id = format!("wf-{}", current_timestamp_nanos());
        self.workspace.initialize_run_dir(&run_id).await?;
        self.workspace.save_definition(&run_id, &workflow).await?;

        let mut context = WorkflowContext::default();
        context.insert("input", request.input.clone());

        let now = current_timestamp_nanos();
        let mut node_states = BTreeMap::new();
        for node in &workflow.nodes {
            let task = self.resolve_task(node)?;
            let output_key = output_key_for(node, task.as_ref());
            node_states.insert(
                node.id.clone(),
                NodeRunState {
                    node_id: node.id.clone(),
                    status: NodeStatus::Pending,
                    attempts: 0,
                    started_at: None,
                    completed_at: None,
                    last_error: None,
                    output_key,
                    output: None,
                    activated_incoming: Vec::new(),
                    session_id: None,
                },
            );
        }

        let state = WorkflowRunState {
            workflow_id: workflow.id.clone(),
            run_id: run_id.clone(),
            status: WorkflowStatus::Pending,
            created_at: now,
            updated_at: now,
            input: request.input,
            context,
            node_states,
            pending_interventions: Vec::new(),
            failed_nodes: Vec::new(),
        };
        self.workspace.save_state(&run_id, &state).await?;
        self.drive_run(workflow, state).await
    }

    pub async fn resume(&self, run_id: &str) -> Result<WorkflowResponse, String> {
        let workflow = self.workspace.load_definition(run_id).await?;
        self.validate_workflow(&workflow)?;
        let state = self.workspace.load_state(run_id).await?;
        self.drive_run(workflow, state).await
    }

    pub async fn submit_intervention(
        &self,
        run_id: &str,
        intervention_id: &str,
        response: Option<String>,
    ) -> Result<WorkflowRunState, String> {
        let mut state = self.workspace.load_state(run_id).await?;
        let request = state
            .pending_interventions
            .iter_mut()
            .find(|entry| entry.id == intervention_id)
            .ok_or_else(|| {
                format!(
                    "Intervention '{}' not found for run '{}'.",
                    intervention_id, run_id
                )
            })?;
        request.response = response;
        request.status = InterventionStatus::Resolved;
        request.resolved_at = Some(current_timestamp_nanos());
        state.updated_at = current_timestamp_nanos();
        self.workspace.save_state(run_id, &state).await?;
        Ok(state)
    }

    pub async fn list_pending_interventions(
        &self,
        run_id: &str,
    ) -> Result<Vec<InterventionRequest>, String> {
        let state = self.workspace.load_state(run_id).await?;
        Ok(state
            .pending_interventions
            .into_iter()
            .filter(|entry| entry.status == InterventionStatus::Pending)
            .collect())
    }

    async fn drive_run(
        &self,
        workflow: WorkflowDefinition,
        mut state: WorkflowRunState,
    ) -> Result<WorkflowResponse, String> {
        let mut events = Vec::new();
        if state.status == WorkflowStatus::Pending {
            state.status = WorkflowStatus::Running;
            let event = WorkflowEvent::WorkflowStarted {
                workflow_id: workflow.id.clone(),
                run_id: state.run_id.clone(),
            };
            self.record_event(&state.run_id, &mut events, event).await?;
        }

        loop {
            self.apply_resolved_interventions(&workflow, &mut state, &mut events)
                .await?;

            if matches!(
                state.status,
                WorkflowStatus::Failed
                    | WorkflowStatus::Completed
                    | WorkflowStatus::CompletedWithFailures
            ) {
                break;
            }

            if state.status == WorkflowStatus::Paused {
                if state
                    .pending_interventions
                    .iter()
                    .any(|entry| entry.status == InterventionStatus::Pending)
                {
                    break;
                }
                state.status = WorkflowStatus::Running;
            }

            let skipped = self
                .reconcile_skipped_nodes(&workflow, &mut state, &mut events)
                .await?;
            let ready = self.collect_ready_nodes(&workflow, &state)?;
            if ready.is_empty() {
                if !skipped {
                    self.finalize_if_quiescent(&mut state, &mut events).await?;
                    break;
                }
                continue;
            }

            for node_id in ready {
                if state.status == WorkflowStatus::Paused || state.status == WorkflowStatus::Failed
                {
                    break;
                }
                let event = WorkflowEvent::NodeReady {
                    node_id: node_id.clone(),
                };
                self.record_event(&state.run_id, &mut events, event).await?;
                self.execute_node(&workflow, &node_id, &mut state, &mut events)
                    .await?;
            }
        }

        state.updated_at = current_timestamp_nanos();
        self.workspace.save_state(&state.run_id, &state).await?;
        Ok(WorkflowResponse {
            workflow_id: state.workflow_id.clone(),
            run_id: state.run_id.clone(),
            status: state.status.clone(),
            context: state.context.clone(),
            events,
        })
    }

    fn validate_all(&self) -> Result<(), String> {
        for workflow in self.workflows.values() {
            self.validate_workflow(workflow)?;
        }
        Ok(())
    }

    fn validate_workflow(&self, workflow: &WorkflowDefinition) -> Result<(), String> {
        let mut node_ids = BTreeSet::new();
        for node in &workflow.nodes {
            if !node_ids.insert(node.id.clone()) {
                return Err(format!(
                    "Workflow '{}' has duplicate node '{}'.",
                    workflow.id, node.id
                ));
            }
            match &node.kind {
                WorkflowNodeKind::Task { task_id, task } => match (task_id, task) {
                    (Some(task_id), None) => {
                        if !self.tasks.contains_key(task_id) {
                            return Err(format!(
                                "Workflow '{}' references unknown task '{}'.",
                                workflow.id, task_id
                            ));
                        }
                    }
                    (None, Some(task)) => self.validate_task(task)?,
                    (Some(_), Some(_)) => {
                        return Err(format!(
                            "Workflow '{}' node '{}' cannot define both task_id and inline task.",
                            workflow.id, node.id
                        ));
                    }
                    (None, None) => {
                        return Err(format!(
                            "Workflow '{}' node '{}' must define a task_id or inline task.",
                            workflow.id, node.id
                        ));
                    }
                },
                WorkflowNodeKind::Decision { condition } => {
                    if condition.trim().is_empty() {
                        return Err(format!(
                            "Workflow '{}' decision node '{}' must define a condition.",
                            workflow.id, node.id
                        ));
                    }
                }
                WorkflowNodeKind::HumanGate { prompt } => {
                    if prompt.trim().is_empty() {
                        return Err(format!(
                            "Workflow '{}' human_gate node '{}' must define a prompt.",
                            workflow.id, node.id
                        ));
                    }
                }
                WorkflowNodeKind::Transform { transform_id, .. } => {
                    if !self.transforms.contains_key(transform_id) {
                        return Err(format!(
                            "Workflow '{}' transform node '{}' references unknown transform '{}'.",
                            workflow.id, node.id, transform_id
                        ));
                    }
                }
                WorkflowNodeKind::Join => {}
            }
        }
        let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut incoming_counts: BTreeMap<String, usize> = workflow
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0usize))
            .collect();

        for edge in &workflow.edges {
            if !node_ids.contains(&edge.from) || !node_ids.contains(&edge.to) {
                return Err(format!(
                    "Workflow '{}' edge '{}' -> '{}' references an unknown node.",
                    workflow.id, edge.from, edge.to
                ));
            }
            adjacency
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            *incoming_counts.entry(edge.to.clone()).or_default() += 1;
        }

        for node in &workflow.nodes {
            if matches!(node.kind, WorkflowNodeKind::Join)
                && incoming_counts.get(&node.id).copied().unwrap_or_default() == 0
            {
                return Err(format!(
                    "Workflow '{}' join node '{}' must have at least one incoming edge.",
                    workflow.id, node.id
                ));
            }
        }

        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for node in &workflow.nodes {
            self.visit_for_cycle(&node.id, &adjacency, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    fn visit_for_cycle(
        &self,
        node_id: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if visited.contains(node_id) {
            return Ok(());
        }
        if !visiting.insert(node_id.to_string()) {
            return Err(format!(
                "Workflow graph contains a cycle at node '{}'.",
                node_id
            ));
        }
        if let Some(children) = adjacency.get(node_id) {
            for child in children {
                self.visit_for_cycle(child, adjacency, visiting, visited)?;
            }
        }
        visiting.remove(node_id);
        visited.insert(node_id.to_string());
        Ok(())
    }

    fn validate_task(&self, task: &TaskDefinition) -> Result<(), String> {
        match &task.target {
            TaskTarget::AgentId(agent_id) if agent_id.trim().is_empty() => {
                Err("Task target agent_id cannot be empty.".to_string())
            }
            TaskTarget::Capabilities(capabilities) if capabilities.is_empty() => {
                Err("Task target capabilities cannot be empty.".to_string())
            }
            _ => Ok(()),
        }
    }

    fn resolve_task(
        &self,
        node: &WorkflowNodeDefinition,
    ) -> Result<Option<TaskDefinition>, String> {
        match &node.kind {
            WorkflowNodeKind::Task { task_id, task } => match (task_id, task) {
                (Some(task_id), None) => self
                    .tasks
                    .get(task_id)
                    .cloned()
                    .map(Some)
                    .ok_or_else(|| format!("Unknown task '{}'.", task_id)),
                (None, Some(task)) => Ok(Some(task.clone())),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    fn collect_ready_nodes(
        &self,
        workflow: &WorkflowDefinition,
        state: &WorkflowRunState,
    ) -> Result<Vec<String>, String> {
        let mut ready = Vec::new();
        for node in &workflow.nodes {
            let node_state = state
                .node_states
                .get(&node.id)
                .ok_or_else(|| format!("Missing node state for '{}'.", node.id))?;
            if node_state.status != NodeStatus::Pending {
                continue;
            }
            let incoming: Vec<&WorkflowEdgeDefinition> = workflow
                .edges
                .iter()
                .filter(|edge| edge.to == node.id)
                .collect();
            if incoming.is_empty() {
                ready.push(node.id.clone());
                continue;
            }

            let mut all_terminal = true;
            let mut activated = false;
            for edge in incoming {
                let from_state = state
                    .node_states
                    .get(&edge.from)
                    .ok_or_else(|| format!("Missing node state for '{}'.", edge.from))?;
                if !from_state.status.is_terminal() {
                    all_terminal = false;
                    break;
                }
                if edge_is_active(edge, from_state, &state.context) {
                    activated = true;
                }
            }

            if all_terminal && activated {
                ready.push(node.id.clone());
            }
        }
        Ok(ready)
    }

    async fn reconcile_skipped_nodes(
        &self,
        workflow: &WorkflowDefinition,
        state: &mut WorkflowRunState,
        events: &mut Vec<WorkflowEvent>,
    ) -> Result<bool, String> {
        let mut any_skipped = false;
        for node in &workflow.nodes {
            let current = state
                .node_states
                .get(&node.id)
                .ok_or_else(|| format!("Missing node state for '{}'.", node.id))?
                .status
                .clone();
            if current != NodeStatus::Pending {
                continue;
            }
            let incoming: Vec<&WorkflowEdgeDefinition> = workflow
                .edges
                .iter()
                .filter(|edge| edge.to == node.id)
                .collect();
            if incoming.is_empty() {
                continue;
            }
            let mut all_terminal = true;
            let mut activated = false;
            for edge in incoming {
                let from_state = state
                    .node_states
                    .get(&edge.from)
                    .ok_or_else(|| format!("Missing node state for '{}'.", edge.from))?;
                if !from_state.status.is_terminal() {
                    all_terminal = false;
                    break;
                }
                if edge_is_active(edge, from_state, &state.context) {
                    activated = true;
                }
            }
            if all_terminal && !activated {
                if let Some(node_state) = state.node_states.get_mut(&node.id) {
                    node_state.status = NodeStatus::Skipped;
                    node_state.completed_at = Some(current_timestamp_nanos());
                }
                any_skipped = true;
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::NodeSkipped {
                        node_id: node.id.clone(),
                    },
                )
                .await?;
            }
        }
        Ok(any_skipped)
    }

    async fn finalize_if_quiescent(
        &self,
        state: &mut WorkflowRunState,
        events: &mut Vec<WorkflowEvent>,
    ) -> Result<(), String> {
        if state
            .pending_interventions
            .iter()
            .any(|entry| entry.status == InterventionStatus::Pending)
        {
            state.status = WorkflowStatus::Paused;
            self.record_event(
                &state.run_id,
                events,
                WorkflowEvent::WorkflowPaused {
                    run_id: state.run_id.clone(),
                    reason: "Waiting for intervention".to_string(),
                },
            )
            .await?;
            return Ok(());
        }

        state.status = if state.failed_nodes.is_empty() {
            WorkflowStatus::Completed
        } else {
            WorkflowStatus::CompletedWithFailures
        };
        self.record_event(
            &state.run_id,
            events,
            WorkflowEvent::WorkflowCompleted {
                run_id: state.run_id.clone(),
                status: state.status.clone(),
            },
        )
        .await
    }

    async fn execute_node(
        &self,
        workflow: &WorkflowDefinition,
        node_id: &str,
        state: &mut WorkflowRunState,
        events: &mut Vec<WorkflowEvent>,
    ) -> Result<(), String> {
        let node = workflow
            .nodes
            .iter()
            .find(|candidate| candidate.id == node_id)
            .ok_or_else(|| format!("Workflow node '{}' not found.", node_id))?;
        let task = self.resolve_task(node)?;
        let attempt = state
            .node_states
            .get(node_id)
            .map(|entry| entry.attempts + 1)
            .unwrap_or(1);
        let output_key = output_key_for(node, task.as_ref());
        let session_id = format!("wf-{}-{}-attempt-{}", state.run_id, node_id, attempt);

        let activated_incoming = workflow
            .edges
            .iter()
            .filter(|edge| edge.to == node_id)
            .filter_map(|edge| {
                let from_state = state.node_states.get(&edge.from)?;
                if edge_is_active(edge, from_state, &state.context) {
                    Some(edge.from.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        {
            let node_state = state
                .node_states
                .get_mut(node_id)
                .ok_or_else(|| format!("Missing node state for '{}'.", node_id))?;
            node_state.status = NodeStatus::Running;
            node_state.attempts = attempt;
            node_state.started_at = Some(current_timestamp_nanos());
            node_state.session_id = Some(session_id.clone());
            node_state.output_key = output_key.clone();
            node_state.activated_incoming = activated_incoming;
        }

        self.record_event(
            &state.run_id,
            events,
            WorkflowEvent::NodeStarted {
                node_id: node_id.to_string(),
                attempt,
            },
        )
        .await?;

        let result = match &node.kind {
            WorkflowNodeKind::Task { .. } => {
                let task =
                    task.ok_or_else(|| format!("Workflow node '{}' is missing a task.", node_id))?;
                self.execute_task_node(state, node, &task, attempt, &session_id)
                    .await
            }
            WorkflowNodeKind::Decision { condition } => {
                self.execute_decision_node(state, condition)
            }
            WorkflowNodeKind::HumanGate { prompt } => {
                self.execute_human_gate_node(state, node, prompt).await
            }
            WorkflowNodeKind::Transform {
                transform_id,
                input_key,
            } => {
                self.execute_transform_node(state, transform_id, input_key.as_deref())
                    .await
            }
            WorkflowNodeKind::Join => Ok(json!({
                "joined": state
                    .node_states
                    .get(node_id)
                    .map(|entry| entry.activated_incoming.clone())
                    .unwrap_or_default()
            })),
        };

        match result {
            Ok(output) => {
                state.context.insert(output_key.clone(), output.clone());
                if let Some(node_state) = state.node_states.get_mut(node_id) {
                    node_state.status = NodeStatus::Completed;
                    node_state.completed_at = Some(current_timestamp_nanos());
                    node_state.output = Some(output);
                    node_state.last_error = None;
                }
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::NodeCompleted {
                        node_id: node_id.to_string(),
                        output_key,
                    },
                )
                .await?;
            }
            Err(error) => {
                if state.status == WorkflowStatus::Paused {
                    if let Some(intervention) = state
                        .pending_interventions
                        .iter()
                        .find(|entry| {
                            entry.node_id == node.id && entry.status == InterventionStatus::Pending
                        })
                        .cloned()
                    {
                        self.record_event(
                            &state.run_id,
                            events,
                            WorkflowEvent::InterventionRequested {
                                intervention_id: intervention.id,
                                node_id: node.id.clone(),
                                reason: intervention.reason,
                            },
                        )
                        .await?;
                    }
                    self.record_event(
                        &state.run_id,
                        events,
                        WorkflowEvent::WorkflowPaused {
                            run_id: state.run_id.clone(),
                            reason: error,
                        },
                    )
                    .await?;
                } else {
                    self.handle_node_failure(workflow, state, node, attempt, error, events)
                        .await?;
                }
            }
        }

        state.updated_at = current_timestamp_nanos();
        self.workspace.save_state(&state.run_id, state).await
    }
    async fn execute_task_node(
        &self,
        state: &WorkflowRunState,
        node: &WorkflowNodeDefinition,
        task: &TaskDefinition,
        attempt: usize,
        session_id: &str,
    ) -> Result<Value, String> {
        let mut input = if task.input_bindings.is_empty() {
            state
                .context
                .get("input")
                .cloned()
                .unwrap_or_else(|| json!({}))
        } else {
            let mut map = Map::new();
            for (alias, path) in &task.input_bindings {
                map.insert(
                    alias.clone(),
                    state.context.lookup_path(path).unwrap_or(Value::Null),
                );
            }
            Value::Object(map)
        };

        if let Some(transform_id) = &task.input_transform {
            input = self
                .apply_transform(transform_id, &input, &state.context)
                .await?;
        }

        let prompt = render_prompt(&task.prompt, &input, &state.context);
        let metadata = WorkflowToolContext {
            workflow_id: state.workflow_id.clone(),
            run_id: state.run_id.clone(),
            node_id: node.id.clone(),
            attempt,
        };
        let workspace_dir = self.workspace.task_workspace(&state.run_id, &node.id);
        #[cfg(not(target_arch = "wasm32"))]
        tokio::fs::create_dir_all(&workspace_dir)
            .await
            .map_err(|e| format!("Failed to create workflow task workspace: {e}"))?;

        let result = self
            .task_runner
            .run_task(&task.target, &metadata, &workspace_dir, &prompt)
            .await?;

        let mut output = result.value;
        if let Some(transform_id) = &task.output_transform {
            output = self
                .apply_transform(transform_id, &output, &state.context)
                .await?;
        }
        if output.is_null() {
            output = json!({
                "content": result.content,
                "agent_id": result.agent_id,
                "session_id": session_id,
                "attempt": attempt,
            });
        }
        Ok(output)
    }

    fn execute_decision_node(
        &self,
        state: &WorkflowRunState,
        condition: &str,
    ) -> Result<Value, String> {
        Ok(json!({
            "matched": evaluate_condition(condition, &state.context)
        }))
    }

    async fn execute_human_gate_node(
        &self,
        state: &mut WorkflowRunState,
        node: &WorkflowNodeDefinition,
        prompt: &str,
    ) -> Result<Value, String> {
        if let Some(existing) = state
            .pending_interventions
            .iter()
            .find(|entry| entry.node_id == node.id && entry.status == InterventionStatus::Resolved)
        {
            let response = existing.response.clone().unwrap_or_default();
            return Ok(json!({
                "response": response.clone(),
                "approved": is_truthy(&Value::String(response)),
            }));
        }

        let intervention_id = format!("int-{}", current_timestamp_nanos());
        state.pending_interventions.push(InterventionRequest {
            id: intervention_id,
            workflow_id: state.workflow_id.clone(),
            run_id: state.run_id.clone(),
            node_id: node.id.clone(),
            prompt: prompt.to_string(),
            reason: "human_gate".to_string(),
            response: None,
            status: InterventionStatus::Pending,
            created_at: current_timestamp_nanos(),
            resolved_at: None,
        });
        if let Some(node_state) = state.node_states.get_mut(&node.id) {
            node_state.status = NodeStatus::Paused;
        }
        state.status = WorkflowStatus::Paused;
        Err("Human intervention required".to_string())
    }

    async fn execute_transform_node(
        &self,
        state: &WorkflowRunState,
        transform_id: &str,
        input_key: Option<&str>,
    ) -> Result<Value, String> {
        let input = match input_key {
            Some(key) => state.context.lookup_path(key).unwrap_or(Value::Null),
            None => state.context.to_value(),
        };
        self.apply_transform(transform_id, &input, &state.context)
            .await
    }

    async fn handle_node_failure(
        &self,
        workflow: &WorkflowDefinition,
        state: &mut WorkflowRunState,
        node: &WorkflowNodeDefinition,
        attempt: usize,
        error: String,
        events: &mut Vec<WorkflowEvent>,
    ) -> Result<(), String> {
        let retry_policy =
            effective_retry_policy(workflow, node, self.resolve_task(node)?.as_ref());
        if attempt < retry_policy.max_attempts {
            if let Some(node_state) = state.node_states.get_mut(&node.id) {
                node_state.status = NodeStatus::Pending;
                node_state.last_error = Some(error.clone());
            }
            self.record_event(
                &state.run_id,
                events,
                WorkflowEvent::NodeRetryScheduled {
                    node_id: node.id.clone(),
                    attempt,
                    error,
                },
            )
            .await?;
            return Ok(());
        }

        let failure_policy =
            effective_failure_policy(workflow, node, self.resolve_task(node)?.as_ref());
        if let Some(node_state) = state.node_states.get_mut(&node.id) {
            node_state.last_error = Some(error.clone());
            node_state.completed_at = Some(current_timestamp_nanos());
        }

        match failure_policy {
            WorkflowFailurePolicy::ContinueBestEffort => {
                if let Some(node_state) = state.node_states.get_mut(&node.id) {
                    node_state.status = NodeStatus::Failed;
                }
                if !state.failed_nodes.contains(&node.id) {
                    state.failed_nodes.push(node.id.clone());
                }
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::NodeFailed {
                        node_id: node.id.clone(),
                        error,
                    },
                )
                .await?;
            }
            WorkflowFailurePolicy::FailWorkflow => {
                if let Some(node_state) = state.node_states.get_mut(&node.id) {
                    node_state.status = NodeStatus::Failed;
                }
                if !state.failed_nodes.contains(&node.id) {
                    state.failed_nodes.push(node.id.clone());
                }
                state.status = WorkflowStatus::Failed;
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::NodeFailed {
                        node_id: node.id.clone(),
                        error: error.clone(),
                    },
                )
                .await?;
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::WorkflowCompleted {
                        run_id: state.run_id.clone(),
                        status: WorkflowStatus::Failed,
                    },
                )
                .await?;
            }
            WorkflowFailurePolicy::PauseForIntervention => {
                if let Some(node_state) = state.node_states.get_mut(&node.id) {
                    node_state.status = NodeStatus::Paused;
                }
                let intervention_id = format!("int-{}", current_timestamp_nanos());
                state.pending_interventions.push(InterventionRequest {
                    id: intervention_id.clone(),
                    workflow_id: state.workflow_id.clone(),
                    run_id: state.run_id.clone(),
                    node_id: node.id.clone(),
                    prompt: format!(
                        "Node '{}' failed after {} attempt(s). Reply with retry, skip, continue, or fail.",
                        node.id, attempt
                    ),
                    reason: format!("step_failure:{error}"),
                    response: None,
                    status: InterventionStatus::Pending,
                    created_at: current_timestamp_nanos(),
                    resolved_at: None,
                });
                state.status = WorkflowStatus::Paused;
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::InterventionRequested {
                        intervention_id,
                        node_id: node.id.clone(),
                        reason: error,
                    },
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn apply_resolved_interventions(
        &self,
        workflow: &WorkflowDefinition,
        state: &mut WorkflowRunState,
        events: &mut Vec<WorkflowEvent>,
    ) -> Result<(), String> {
        let mut remaining = Vec::new();
        let interventions = std::mem::take(&mut state.pending_interventions);
        for intervention in interventions {
            if intervention.status == InterventionStatus::Pending {
                remaining.push(intervention);
                continue;
            }

            self.record_event(
                &state.run_id,
                events,
                WorkflowEvent::InterventionResolved {
                    intervention_id: intervention.id.clone(),
                    node_id: intervention.node_id.clone(),
                },
            )
            .await?;

            let node = workflow
                .nodes
                .iter()
                .find(|candidate| candidate.id == intervention.node_id)
                .ok_or_else(|| format!("Workflow node '{}' not found.", intervention.node_id))?;

            if intervention.reason == "human_gate" {
                let response = intervention.response.clone().unwrap_or_default();
                let output = json!({
                    "response": response.clone(),
                    "approved": is_truthy(&Value::String(response)),
                });
                if let Some(node_state) = state.node_states.get_mut(&node.id) {
                    node_state.status = NodeStatus::Completed;
                    node_state.completed_at = Some(current_timestamp_nanos());
                    node_state.output = Some(output.clone());
                    node_state.last_error = None;
                }
                let output_key = state
                    .node_states
                    .get(&node.id)
                    .map(|entry| entry.output_key.clone())
                    .unwrap_or_else(|| node.id.clone());
                state.context.insert(output_key.clone(), output);
                self.record_event(
                    &state.run_id,
                    events,
                    WorkflowEvent::NodeCompleted {
                        node_id: node.id.clone(),
                        output_key,
                    },
                )
                .await?;
                continue;
            }

            match intervention
                .response
                .clone()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "retry" => {
                    if let Some(node_state) = state.node_states.get_mut(&node.id) {
                        node_state.status = NodeStatus::Pending;
                    }
                    state.status = WorkflowStatus::Running;
                }
                "skip" => {
                    if let Some(node_state) = state.node_states.get_mut(&node.id) {
                        node_state.status = NodeStatus::Skipped;
                        node_state.completed_at = Some(current_timestamp_nanos());
                    }
                    state.status = WorkflowStatus::Running;
                    self.record_event(
                        &state.run_id,
                        events,
                        WorkflowEvent::NodeSkipped {
                            node_id: node.id.clone(),
                        },
                    )
                    .await?;
                }
                "continue" => {
                    if let Some(node_state) = state.node_states.get_mut(&node.id) {
                        node_state.status = NodeStatus::Failed;
                        node_state.completed_at = Some(current_timestamp_nanos());
                    }
                    if !state.failed_nodes.contains(&node.id) {
                        state.failed_nodes.push(node.id.clone());
                    }
                    state.status = WorkflowStatus::Running;
                }
                _ => {
                    if let Some(node_state) = state.node_states.get_mut(&node.id) {
                        node_state.status = NodeStatus::Failed;
                        node_state.completed_at = Some(current_timestamp_nanos());
                    }
                    if !state.failed_nodes.contains(&node.id) {
                        state.failed_nodes.push(node.id.clone());
                    }
                    state.status = WorkflowStatus::Failed;
                }
            }
        }
        state.pending_interventions = remaining;
        Ok(())
    }

    async fn apply_transform(
        &self,
        transform_id: &str,
        input: &Value,
        context: &WorkflowContext,
    ) -> Result<Value, String> {
        let transform = self
            .transforms
            .get(transform_id)
            .ok_or_else(|| format!("Unknown transform '{}'.", transform_id))?;
        transform.apply(input, context).await
    }

    async fn record_event(
        &self,
        run_id: &str,
        events: &mut Vec<WorkflowEvent>,
        event: WorkflowEvent,
    ) -> Result<(), String> {
        self.workspace.append_event(run_id, &event).await?;
        events.push(event);
        Ok(())
    }
}
struct IdentityTransform;

#[async_trait::async_trait(?Send)]
impl WorkflowTransform for IdentityTransform {
    async fn apply(&self, input: &Value, _context: &WorkflowContext) -> Result<Value, String> {
        Ok(input.clone())
    }
}

struct ExtractContentTransform;

#[async_trait::async_trait(?Send)]
impl WorkflowTransform for ExtractContentTransform {
    async fn apply(&self, input: &Value, _context: &WorkflowContext) -> Result<Value, String> {
        match input {
            Value::Object(map) => Ok(map.get("content").cloned().unwrap_or_else(|| input.clone())),
            _ => Ok(input.clone()),
        }
    }
}

fn output_key_for(node: &WorkflowNodeDefinition, task: Option<&TaskDefinition>) -> String {
    node.output_key
        .clone()
        .or_else(|| task.and_then(|task| task.output_key.clone()))
        .unwrap_or_else(|| node.id.clone())
}

fn effective_retry_policy(
    workflow: &WorkflowDefinition,
    node: &WorkflowNodeDefinition,
    task: Option<&TaskDefinition>,
) -> RetryPolicy {
    node.retry_policy
        .clone()
        .or_else(|| task.and_then(|task| task.retry_policy.clone()))
        .or_else(|| workflow.retry_policy.clone())
        .unwrap_or_default()
}

fn effective_failure_policy(
    workflow: &WorkflowDefinition,
    node: &WorkflowNodeDefinition,
    task: Option<&TaskDefinition>,
) -> WorkflowFailurePolicy {
    node.failure_policy
        .clone()
        .or_else(|| task.and_then(|task| task.failure_policy.clone()))
        .or_else(|| workflow.failure_policy.clone())
        .unwrap_or_default()
}

fn edge_is_active(
    edge: &WorkflowEdgeDefinition,
    from_state: &NodeRunState,
    context: &WorkflowContext,
) -> bool {
    match &edge.transition {
        WorkflowEdgeTransition::Always => from_state.status.is_terminal(),
        WorkflowEdgeTransition::OnSuccess => from_state.status == NodeStatus::Completed,
        WorkflowEdgeTransition::OnFailure => from_state.status == NodeStatus::Failed,
        WorkflowEdgeTransition::Condition(condition) => {
            from_state.status == NodeStatus::Completed && evaluate_condition(condition, context)
        }
    }
}

fn evaluate_condition(condition: &str, context: &WorkflowContext) -> bool {
    let condition = condition.trim();
    if let Some((left, right)) = condition.split_once("==") {
        return normalize_condition_value(context.lookup_path(normalize_path(left.trim())))
            == parse_literal(right.trim());
    }
    if let Some((left, right)) = condition.split_once("!=") {
        return normalize_condition_value(context.lookup_path(normalize_path(left.trim())))
            != parse_literal(right.trim());
    }
    if let Some(path) = condition.strip_prefix('!') {
        return !is_truthy(
            &context
                .lookup_path(normalize_path(path.trim()))
                .unwrap_or(Value::Null),
        );
    }
    is_truthy(
        &context
            .lookup_path(normalize_path(condition))
            .unwrap_or(Value::Null),
    )
}

fn normalize_path(path: &str) -> &str {
    path.strip_prefix("context.").unwrap_or(path)
}

fn normalize_condition_value(value: Option<Value>) -> Value {
    value.unwrap_or(Value::Null)
}

fn parse_literal(raw: &str) -> Value {
    let raw = raw.trim();
    if (raw.starts_with('"') && raw.ends_with('"'))
        || (raw.starts_with('\'') && raw.ends_with('\''))
    {
        return Value::String(raw[1..raw.len().saturating_sub(1)].to_string());
    }
    if raw.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if raw.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if let Ok(number) = raw.parse::<i64>() {
        return json!(number);
    }
    if let Ok(number) = raw.parse::<f64>() {
        return json!(number);
    }
    Value::String(raw.to_string())
}
fn render_prompt(template: &str, input: &Value, context: &WorkflowContext) -> String {
    let mut rendered = String::new();
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        rendered.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        if let Some(end) = after_start.find("}}") {
            let key = after_start[..end].trim();
            let replacement = resolve_template_value(key, input, context);
            rendered.push_str(&replacement);
            remaining = &after_start[end + 2..];
        } else {
            rendered.push_str(&remaining[start..]);
            remaining = "";
            break;
        }
    }
    rendered.push_str(remaining);

    if !matches!(input, Value::Null)
        && !(input.is_object() && input.as_object().map(|map| map.is_empty()).unwrap_or(false))
        && !template.contains("{{")
    {
        if !rendered.trim().is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str("Workflow input:\n");
        rendered
            .push_str(&serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string()));
    }

    rendered
}

fn resolve_template_value(key: &str, input: &Value, context: &WorkflowContext) -> String {
    if let Some(path) = key.strip_prefix("input.") {
        if let Some(value) = lookup_value(input, path) {
            return value_to_template_string(&value);
        }
    }
    if let Some(value) = context.lookup_path(normalize_path(key)) {
        return value_to_template_string(&value);
    }
    String::new()
}

fn lookup_value(root: &Value, path: &str) -> Option<Value> {
    let mut current = root.clone();
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?.clone(),
            _ => return None,
        };
    }
    Some(current)
}

fn value_to_template_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(false),
        Value::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !(normalized.is_empty()
                || normalized == "false"
                || normalized == "0"
                || normalized == "no")
        }
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
    }
}

fn current_timestamp_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
