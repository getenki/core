---
sidebar_position: 2
slug: /builder-cli
---

# Builder CLI

The `enki` CLI in `crates/builder` is the local workflow layer for manifest-driven projects.

Use it when you want to:

- scaffold a new Enki project
- install project dependencies
- run or test agents defined in `enki.toml`
- inspect configured agents
- open an interactive REPL against an agent
- scaffold project-local Python tools and agent scripts

## Manifest shape

The CLI reads `enki.toml`.

Minimal example:

```toml
[project]
name = "demo"

[workspace]
home = "./.enki"

[[agent]]
id = "assistant"
name = "Assistant"
model = "ollama::qwen3.5"
system_prompt = "You are a helpful assistant."
max_iterations = 20
capabilities = ["general"]
```

Supported sections in the current implementation:

- `[project]`: `name`, optional `version`
- `[workspace]`: optional `home`
- `[[agent]]`: `id`, `name`, `model`, optional `system_prompt`, `max_iterations`, `capabilities`, `tools`, and `script`
- `[[tool]]`: project-local Python tools with `id`, `kind`, `path`, and `symbol`

## Main commands

Create a project:

```bash
enki init --name my-app --template py
```

Install dependencies for the detected project type:

```bash
enki build
```

Run one message through an agent:

```bash
enki run --agent assistant --message "Summarize the project."
```

Test configured agents with a short connectivity check:

```bash
enki test
```

Print configured agents and their models:

```bash
enki monitor
```

Open the interactive REPL:

```bash
enki join --agent assistant
```

## Interactive mode

`enki join` is the current human-in-the-loop entry point. It keeps session state, lets you talk to an agent repeatedly, and routes messages through the runtime instead of starting a new one-shot process every turn.

For Python projects, the CLI forwards requests into the Python runtime loader. For Rust-only manifests, it builds the multi-agent runtime directly in Rust.

## Tool and agent scaffolding

Create a project-local Python tool file and register it in `enki.toml`:

```bash
enki tool new --name weather --agent assistant
```

Add a new agent entry:

```bash
enki agent add reviewer
```

Generate a boilerplate Python agent script while adding it:

```bash
enki agent add reviewer --script
```

## Runtime behavior

The builder uses the workspace home from `enki.toml`, validates referenced Python tools before execution, and routes by project type:

- Node projects: installs with `npm install`
- Python projects: installs with `pip install -e .`
- Rust projects: builds with `cargo build`

`enki run` currently prints detailed execution steps for Rust-managed runs, which is useful when debugging loop phases and tool usage.
