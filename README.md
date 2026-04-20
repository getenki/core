<p align="center">
  <img src="https://docs.getenki.com/img/logo-dark.png" alt="Enki logo" width="160">
</p>

# Enki

Async-first multi-agent framework built on Rust and Tokio.

This repository contains the current `core-next` workspace for Enki's Rust runtime, the `enki-py` Python bindings, the `@getenki/ai` Node.js bindings, and the `enki` builder CLI.

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
|       |-- enki-js/
|       `-- enki-py/
|-- docs/
`-- test/
```

The workspace currently contains:

- `crates/core`: Rust agent runtime, memory system, tool execution, LLM provider abstraction, and CLI entrypoint
- `crates/builder`: manifest-driven CLI for scaffolding, running, testing, monitoring, and interactive sessions
- `crates/bindings/enki-js`: native Node.js bindings built with `napi-rs`
- `crates/bindings/enki-py`: UniFFI-based Python bindings and higher-level Python package packaging
- `docs/enki-doc`: the docs site source used to publish `docs.getenki.com`

## What This Repo Builds

- A stateful agent runtime with persistent sessions and workspace-backed execution
- A workflow runtime with DAG execution, persisted run state, resume support, and intervention handling
- Built-in tools for `read_file`, `write_file`, and `exec`
- Reusable tool registries that can be prepared once and connected to agents dynamically
- Human-in-the-loop runtime support through the intrinsic `ask_human` tool
- Execution tracing through per-step `ExecutionStep` events
- Multi-provider LLM support via the `provider::model` format
- Python bindings exposing low-level FFI types plus the high-level `Agent` wrapper, `ToolRegistry`, `MultiAgentRuntime`, and `EnkiWorkflowRuntime`
- JavaScript bindings exposing `NativeEnkiAgent`, `NativeToolRegistry`, `NativeMultiAgentRuntime`, `NativeWorkflowRuntime`, and traced run results

Examples of supported model strings in the current codebase:

- `ollama::qwen3.5`
- `openai::gpt-4o`
- `anthropic::claude-3-opus-20240229`
- `google::gemini-pro`

## Install

For Rust consumers, depend on the published library package and import it as `core_next`:

```toml
[dependencies]
core_next = { package = "core", version = "0.5.61" }
```

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

### JavaScript bindings

From `crates/bindings/enki-js`:

```powershell
npm install
npm run build
```

### Builder CLI

From the repository root:

```powershell
cargo build -p builder
cargo run -p builder -- --help
```

### Docs site

Run the site from `docs/enki-doc` with Node.js 18+:

```powershell
npm install
npm start
```

To produce the static site:

```powershell
npm run build
```

## Run

The low-level Rust `core` binary expects:

```text
core <session_id> "<message>"
```

Example:

```powershell
$env:ENKI_MODEL="ollama::qwen3.5"
cargo run -p core -- session-1 "Summarize the repository structure"
```

If you do not inject an LLM in code, the runtime resolves the model from `ENKI_MODEL`.

For manifest-driven app workflows, prefer the `enki` builder CLI:

```powershell
cargo run -p builder -- run --message "Summarize the repository structure"
cargo run -p builder -- join
```

## Examples

The repository includes runnable examples under `example/`:

- `example/basic-js/index.js`: basic JavaScript multi-agent runtime example
- `example/basic-js/tool-registry.js`: JavaScript example showing a reusable `NativeToolRegistry` connected to an agent dynamically
- `example/basic-js/custom-agent-loop.js`: JavaScript single-agent example overriding the default agentic loop in JavaScript
- `example/basic-js/react-custom-agent-loop.js`: JavaScript single-agent example running a ReAct loop with direct LLM calls
- `example/basic-js/multi-agent-tools-memory.js`: JavaScript example with researcher/coordinator agents, tool calling, and shared memory
- `example/basic-ts/index.ts`: basic TypeScript multi-agent runtime example
- `example/basic-ts/agent-workflow.ts`: TypeScript workflow runtime example using `NativeEnkiAgent` and `NativeWorkflowRuntime`
- `example/basic-ts/human-intervention-workflow.ts`: TypeScript workflow example showing `human_gate` pauses and failure escalation interventions
- `example/basic-ts/multi-agent-tools-memory.ts`: TypeScript example with researcher/coordinator agents, tool calling, and shared memory
- `example/enki-py/simple_agent.py`: basic Python single-agent example
- `example/enki-py/tool_registry.py`: Python example showing a reusable `ToolRegistry` connected to an agent dynamically
- `example/enki-py/custom_agentic_loop.py`: Python single-agent example overriding the default agentic loop in Python
- `example/enki-py/react_custom_agentic_loop.py`: Python single-agent example running a ReAct loop with direct LLM calls
- `example/enki-py/compare_agent_loops.py`: Python comparison example running the same question through default, prompt-customized, planner, and ReAct loops
- `example/enki-py/simple_multi_agent.py`: basic Python multi-agent example
- `example/enki-py/multi_agent_with_memory_and_tools.py`: Python multi-agent example with tools and shared memory
- `example/enki-py/human_intervention_workflow.py`: Python workflow example showing `human_gate` pauses and failure escalation interventions

Run the Node examples from their example directories:

```powershell
cd example/basic-js
npm install
npm start
npm run start:tool-registry
npm run start:custom-agent-loop
npm run start:react-custom-agent-loop
npm run start:multi-agent-tools-memory
```

```powershell
cd example/basic-ts
npm install
npm start
npm run start:agent-workflow
npm run start:human-intervention-workflow
npm run start:multi-agent-tools-memory
```

These examples default to `ollama::qwen3.5:latest` unless `ENKI_MODEL` is set.

## Custom Agentic Loops

Enki supports two customization levels for agent loops:

- prompt-level customization that replaces the default loop instructions while keeping the built-in Rust runtime loop
- full loop overrides where Python or JavaScript owns the turn-by-turn loop and returns execution steps directly

JavaScript examples:

- `example/basic-js/custom-agent-loop.js`: uses `agent.setAgentLoopHandler(...)` to return a final response directly from JavaScript
- `example/basic-js/react-custom-agent-loop.js`: uses `agent.setAgentLoopHandler(...)` to run a JavaScript-managed ReAct loop with direct LLM calls

Python examples:

- `example/enki-py/custom_agentic_loop.py`: uses `agent_loop_handler=` to return a final response directly from Python
- `example/enki-py/react_custom_agentic_loop.py`: uses `agent_loop_handler=` to run a Python-managed ReAct loop with direct LLM calls
- `example/enki-py/compare_agent_loops.py`: compares the default loop, prompt-customized loop, and Python-defined loop handlers side by side

If you only want to change the instructions the model sees, use:

- JavaScript: the optional `agenticLoop` constructor argument on `NativeEnkiAgent`
- Python: the optional `agentic_loop=` constructor argument on `Agent`

If you want your application code to control the loop itself, use:

- JavaScript: `agent.setAgentLoopHandler(...)`
- Python: `agent_loop_handler=` or `agent.set_agent_loop_handler(...)`

## Python API

The published docs describe two Python layers:

- a generated low-level API around `EnkiAgent`, `EnkiTool`, and `EnkiToolHandler`
- a generated workflow API around `EnkiWorkflowRuntime`, with the recommended Python path being `Agent(...).as_workflow_agent(...)`
- a higher-level Python wrapper for more ergonomic agent usage

This repo contains the low-level Rust-backed binding implementation in `crates/bindings/enki-py`.

## JavaScript API

The Node.js binding in `crates/bindings/enki-js` exposes:

- `NativeEnkiAgent`
- `NativeToolRegistry`
- `NativeMultiAgentRuntime`
- `runWithTrace()` and `processWithTrace()` for traced execution

The Python binding in `crates/bindings/enki-py` also exposes:

- `ToolRegistry`
- `Agent.connect_tool_registry(...)`
- `Agent(tool_registry=...)`

## Persistence

Agent state is stored under a per-agent workspace rooted at the configured workspace home. The runtime persists:

- session transcripts
- memory state
- current task workspaces

The `test/.atomiagent/...` fixtures show the expected on-disk layout.

## Notes

- The current workspace version is `0.5.61`.
- The Rust package name is `core`, and the exported library name is `core_next`.
- The docs site currently brands Enki publicly as in active development/private preview while the open-source core and `enki-py` docs are already published.

## License

This project is licensed under the terms of the [LICENSE](LICENSE) file.



