---
sidebar_position: 1
slug: /rust
---

# Rust

The Rust workspace contains Enki's core runtime and the language bindings built on top of it. If you are consuming Enki from Rust, the crate you use is `enki-next`, imported as `enki_next`.

## Rust package

The Rust crate in this workspace is packaged as `enki-next` and imported in Rust as `enki_next`.

After publishing, consumers can depend on it like this:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.77" }
```

If you want the bundled universal LLM provider, enable the feature explicitly:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.77", features = ["universal-llm-provider"] }
```

## Choose the Rust API

These are the main public Rust entry points:

- `enki_next::agent::Agent`: low-level agent type when you want direct control over the agent instance, tool executor, workspace, and loop.
- `enki_next::runtime::RuntimeBuilder`: the easiest way to assemble a single-agent runtime with custom tools, memory, workspace, and an injected LLM provider.
- `enki_next::runtime::MultiAgentRuntime`: multi-agent orchestration with agent discovery and delegation.
- `enki_next::workflow::WorkflowRuntime`: persisted DAG-style workflows with reusable tasks, inline tasks, transforms, decisions, joins, resume, and intervention handling.

If you just want to get started from Rust code, start with `RuntimeBuilder` for single-agent execution or `WorkflowRuntime` for structured orchestration.

## Single-agent runtime

For application-owned Rust integrations, `RuntimeBuilder` is the cleanest single-agent entry point:

```rust
use enki_next::agent::AgentDefinition;
use enki_next::runtime::RuntimeBuilder;

let runtime = RuntimeBuilder::new(AgentDefinition {
    name: "Rust Assistant".to_string(),
    system_prompt_preamble: "You are a concise Rust assistant.".to_string(),
    model: "openai::gpt-4o-mini".to_string(),
    max_iterations: 8,
})
.with_workspace_home("./.enki")
.build()
.await?;
```

`RuntimeBuilder` lets you:

- set the model with `AgentDefinition.model` or `.with_model(...)`
- inject a custom provider with `.with_llm(...)`
- add tools with `.register_tool(...)` or `.with_tool_registry(...)`
- override the task workspace with `.with_workspace_home(...)`
- add a custom memory manager with `.with_memory(...)`

The runtime automatically injects the intrinsic `ask_human` tool so agents can pause for human input when you use the human-aware runtime methods.

## Multi-agent runtime

Use `MultiAgentRuntime::builder()` when you want multiple named agents that can discover peers and delegate work:

```rust
use enki_next::agent::AgentDefinition;
use enki_next::runtime::MultiAgentRuntime;

let runtime = MultiAgentRuntime::builder()
    .add_agent(
        "coordinator",
        AgentDefinition {
            name: "Coordinator".to_string(),
            system_prompt_preamble: "Route research tasks to other agents.".to_string(),
            model: "openai::gpt-4o-mini".to_string(),
            max_iterations: 8,
        },
        vec!["planning".to_string()],
    )
    .add_agent(
        "researcher",
        AgentDefinition {
            name: "Researcher".to_string(),
            system_prompt_preamble: "Read files and summarize findings.".to_string(),
            model: "openai::gpt-4o-mini".to_string(),
            max_iterations: 8,
        },
        vec!["research".to_string()],
    )
    .with_workspace_home("./.enki")
    .build()
    .await?;
```

This runtime injects the `discover_agents` and `delegate_task` tools so agents can route work across the shared registry.

## Workspace layout

The main crates in this repository are:

- `crates/core`: Rust agent runtime, memory system, tool execution, LLM provider abstraction, and CLI entrypoint
- `crates/builder`: the `enki` CLI for manifest-driven projects and interactive sessions
- `crates/bindings/enki-py`: UniFFI-based Python bindings
- `crates/bindings/enki-js`: native Node.js bindings built with `napi-rs`

## What the runtime provides

The Rust core is responsible for:

- Agent execution and iteration control
- Session and workspace state management
- Memory handling
- Tool execution
- Workflow DAG execution with persisted run state, resume support, and intervention handling
- Human-in-the-loop support through the intrinsic `ask_human` tool
- Execution tracing via per-step `ExecutionStep` events
- Provider/model resolution using the `provider::model` format

Examples of model strings used in this workspace:

- `ollama::qwen3.5`
- `openai::gpt-4o`
- `anthropic::claude-3-opus-20240229`
- `google::gemini-3.1-pro-preview`

## Run the crate examples

`crates/core/examples` contains runnable Rust examples for the main crate-level APIs:

- `cargo run -p enki-next --example simple_agent -- "Summarize this repository"`: low-level `Agent` setup with a task workspace.
- `cargo run -p enki-next --example runtime_builder`: `RuntimeBuilder` plus a custom Rust tool and a mock LLM provider.
- `cargo run -p enki-next --example multi_agent -- "Summarize the repository structure"`: multi-agent runtime with discovery and delegation.
- `cargo run -p enki-next --example workflow`: persisted workflow runtime using a mock task runner instead of a live model provider.

The `simple_agent` and `multi_agent` examples expect either `ENKI_MODEL`, `AgentDefinition.model`, or an injected provider. The `runtime_builder` and `workflow` examples are self-contained and can be run without external model credentials.

## Build the workspace

From the repository root:

```bash
cargo build
cargo test
```

## Core binary

The low-level `core` binary expects:

```text
core <session_id> "<message>"
```

Example:

```bash
cargo run -p enki-next -- session-1 "Summarize the repository structure"
```

If you do not inject an LLM in code, the runtime resolves the model from `ENKI_MODEL`.

## Builder CLI

For local app-style workflows, use the `enki` builder crate instead of the low-level `core` binary.

Current commands include:

- `enki init`
- `enki build`
- `enki run`
- `enki test`
- `enki monitor`
- `enki join`
- `enki tool new`
- `enki agent add`

See [Builder CLI](/docs/builder-cli) for the manifest format and command flow.

## Rust docs

- [Rust Workflow](/docs/rust-workflow)
- [Examples](/docs/examples)
- [Builder CLI](/docs/builder-cli)
- [Build from Source](/docs/build-from-source)
