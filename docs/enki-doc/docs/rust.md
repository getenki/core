---
sidebar_position: 1
slug: /rust
---

# Rust

The Rust workspace contains Enki's core runtime and the language bindings built on top of it.

## Workspace layout

The main crates in this repository are:

- `crates/core`: Rust agent runtime, memory system, tool execution, LLM provider abstraction, and CLI entrypoint
- `crates/bindings/enki-py`: UniFFI-based Python bindings
- `crates/bindings/enki-js`: `wasm-bindgen` JavaScript bindings

## What the runtime provides

The Rust core is responsible for:

- Agent execution and iteration control
- Session and workspace state management
- Memory handling
- Tool execution
- Provider/model resolution using the `provider::model` format

Examples of model strings used in this workspace:

- `ollama::qwen3.5`
- `openai::gpt-4o`
- `anthropic::claude-3-opus-20240229`
- `google::gemini-pro`

## Build the workspace

From the repository root:

```bash
cargo build
cargo test
```

## Run the CLI

The current Rust CLI expects:

```text
core <session_id> "<message>"
```

Example:

```bash
cargo run -p core -- session-1 "Summarize the repository structure"
```

If you do not inject an LLM in code, the runtime resolves the model from `ENKI_MODEL`.

## Rust docs

- [Build from Source](/docs/build-from-source)
