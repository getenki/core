# Enki Core

`core` is Enki's Rust runtime library. It provides the shared agent, workflow, memory, registry, tooling, and LLM provider abstractions used by the higher-level SDKs and local CLI entrypoints.

The crate is published as package `core`, but the Rust import name is `core_next`.

## Install

Add the crate to your Rust project with a rename so the import path stays explicit:

```toml
[dependencies]
core_next = { package = "core", version = "0.5.61" }
```

If you want the bundled universal LLM provider used by the local CLI and examples, enable the feature explicitly:

```toml
[dependencies]
core_next = { package = "core", version = "0.5.61", features = ["universal-llm-provider"] }
```

## Main modules

- `core_next::agent`
- `core_next::llm`
- `core_next::memory`
- `core_next::message`
- `core_next::registry`
- `core_next::runtime`
- `core_next::tooling`
- `core_next::workflow`

## Top-level workflow exports

The crate root re-exports the most common workflow types:

- `TaskDefinition`
- `WorkflowDefinition`
- `WorkflowRequest`
- `WorkflowRunState`
- `WorkflowRuntime`
- `WorkflowTaskRunner`

## Example

```rust
use core_next::{
    TaskDefinition, WorkflowDefinition, WorkflowRequest, WorkflowRuntime, WorkflowTaskRunner,
};
```

## Release posture

This workspace keeps the binary target for local development, but the publishable surface is the Rust library in `crates/core`. The crate README and docs are intentionally focused on the reusable API that external users will depend on from crates.io.
