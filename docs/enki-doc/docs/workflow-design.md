---
sidebar_position: 4
slug: /workflow-design
---

# Workflow Design

Enki's workflow model is built as a Rust-managed DAG runtime that sits beside the agent loop, not on top of prompt-only orchestration. The design goal is to make long-running multi-step work resumable, inspectable, and deterministic while still letting agents do the actual task execution.

## Design goals

- Keep workflow structure explicit instead of embedding control flow in prompts
- Separate orchestration state from agent execution state
- Make runs resumable through persisted state and event logs
- Support both direct agent targeting and capability-based routing
- Allow human gates, transforms, and branching without leaving the runtime
- Expose the same core workflow model to Rust, Python, and JavaScript

## Core architecture

The workflow system is split across three layers inside the Rust core:

- `types.rs`: serializable definitions for tasks, nodes, edges, run state, events, and interventions
- `runtime.rs`: the workflow builder, validation, run driver, node execution, branching, retries, and intervention handling
- `persistence.rs`: on-disk storage for workflow definitions, run state snapshots, interventions, task workspaces, and event logs

This keeps the workflow model declarative while centralizing execution semantics in one runtime.

## Workflow model

The runtime works with a few core concepts:

- `TaskDefinition`: a reusable task template with a target, prompt, bindings, transforms, retry policy, and failure policy
- `WorkflowDefinition`: a DAG of nodes and edges plus workflow-level retry and failure behavior
- `WorkflowNodeKind`: node behavior, currently `task`, `decision`, `human_gate`, `transform`, and `join`
- `WorkflowEdgeTransition`: edge activation rules, currently `always`, `on_success`, `on_failure`, and `condition`
- `WorkflowContext`: the shared structured value map that carries workflow input and node outputs forward

This model lets Enki keep orchestration logic in typed data instead of re-deriving the next step from a model response.

## Builder pattern

Workflows are assembled through `WorkflowRuntimeBuilder`:

- add reusable tasks
- add workflow definitions
- register transforms
- configure the workspace home
- inject a task runner
- optionally attach an event listener

The builder registers built-in transforms up front:

- `identity`
- `extract_content`

It also validates the full workflow set before the runtime becomes usable, which catches graph and reference issues early.

## Execution model

A workflow run starts from a `WorkflowRequest` with a `workflow_id` and structured input. The runtime:

- loads the workflow definition
- initializes a run id and persisted run directory
- seeds the workflow context with `input`
- creates `NodeRunState` entries for every node
- drives the DAG until the run is completed, paused, failed, or completed with failures

The run loop is stateful rather than prompt-driven. It resolves ready nodes, executes them, records outputs into context, evaluates outgoing edges, and advances downstream nodes based on transition rules.

## Task execution boundary

The workflow runtime does not execute agent prompts directly. Instead, it delegates task nodes through a `WorkflowTaskRunner`.

That separation is important:

- workflow orchestration owns graph control, retries, branching, and persistence
- the task runner owns how a task is actually carried out by an agent or runtime

In practice, Enki wires this to the multi-agent runtime, which means workflow tasks can target:

- a specific `agent_id`
- a set of `capabilities`

This lets workflows route work declaratively while still reusing the same agent runtime primitives.

## Context, bindings, and transforms

Workflows pass structured data forward through `WorkflowContext`.

The current implementation supports:

- `input_bindings` to map workflow input or prior outputs into task prompt variables
- `input_transform` to reshape task input before execution
- `output_transform` to normalize task output before it is stored
- node `output_key` values so downstream nodes can reference prior results

This pattern gives workflows dataflow semantics rather than forcing every step to parse raw model text.

## Failure and retry design

Both tasks and workflows can define:

- `RetryPolicy`
- `WorkflowFailurePolicy`

Current failure policies are:

- `continue_best_effort`
- `fail_workflow`
- `pause_for_intervention`

This lets the runtime distinguish between recoverable node failure, terminal workflow failure, and runs that should stop for human input before continuing.

## Human intervention

Human gates are first-class workflow nodes, and failures can also escalate into interventions.

The runtime records pending interventions with:

- workflow id
- run id
- node id
- prompt
- reason
- response
- resolution timestamps

This gives Enki a resumable human-in-the-loop mechanism without breaking the workflow run into ad hoc external state.

## Persistence model

Workflow state is stored under the workflow workspace rooted at:

- `<workspace_home>/.atomiagent/workflows`

Each run gets its own directory with persisted artifacts such as:

- `workflow.json` for the snapshot of the definition used for that run
- `state.json` for current run state
- `events.jsonl` for the append-only event log
- `interventions.json` for pending and resolved intervention records
- `tasks/<node_id>/` for per-node task workspace data

This storage model is what makes `inspect`, `list_runs`, and `resume` possible.

## Observability

The runtime emits structured workflow events such as:

- workflow started
- node ready
- node started
- node completed
- node failed
- retry scheduled
- node skipped
- intervention requested
- intervention resolved
- workflow paused
- workflow completed

These events are suitable for CLIs, SDK surfaces, and future monitoring UIs because they expose orchestration state directly rather than hiding it in free-form logs.

## Relationship to the Builder CLI

The `enki` builder CLI is the main manifest-driven interface for local workflow use, but it is a client of the same runtime design rather than a separate implementation.

That means:

- the CLI defines tasks and workflows in TOML
- the Rust runtime validates and executes the DAG
- persisted runs can be inspected and resumed through runtime-backed commands

The current CLI intentionally limits some features, but the underlying workflow runtime is broader and is also exposed through Rust, Python, and JavaScript bindings.

## Language binding strategy

Workflow bindings mirror the same Rust workflow engine instead of reimplementing orchestration per language.

Today that means:

- Rust exposes `WorkflowRuntime` directly
- Python exposes `EnkiWorkflowRuntime`
- JavaScript and TypeScript expose `NativeWorkflowRuntime`

All of them share the same runtime semantics for graph validation, persistence, branching, interventions, and resumed execution.

## Example workflow shape

A common pattern is:

- research node gathers structured notes
- transform node extracts the normalized content
- decision node checks whether the output is usable
- writer node produces a final summary
- join node merges converging paths

That structure is a good fit for workflows because orchestration decisions stay explicit in the graph while agents remain focused on individual tasks.

## Related docs

- [Agent Design](/docs/agent-design)
- [Builder CLI](/docs/builder-cli)
- [Rust Workflow](/docs/rust-workflow)
- [Python Workflow](/docs/python-workflow)
- [JavaScript Workflow](/docs/javascript-workflow)
- [TypeScript Workflow](/docs/typescript-workflow)