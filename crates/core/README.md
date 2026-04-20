# Enki Next

`enki-next` is Enki's Rust runtime library. It provides the shared agent, workflow, memory, registry, tooling, and LLM provider abstractions used by the higher-level SDKs and local CLI entrypoints.

The crate is packaged as `enki-next`, and the Rust import name is `enki_next`.

## Install

Within this workspace, the package is available locally as `enki-next`.

After publishing, add it to your Rust project with a rename so the import path stays explicit:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.76" }
```

If you want the bundled universal LLM provider used by the local CLI and examples, enable the feature explicitly:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.76", features = ["universal-llm-provider"] }
```

## Choose the right Rust entry point

Use these APIs depending on what you are building:

- `enki_next::agent::Agent`: the low-level single-agent type when you want direct control over the agent instance, tool executor, workspace, and loop.
- `enki_next::runtime::RuntimeBuilder`: the easiest place to start for a single-agent runtime with custom tools, a custom LLM provider, and a task workspace.
- `enki_next::runtime::MultiAgentRuntime`: multiple named agents sharing discovery and delegation tools.
- `enki_next::workflow::WorkflowRuntime`: DAG-style workflow orchestration with persisted run state, resume support, transforms, decisions, joins, and intervention handling.

## Main modules

- `enki_next::agent`
- `enki_next::llm`
- `enki_next::memory`
- `enki_next::message`
- `enki_next::registry`
- `enki_next::runtime`
- `enki_next::tooling`
- `enki_next::workflow`

## Top-level workflow exports

The crate root re-exports the most common workflow types:

- `TaskDefinition`
- `WorkflowDefinition`
- `WorkflowRequest`
- `WorkflowRunState`
- `WorkflowRuntime`
- `WorkflowTaskRunner`

## Single-agent runtime example

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

When you do not inject your own provider with `with_llm(...)`, the crate resolves the model from `AgentDefinition.model` or `ENKI_MODEL`. The built-in universal provider is available only when the `universal-llm-provider` feature is enabled.

## Runnable crate examples

The crate ships runnable examples under `crates/core/examples`:

- `cargo run -p enki-next --example simple_agent -- "Summarize this repository"`: low-level `Agent` setup with a task workspace.
- `cargo run -p enki-next --example runtime_builder`: `RuntimeBuilder` plus a custom Rust tool and a mock LLM provider.
- `cargo run -p enki-next --example multi_agent -- "Summarize the repository structure"`: two-agent runtime with discovery and delegation.
- `cargo run -p enki-next --example workflow`: persisted workflow runtime with reusable tasks, inline tasks, transforms, decisions, and joins.

The `simple_agent` and `multi_agent` examples use the built-in tool registry and expect either `AgentDefinition.model`, `ENKI_MODEL`, or an injected provider. The `runtime_builder` and `workflow` examples are self-contained and do not require a live model provider.

## Release posture

This workspace keeps the binary target for local development, but the reusable Rust surface lives in `crates/core`. The crate README and docs are intentionally focused on the API that external users can depend on once the package is published.
