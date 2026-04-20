---
sidebar_position: 2
slug: /rust-workflow
---

# Rust Workflow

Rust exposes the workflow engine directly when you want to build workflow definitions in code instead of driving them through `enki.toml`.

Import the main types from `enki_next`:

- `WorkflowRuntime`
- `WorkflowRequest`
- `WorkflowRunState`
- `WorkflowDefinition`
- `TaskDefinition`
- `WorkflowTaskRunner`

Typical setup:

```rust
use enki_next::{
    TaskDefinition, WorkflowDefinition, WorkflowRequest, WorkflowRuntime, WorkflowTaskRunner,
};
use serde_json::json;

let runtime = WorkflowRuntime::builder()
    .with_workspace_home("./.enki")
    .with_task_runner(task_runner)
    .add_task(task)
    .add_workflow(workflow)
    .build()
    .await?;

let response = runtime
    .start(WorkflowRequest::new(
        "release-note-review",
        json!({ "topic": "runtime-managed workflows" }),
    ))
    .await?;
```

The runtime persists workflow state under the configured workspace home and supports:

- `list_workflows()` for registered definitions
- `list_runs()` for persisted runs
- `inspect(run_id)` for loading a saved run state
- `start(request)` for a new run
- `resume(run_id)` for paused or interrupted runs
- `submit_intervention(run_id, intervention_id, response)` for human-gate responses

For a complete runnable example with reusable tasks, inline tasks, transforms, decisions, joins, and persisted runs, see `cargo run -p enki-next --example workflow`.

If you want the same workflow concepts demonstrated from a separate consumer crate, run:

```powershell
cargo run --manifest-path example/enki-rs/Cargo.toml --bin workflow_detailed
```

That example shows:

- a detached Rust app crate depending on `enki-next`
- a reusable task plus an inline task
- a custom transform registered by the application
- a `human_gate` pause, intervention submission, and `resume(...)`
- persisted run inspection with `inspect(...)` and `list_runs()`
