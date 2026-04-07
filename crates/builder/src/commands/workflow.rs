use crate::cli::{
    WorkflowInspectArgs, WorkflowJoinArgs, WorkflowListArgs, WorkflowResumeArgs, WorkflowRunArgs,
};
use crate::manifest::Manifest;
use crate::project_runtime;
use core_next::agent::AgentDefinition;
use core_next::runtime::multi_agent::MultiAgentRuntimeBuilder;
use core_next::workflow::{
    WorkflowRequest, WorkflowRuntime, WorkflowRuntimeBuilder, WorkflowStatus, WorkflowTaskRunner,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub async fn list(args: WorkflowListArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    println!("\x1b[1;36mWorkflows\x1b[0m - {}", manifest.project.name);
    println!();
    if manifest.workflows.is_empty() {
        println!("  No workflows configured.");
        println!();
        return Ok(());
    }

    println!("  {:<24} {:<32} {}", "ID", "NAME", "NODES");
    println!("  {}", "-".repeat(72));
    for workflow in &manifest.workflows {
        println!(
            "  {:<24} {:<32} {}",
            workflow.id,
            workflow.name.as_deref().unwrap_or(&workflow.id),
            workflow.nodes.len()
        );
    }
    println!();
    Ok(())
}

pub async fn run(args: WorkflowRunArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest).await?;
    let input = parse_input(&args.input)?;
    let response = runtime
        .start(WorkflowRequest::new(args.workflow, input))
        .await?;
    print_response(
        &response.run_id,
        &response.status,
        response.context.to_value(),
    );
    Ok(())
}

pub async fn inspect(args: WorkflowInspectArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest).await?;
    let state = runtime.inspect(&args.run).await?;
    let raw = serde_json::to_string_pretty(&state)
        .map_err(|e| format!("Failed to render workflow state: {e}"))?;
    println!("{}", raw);
    Ok(())
}

pub async fn resume(args: WorkflowResumeArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest).await?;
    let response = runtime.resume(&args.run).await?;
    print_response(
        &response.run_id,
        &response.status,
        response.context.to_value(),
    );
    Ok(())
}

pub async fn join(args: WorkflowJoinArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest).await?;
    let stdin = std::io::stdin();
    let mut input = String::new();

    loop {
        let pending = runtime.list_pending_interventions(&args.run).await?;
        if pending.is_empty() {
            let response = runtime.resume(&args.run).await?;
            print_response(
                &response.run_id,
                &response.status,
                response.context.to_value(),
            );
            if response.status != WorkflowStatus::Paused {
                break;
            }
            continue;
        }

        for intervention in pending {
            println!("\x1b[1;36mIntervention\x1b[0m {}", intervention.id);
            println!("  Node: {}", intervention.node_id);
            println!("  Reason: {}", intervention.reason);
            println!("  Prompt: {}", intervention.prompt);
            print!("  Response: ");
            use std::io::Write;
            std::io::stdout().flush().map_err(|e| e.to_string())?;
            input.clear();
            stdin.read_line(&mut input).map_err(|e| e.to_string())?;
            runtime
                .submit_intervention(&args.run, &intervention.id, Some(input.trim().to_string()))
                .await?;
            println!();
        }
    }

    Ok(())
}

async fn build_workflow_runtime(
    manifest_path: &Path,
    manifest: &Manifest,
) -> Result<WorkflowRuntime, String> {
    let project_dir = manifest_path.parent().unwrap_or(Path::new("."));
    if project_runtime::is_python_project(project_dir) {
        return Err(
            "Workflow CLI v1 uses the Rust core runtime and does not execute Python-scripted agents yet."
                .to_string(),
        );
    }

    for transform in &manifest.transforms {
        if !transform.kind.eq_ignore_ascii_case("builtin") {
            return Err(format!(
                "Workflow transform '{}' uses kind '{}'. CLI v1 supports built-in transforms only; register custom transforms through the Rust runtime API.",
                transform.id, transform.kind
            ));
        }
        if !matches!(transform.id.as_str(), "identity" | "extract_content") {
            return Err(format!(
                "Workflow transform '{}' is not a built-in transform. Available built-ins: identity, extract_content.",
                transform.id
            ));
        }
    }

    let workspace_home = workspace_home(manifest_path, manifest);
    let mut multi_builder = MultiAgentRuntimeBuilder::new().with_workspace_home(&workspace_home);
    for agent_cfg in &manifest.agents {
        multi_builder = multi_builder.add_agent(
            &agent_cfg.id,
            AgentDefinition {
                name: agent_cfg.name.clone(),
                system_prompt_preamble: agent_cfg.system_prompt.clone(),
                model: agent_cfg.model.clone(),
                max_iterations: agent_cfg.max_iterations,
            },
            agent_cfg.capabilities.clone(),
        );
    }
    let multi_runtime = multi_builder.build().await?;
    let runner: Arc<dyn WorkflowTaskRunner> = Arc::new(multi_runtime);

    let mut builder = WorkflowRuntimeBuilder::new()
        .with_workspace_home(workspace_home)
        .with_task_runner(runner);

    for task in manifest.workflow_tasks()? {
        builder = builder.add_task(task);
    }
    for workflow in manifest.workflow_definitions()? {
        builder = builder.add_workflow(workflow);
    }

    builder.build().await
}

fn workspace_home(manifest_path: &Path, manifest: &Manifest) -> PathBuf {
    manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&manifest.workspace.home)
}

fn parse_input(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).or_else(|_| Ok(json!({ "message": raw })))
}

fn print_response(run_id: &str, status: &WorkflowStatus, context: Value) {
    println!("\x1b[1;36mWorkflow run\x1b[0m {}", run_id);
    println!("  Status: {:?}", status);
    println!("  Context:");
    println!(
        "{}",
        serde_json::to_string_pretty(&context).unwrap_or_else(|_| context.to_string())
    );
    println!();
}
