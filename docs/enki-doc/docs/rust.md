---
sidebar_position: 1
slug: /rust
---

# Rust

The Rust workspace contains Enki's core runtime and the language bindings built on top of it.

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
cargo run -p core -- session-1 "Summarize the repository structure"
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
- [Builder CLI](/docs/builder-cli)
- [Build from Source](/docs/build-from-source)