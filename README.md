<p align="center">
  <img src="https://docs.getenki.com/img/logo-dark.png" alt="Enki logo" width="160">
</p>

# Enki

Async-first multi-agent framework built on Rust and Tokio.

This repository contains the current `core-next` workspace for Enki's Rust runtime plus the `enki-py` Python bindings.

## Docs

- Product docs: <https://docs.getenki.com>
- Getting started: <https://docs.getenki.com/docs/intro>
- Installation: <https://docs.getenki.com/docs/installation>
- Build from source: <https://docs.getenki.com/docs/build-from-source>

## Workspace

```text
.
|-- Cargo.toml
|-- crates/
|   |-- core/
|   `-- bindings/
|       `-- enki-py/
|-- docs/
`-- test/
```

The workspace currently contains:

- `crates/core`: Rust agent runtime, memory system, tool execution, LLM provider abstraction, and CLI entrypoint
- `crates/bindings/enki-py`: UniFFI-based Python bindings and higher-level Python package packaging
- `docs/enki-py`: the docs site source used to publish `docs.getenki.com`

## What This Repo Builds

- A stateful agent runtime with persistent sessions and workspace-backed execution
- Built-in tools for `read_file`, `write_file`, and `exec`
- Multi-provider LLM support via the `provider::model` format
- Python bindings exposing `EnkiAgent`, `EnkiTool`, and `EnkiToolHandler`

Examples of supported model strings in the current codebase:

- `ollama::qwen3.5`
- `openai::gpt-4o`
- `anthropic::claude-3-opus-20240229`
- `google::gemini-pro`

## Install

For users of the published Python package, the docs currently recommend:

```bash
pip install enki-py
```

Or with `uv`:

```bash
uv add enki-py
```

## Build

### Rust workspace

```powershell
cargo build
cargo test
```

### Python bindings

From `crates/bindings/enki-py`:

```powershell
pip install maturin
maturin develop
```

### Docs site

The published docs' build instructions currently say to run the site from `docs/enki-py` with Node.js 18+:

```powershell
npm install
npm start
```

To produce the static site:

```powershell
npm run build
```

## Run

The current Rust CLI expects:

```text
core <session_id> "<message>"
```

Example:

```powershell
$env:ENKI_MODEL="ollama::qwen3.5"
cargo run -p core -- session-1 "Summarize the repository structure"
```

If you do not inject an LLM in code, the runtime resolves the model from `ENKI_MODEL`.

## Python API

The published docs describe two Python layers:

- a generated low-level API around `EnkiAgent`, `EnkiTool`, and `EnkiToolHandler`
- a higher-level Python wrapper for more ergonomic agent usage

This repo contains the low-level Rust-backed binding implementation in `crates/bindings/enki-py`.

## Persistence

Agent state is stored under a per-agent workspace rooted at the configured workspace home. The runtime persists:

- session transcripts
- memory state
- current task workspaces

The `test/.atomiagent/...` fixtures show the expected on-disk layout.

## Notes

- The current workspace version is `0.1.2`.
- The Rust package name is `core`, and the exported library name is `core_next`.
- The docs site currently brands Enki publicly as in active development/private preview while the open-source core and `enki-py` docs are already published.

## License

This project is licensed under the terms of the [LICENSE](LICENSE) file.
