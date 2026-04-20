use crate::cli::{
    WorkflowInspectArgs, WorkflowJoinArgs, WorkflowListArgs, WorkflowNewArgs, WorkflowResumeArgs,
    WorkflowRunArgs,
};
use crate::manifest::Manifest;
use enki_next::agent::AgentDefinition;
use enki_next::runtime::multi_agent::MultiAgentRuntimeBuilder;
use enki_next::workflow::{
    WorkflowEvent, WorkflowEventListener, WorkflowRequest, WorkflowResponse, WorkflowRuntime,
    WorkflowRuntimeBuilder, WorkflowStatus, WorkflowTaskRunner,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use toml::Value as TomlValue;

pub fn new(args: WorkflowNewArgs) -> Result<(), String> {
    if !args.manifest.exists() {
        return Err(format!(
            "Manifest file not found at {}.",
            args.manifest.display()
        ));
    }

    let manifest = Manifest::load(&args.manifest)?;
    let manifest_dir = args.manifest.parent().unwrap_or(Path::new("."));
    let workflow_name = normalize_name(&args.name);
    if workflow_name.is_empty() {
        return Err("Workflow name must contain letters or numbers.".to_string());
    }

    let workflow_id = args.id.unwrap_or_else(|| to_kebab_case(&workflow_name));
    if workflow_id.is_empty() {
        return Err("Workflow id must contain letters or numbers.".to_string());
    }
    if manifest
        .workflows
        .iter()
        .any(|workflow| workflow.id == workflow_id)
    {
        return Err(format!(
            "Workflow id '{}' already exists in the configured manifest set.",
            workflow_id
        ));
    }

    let workflow_file = args
        .file
        .unwrap_or_else(|| PathBuf::from("workflows").join(format!("{}.toml", workflow_id)));
    let workflow_file_key = to_posix_path(&workflow_file);
    if manifest
        .workflow_files
        .iter()
        .any(|entry| entry == &workflow_file_key)
        || manifest
            .project
            .workflow_files
            .iter()
            .any(|entry| entry == &workflow_file_key)
    {
        return Err(format!(
            "Workflow file '{}' is already registered in {}.",
            workflow_file_key,
            args.manifest.display()
        ));
    }

    let absolute_workflow_path = manifest_dir.join(&workflow_file);
    if absolute_workflow_path.exists() {
        return Err(format!(
            "Workflow file '{}' already exists.",
            absolute_workflow_path.display()
        ));
    }

    let task_target = resolve_task_target(&manifest, args.agent.as_deref(), &args.capabilities)?;
    let task_id = format!("{}-task", workflow_id);
    if manifest.tasks.iter().any(|task| task.id == task_id) {
        return Err(format!(
            "Generated task id '{}' already exists in the configured manifest set. Use --id to choose a different workflow id.",
            task_id
        ));
    }

    write_workflow_file(
        &absolute_workflow_path,
        &workflow_name,
        &workflow_id,
        &task_id,
        &task_target,
    )?;
    update_manifest_with_workflow_file(&args.manifest, &workflow_file_key)?;

    println!();
    println!("\x1b[1;32mWorkflow created!\x1b[0m");
    println!();
    println!("  \x1b[2mWorkflow:\x1b[0m");
    println!("    name = \"{}\"", workflow_name);
    println!("    id = \"{}\"", workflow_id);
    println!();
    println!("  \x1b[2mFiles:\x1b[0m");
    println!("    {}", workflow_file.display());
    println!("    {}", args.manifest.display());
    println!();
    println!("  \x1b[2mStarter target:\x1b[0m");
    match task_target {
        TaskTargetSpec::Agent(agent_id) => println!("    agent = \"{}\"", agent_id),
        TaskTargetSpec::Capabilities(capabilities) => {
            println!("    capabilities = {:?}", capabilities)
        }
    }
    println!();
    println!("  \x1b[2mNext:\x1b[0m");
    println!(
        "    enki workflow run --manifest {} --workflow {} --input '{{\"message\":\"describe the task\"}}'",
        args.manifest.display(),
        workflow_id
    );
    println!();

    Ok(())
}

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
    let runtime = build_workflow_runtime(&args.manifest, &manifest, true).await?;
    let input = parse_input(&args.input)?;
    let response = runtime
        .start(WorkflowRequest::new(args.workflow, input))
        .await?;
    print_response(&response, true);
    Ok(())
}

pub async fn inspect(args: WorkflowInspectArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest, false).await?;
    let state = runtime.inspect(&args.run).await?;
    let raw = serde_json::to_string_pretty(&state)
        .map_err(|e| format!("Failed to render workflow state: {e}"))?;
    println!("{}", raw);
    Ok(())
}

pub async fn resume(args: WorkflowResumeArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest, true).await?;
    let response = runtime.resume(&args.run).await?;
    print_response(&response, true);
    Ok(())
}

pub async fn join(args: WorkflowJoinArgs) -> Result<(), String> {
    let manifest = Manifest::load(&args.manifest)?;
    let runtime = build_workflow_runtime(&args.manifest, &manifest, true).await?;
    let stdin = std::io::stdin();
    let mut input = String::new();

    loop {
        let pending = runtime.list_pending_interventions(&args.run).await?;
        if pending.is_empty() {
            let response = runtime.resume(&args.run).await?;
            print_response(&response, true);
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

#[derive(Clone, Debug)]
enum TaskTargetSpec {
    Agent(String),
    Capabilities(Vec<String>),
}

struct CliWorkflowEventListener {
    header_printed: Mutex<bool>,
    event_index: Mutex<usize>,
}

impl CliWorkflowEventListener {
    fn new() -> Self {
        Self {
            header_printed: Mutex::new(false),
            event_index: Mutex::new(0),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl WorkflowEventListener for CliWorkflowEventListener {
    async fn on_event(&self, event: &WorkflowEvent) -> Result<(), String> {
        let mut header_printed = self
            .header_printed
            .lock()
            .map_err(|_| "Workflow event listener header lock was poisoned.".to_string())?;
        if !*header_printed {
            match event {
                WorkflowEvent::WorkflowStarted {
                    workflow_id,
                    run_id,
                } => {
                    println!("\x1b[1;36mWorkflow run\x1b[0m {}", run_id);
                    println!("  Workflow: {}", workflow_id);
                    println!("  Status: Running");
                    println!();
                }
                _ => {
                    println!("\x1b[1;36mWorkflow run\x1b[0m");
                    println!();
                }
            }
            println!("\x1b[1;36mLive execution\x1b[0m");
            *header_printed = true;
        }
        drop(header_printed);

        let mut event_index = self
            .event_index
            .lock()
            .map_err(|_| "Workflow event listener index lock was poisoned.".to_string())?;
        *event_index += 1;
        println!("  {}. {}", *event_index, render_event_line(event));
        Ok(())
    }
}

fn resolve_task_target(
    manifest: &Manifest,
    requested_agent: Option<&str>,
    requested_capabilities: &[String],
) -> Result<TaskTargetSpec, String> {
    if let Some(agent_id) = requested_agent {
        if manifest.agents.iter().any(|agent| agent.id == agent_id) {
            return Ok(TaskTargetSpec::Agent(agent_id.to_string()));
        }
        return Err(format!("Agent '{}' was not found in enki.toml.", agent_id));
    }

    if !requested_capabilities.is_empty() {
        return Ok(TaskTargetSpec::Capabilities(
            requested_capabilities.to_vec(),
        ));
    }

    if manifest.agents.len() == 1 {
        return Ok(TaskTargetSpec::Agent(manifest.agents[0].id.clone()));
    }

    Err("Choose a starter workflow target with --agent <id> or --capability <name>.".to_string())
}

fn write_workflow_file(
    path: &Path,
    workflow_name: &str,
    workflow_id: &str,
    task_id: &str,
    task_target: &TaskTargetSpec,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
    }

    let body = render_workflow_file(workflow_name, workflow_id, task_id, task_target);
    fs::write(path, body).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn render_workflow_file(
    workflow_name: &str,
    workflow_id: &str,
    task_id: &str,
    task_target: &TaskTargetSpec,
) -> String {
    let target_lines = match task_target {
        TaskTargetSpec::Agent(agent_id) => format!("agent = \"{}\"", agent_id),
        TaskTargetSpec::Capabilities(capabilities) => format!(
            "capabilities = [{}]",
            capabilities
                .iter()
                .map(|capability| format!("\"{}\"", capability))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };

    format!(
        "[[task]]\nid = \"{task_id}\"\n{target_lines}\nprompt = \"Complete the workflow task for {{{{input.message}}}}.\"\noutput_key = \"result\"\n\n[[workflow]]\nid = \"{workflow_id}\"\nname = \"{workflow_name}\"\nfailure_policy = \"continue_best_effort\"\n\n[[workflow.node]]\nid = \"run\"\nkind = \"task\"\ntask = \"{task_id}\"\n",
        task_id = task_id,
        target_lines = target_lines,
        workflow_id = workflow_id,
        workflow_name = workflow_name,
    )
}

fn update_manifest_with_workflow_file(
    manifest_path: &Path,
    workflow_file: &str,
) -> Result<(), String> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read {}: {e}", manifest_path.display()))?;
    let mut document: TomlValue = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {e}", manifest_path.display()))?;

    let root = document.as_table_mut().ok_or_else(|| {
        format!(
            "{} must contain a TOML table at the root.",
            manifest_path.display()
        )
    })?;

    let project = root
        .entry("project")
        .or_insert_with(|| TomlValue::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| "Expected [project] to be a TOML table.".to_string())?;

    let workflow_files = project
        .entry("workflow_files")
        .or_insert_with(|| TomlValue::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "Expected [project].workflow_files to be an array.".to_string())?;

    if !workflow_files
        .iter()
        .any(|value| value.as_str() == Some(workflow_file))
    {
        workflow_files.push(TomlValue::String(workflow_file.to_string()));
    }

    let rendered = toml::to_string_pretty(&document)
        .map_err(|e| format!("Failed to serialize {}: {e}", manifest_path.display()))?;
    fs::write(manifest_path, rendered)
        .map_err(|e| format!("Failed to write {}: {e}", manifest_path.display()))
}

fn validate_workflow_runtime_compatibility(manifest: &Manifest) -> Result<(), String> {
    let scripted_agents = manifest
        .agents
        .iter()
        .filter(|agent| {
            agent
                .script
                .as_deref()
                .is_some_and(|script| !script.trim().is_empty())
        })
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();
    if !scripted_agents.is_empty() {
        return Err(format!(
            "Workflow CLI v1 does not execute Python-scripted agents yet. Remove `script` from agent(s): {}",
            scripted_agents.join(", ")
        ));
    }

    let tool_agents = manifest
        .agents
        .iter()
        .filter(|agent| !manifest.resolve_tools(agent).is_empty())
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();
    if !tool_agents.is_empty() {
        return Err(format!(
            "Workflow CLI v1 does not execute Python tool-backed agents yet. Remove `tools` from agent(s): {}",
            tool_agents.join(", ")
        ));
    }

    Ok(())
}

async fn build_workflow_runtime(
    manifest_path: &Path,
    manifest: &Manifest,
    live_output: bool,
) -> Result<WorkflowRuntime, String> {
    validate_workflow_runtime_compatibility(manifest)?;

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
    if live_output {
        builder = builder.with_event_listener(Arc::new(CliWorkflowEventListener::new()));
    }

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

fn print_response(response: &WorkflowResponse, streamed_live: bool) {
    if streamed_live {
        println!();
        println!("\x1b[1;36mFinal status\x1b[0m");
        println!("  Run: {}", response.run_id);
        println!("  Workflow: {}", response.workflow_id);
        println!("  Status: {:?}", response.status);
        println!();
    } else {
        println!("\x1b[1;36mWorkflow run\x1b[0m {}", response.run_id);
        println!("  Workflow: {}", response.workflow_id);
        println!("  Status: {:?}", response.status);
        println!();

        if !response.events.is_empty() {
            println!("\x1b[1;36mExecution log\x1b[0m");
            for (index, event) in response.events.iter().enumerate() {
                println!("  {}. {}", index + 1, render_event_line(event));
            }
            println!();
        }
    }

    let context_value = response.context.to_value();

    println!("\x1b[1;36mSummary of work\x1b[0m");
    for line in render_work_summary(&context_value) {
        println!("  {}", line);
    }
    println!();

    let detailed_outputs = render_detailed_outputs(&context_value);
    if !detailed_outputs.is_empty() {
        println!("\x1b[1;36mDetailed outputs\x1b[0m");
        for (key, detail) in detailed_outputs {
            println!("  {}:", key);
            for line in indent_block(&detail, 4) {
                println!("{}", line);
            }
            println!();
        }
    }
}

fn render_event_line(event: &WorkflowEvent) -> String {
    match event {
        WorkflowEvent::WorkflowStarted { workflow_id, .. } => {
            format!("Workflow '{}' started", workflow_id)
        }
        WorkflowEvent::NodeReady { node_id } => format!("Node '{}' became ready", node_id),
        WorkflowEvent::NodeStarted { node_id, attempt } => {
            format!("Node '{}' started (attempt {})", node_id, attempt)
        }
        WorkflowEvent::NodeCompleted {
            node_id,
            output_key,
        } => format!("Node '{}' completed and wrote '{}'.", node_id, output_key),
        WorkflowEvent::NodeFailed { node_id, error } => {
            format!("Node '{}' failed: {}", node_id, summarize_text(error, 120))
        }
        WorkflowEvent::NodeRetryScheduled {
            node_id,
            attempt,
            error,
        } => format!(
            "Node '{}' scheduled retry after attempt {}: {}",
            node_id,
            attempt,
            summarize_text(error, 120)
        ),
        WorkflowEvent::NodeSkipped { node_id } => format!("Node '{}' was skipped", node_id),
        WorkflowEvent::InterventionRequested {
            intervention_id,
            node_id,
            reason,
        } => format!(
            "Intervention '{}' requested for node '{}': {}",
            intervention_id,
            node_id,
            summarize_text(reason, 120)
        ),
        WorkflowEvent::InterventionResolved {
            intervention_id,
            node_id,
        } => format!(
            "Intervention '{}' resolved for node '{}'",
            intervention_id, node_id
        ),
        WorkflowEvent::WorkflowPaused { reason, .. } => {
            format!("Workflow paused: {}", summarize_text(reason, 120))
        }
        WorkflowEvent::WorkflowCompleted { status, .. } => {
            format!("Workflow finished with status {:?}", status)
        }
    }
}

fn render_work_summary(context: &Value) -> Vec<String> {
    let Some(map) = context.as_object() else {
        return vec![format!("Context: {}", summarize_value(context))];
    };

    let mut lines = Vec::new();
    if let Some(input) = map.get("input") {
        lines.push(format!("Input: {}", summarize_value(input)));
    }

    let outputs = map
        .iter()
        .filter(|(key, _)| key.as_str() != "input")
        .map(|(key, value)| format!("{}: {}", key, summarize_value(value)))
        .collect::<Vec<_>>();

    if outputs.is_empty() {
        lines.push("No node outputs yet.".to_string());
    } else {
        lines.push("Outputs:".to_string());
        lines.extend(outputs.into_iter().map(|line| format!("- {}", line)));
    }

    lines
}

fn render_detailed_outputs(context: &Value) -> Vec<(String, String)> {
    let Some(map) = context.as_object() else {
        return Vec::new();
    };

    map.iter()
        .filter(|(key, _)| key.as_str() != "input")
        .map(|(key, value)| (key.clone(), detailed_value(value)))
        .collect()
}

fn detailed_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => {
            if text.trim().is_empty() {
                "<empty>".to_string()
            } else {
                text.trim().to_string()
            }
        }
        Value::Array(_) => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        Value::Object(map) => {
            if let Some(content) = map.get("content").and_then(Value::as_str) {
                if !content.trim().is_empty() {
                    return content.trim().to_string();
                }
            }
            if let Some(response) = map.get("response").and_then(Value::as_str) {
                if !response.trim().is_empty() {
                    return response.trim().to_string();
                }
            }
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
    }
}

fn indent_block(text: &str, spaces: usize) -> Vec<String> {
    let prefix = " ".repeat(spaces);
    let normalized = if text.is_empty() {
        "<empty>".to_string()
    } else {
        text.replace("\r\n", "\n")
    };

    normalized
        .lines()
        .map(|line| format!("{}{}", prefix, line))
        .collect()
}

fn summarize_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => summarize_text(text, 160),
        Value::Array(values) => {
            let preview = values
                .iter()
                .take(4)
                .map(summarize_value)
                .collect::<Vec<_>>()
                .join(", ");
            if values.len() > 4 {
                format!("[{}, ...]", preview)
            } else {
                format!("[{}]", preview)
            }
        }
        Value::Object(map) => {
            if let Some(content) = map.get("content").and_then(Value::as_str) {
                return summarize_text(content, 160);
            }
            if let Some(response) = map.get("response").and_then(Value::as_str) {
                return summarize_text(response, 160);
            }
            if let Some(joined) = map.get("joined") {
                return format!("joined {}", summarize_value(joined));
            }
            if let Some(matched) = map.get("matched") {
                return format!("matched={}", summarize_value(matched));
            }
            let keys = map.keys().take(4).cloned().collect::<Vec<_>>().join(", ");
            if map.len() > 4 {
                format!("object {{{}, ...}}", keys)
            } else if keys.is_empty() {
                "object {}".to_string()
            } else {
                format!("object {{{}}}", keys)
            }
        }
    }
}

fn summarize_text(text: &str, max_len: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_len {
        compact
    } else {
        let truncated = compact
            .chars()
            .take(max_len.saturating_sub(3))
            .collect::<String>();
        format!("{}...", truncated)
    }
}

fn normalize_name(value: &str) -> String {
    value.trim().to_string()
}

fn to_kebab_case(value: &str) -> String {
    split_words(value).join("-")
}

fn split_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn to_posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::WorkflowNewArgs;

    fn temp_manifest_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}-{}", label, unique))
    }

    #[test]
    fn allows_python_project_layout_when_agents_are_rust_managed() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "demo"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
"#,
        )
        .unwrap();

        validate_workflow_runtime_compatibility(&manifest).unwrap();
    }

    #[test]
    fn rejects_python_scripted_agents_for_workflow_cli() {
        let manifest: Manifest = toml::from_str(
            r#"[project]
name = "demo"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
script = "src/agents/assistant.py"
"#,
        )
        .unwrap();

        let error = validate_workflow_runtime_compatibility(&manifest).unwrap_err();
        assert!(error.contains("Python-scripted agents"));
    }

    #[test]
    fn renders_work_summary_from_context() {
        let summary = render_work_summary(&json!({
            "input": { "topic": "demo" },
            "result": { "content": "Completed the requested workflow task successfully." },
            "decision": { "matched": true }
        }));

        assert!(summary.iter().any(|line| line.contains("Input:")));
        assert!(summary.iter().any(|line| line.contains("Outputs:")));
        assert!(summary.iter().any(|line| {
            line.contains("result: Completed the requested workflow task successfully.")
        }));
        assert!(
            summary
                .iter()
                .any(|line| line.contains("decision: matched=true"))
        );
    }

    #[test]
    fn renders_detailed_outputs_without_truncating_content() {
        let detailed = render_detailed_outputs(&json!({
            "input": { "topic": "demo" },
            "result": {
                "content": "Line one of the workflow output.\nLine two keeps going with more detail that should remain visible in full."
            }
        }));

        assert_eq!(detailed.len(), 1);
        assert_eq!(detailed[0].0, "result");
        assert!(detailed[0].1.contains("Line one of the workflow output."));
        assert!(
            detailed[0]
                .1
                .contains("Line two keeps going with more detail")
        );
    }

    #[test]
    fn renders_event_lines() {
        let line = render_event_line(&WorkflowEvent::NodeCompleted {
            node_id: "review".to_string(),
            output_key: "summary".to_string(),
        });
        assert!(line.contains("review"));
        assert!(line.contains("summary"));
    }

    #[test]
    fn creates_workflow_file_and_updates_manifest() {
        let dir = temp_manifest_dir("enki-workflow-new");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest_path = dir.join("enki.toml");
        fs::write(
            &manifest_path,
            r#"[project]
name = "demo"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
"#,
        )
        .unwrap();

        new(WorkflowNewArgs {
            manifest: manifest_path.clone(),
            name: "Release Review".to_string(),
            id: None,
            file: None,
            agent: None,
            capabilities: Vec::new(),
        })
        .unwrap();

        let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
        assert!(updated_manifest.contains("workflow_files = [\"workflows/release-review.toml\"]"));

        let workflow_file = dir.join("workflows").join("release-review.toml");
        let workflow_content = fs::read_to_string(&workflow_file).unwrap();
        assert!(workflow_content.contains("id = \"release-review\""));
        assert!(workflow_content.contains("agent = \"assistant\""));
        assert!(workflow_content.contains("task = \"release-review-task\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn requires_target_when_manifest_has_multiple_agents() {
        let dir = temp_manifest_dir("enki-workflow-new-multi");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest_path = dir.join("enki.toml");
        fs::write(
            &manifest_path,
            r#"[project]
name = "demo"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"

[[agent]]
id = "reviewer"
name = "Reviewer"
model = "ollama::qwen3.5"
"#,
        )
        .unwrap();

        let error = new(WorkflowNewArgs {
            manifest: manifest_path,
            name: "Release Review".to_string(),
            id: None,
            file: None,
            agent: None,
            capabilities: Vec::new(),
        })
        .unwrap_err();

        assert!(error.contains("--agent <id> or --capability <name>"));

        let _ = fs::remove_dir_all(&dir);
    }
}
