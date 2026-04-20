# Enki Next

`enki-next` is Enki's Rust runtime library. It provides the shared agent, workflow, memory, registry, tooling, and LLM provider abstractions used by the higher-level SDKs and local CLI entrypoints.

The crate is published as package `enki-next`, and the Rust import name is `enki_next`.

## Install

Add the crate to your Rust project with a rename so the import path stays explicit:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.61" }
```

If you want the bundled universal LLM provider used by the local CLI and examples, enable the feature explicitly:

```toml
[dependencies]
enki_next = { package = "enki-next", version = "0.5.61", features = ["universal-llm-provider"] }
```

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

## Example

```rust
use enki_next::{
    TaskDefinition, WorkflowDefinition, WorkflowRequest, WorkflowRuntime, WorkflowTaskRunner,
};
```

## Release posture

This workspace keeps the binary target for local development, but the publishable surface is the Rust library in `crates/core`. The crate README and docs are intentionally focused on the reusable API that external users will depend on from crates.io.
