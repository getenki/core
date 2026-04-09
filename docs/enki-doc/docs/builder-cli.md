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
- define and run runtime-managed workflows from the CLI

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

- `[project]`: `name`, optional `version`, optional `workflow_files`
- `[workspace]`: optional `home`
- `[[agent]]`: `id`, `name`, `model`, optional `system_prompt`, `max_iterations`, `capabilities`, `tools`, and `script`
- `[[tool]]`: project-local Python tools with `id`, `kind`, `path`, and `symbol`
- `[[transform]]`: workflow transforms
- `[[task]]`: reusable workflow task definitions
- `[[workflow]]`: workflow DAG definitions
- `[[workflow.node]]`: workflow nodes
- `[[workflow.edge]]`: workflow edges

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

## Workflow commands

Create a starter workflow TOML file and register it in `enki.toml`:

```bash
enki workflow new --manifest ./enki.toml --name "Release Note Review" --agent reviewer
```

You can also target capabilities instead of a specific agent:

```bash
enki workflow new --manifest ./enki.toml --name "Research Flow" --capability research
```

List workflows defined in the manifest:

```bash
enki workflow list --manifest ./enki.toml
```

Start a workflow run:

```bash
enki workflow run --manifest ./enki.toml --workflow release-note-review --input '{"topic":"runtime-managed workflows"}'
```

Inspect a persisted workflow run:

```bash
enki workflow inspect --manifest ./enki.toml --run <run-id>
```

Resume a paused or interrupted workflow run:

```bash
enki workflow resume --manifest ./enki.toml --run <run-id>
```

Join a paused workflow to answer intervention prompts:

```bash
enki workflow join --manifest ./enki.toml --run <run-id>
```

## Complete simple workflow example

This is a full minimal setup that uses a separate workflow TOML file with the CLI.

1. Create the starter workflow file:

```bash
enki workflow new --manifest ./enki.toml --name "Release Note Review" --agent reviewer
```

2. Main `enki.toml`:

```toml
[project]
name = "demo"
workflow_files = ["workflows/release-note-review.toml"]

[workspace]
home = "./.enki"

[[agent]]
id = "reviewer"
name = "Reviewer"
model = "ollama::qwen3.5"
system_prompt = "You review release note drafts and make them clearer and more concise."
capabilities = ["review"]
```

3. `workflows/release-note-review.toml`:

```toml
[[task]]
id = "release-note-review-task"
agent = "reviewer"
prompt = "Review this release note request and produce a concise final draft:\n{{input.message}}"
output_key = "result"

[[workflow]]
id = "release-note-review"
name = "Release Note Review"
failure_policy = "continue_best_effort"

[[workflow.node]]
id = "run"
kind = "task"
task = "release-note-review-task"
```

4. Run it:

```bash
enki workflow run --manifest ./enki.toml --workflow release-note-review --input '{"message":"Added workflow runtime persistence and resume support."}'
```

5. Inspect the persisted run later:

```bash
enki workflow inspect --manifest ./enki.toml --run <run-id>
```

The simple example above is intentionally one task and one workflow node. Once that works, expand it by adding more `[[workflow.node]]` and `[[workflow.edge]]` entries or by introducing reusable `[[task]]` definitions in other workflow TOML files.

## Split workflow TOMLs

You can keep agents in the main `enki.toml` and move workflow definitions into separate TOML files. `enki workflow new` generates this layout by default.

Recommended layout:

```text
.
|-- enki.toml
`-- workflows/
    |-- research.toml
    `-- release.toml
```

Main manifest:

```toml
[project]
name = "demo"
workflow_files = [
  "workflows/research.toml",
  "workflows/release.toml",
]

[workspace]
home = "./.enki"

[[agent]]
id = "writer"
name = "Writer"
model = "ollama::qwen3.5"
capabilities = ["writing"]

[[agent]]
id = "reviewer"
name = "Reviewer"
model = "ollama::qwen3.5"
capabilities = ["review"]
```

Included workflow file:

```toml
[[task]]
id = "draft_release_note"
capabilities = ["writing"]
prompt = "Draft a concise release note for {{input.topic}}."
output_key = "draft"

[[workflow]]
id = "release-note-review"
name = "Release Note Review"
failure_policy = "continue_best_effort"

[[workflow.node]]
id = "draft"
kind = "task"
task = "draft_release_note"

[[workflow.node]]
id = "review"
kind = "task"
agent = "reviewer"
prompt = "Review this draft:\n{{context.draft.content}}"
output_key = "review"

[[workflow.node]]
id = "done"
kind = "join"

[[workflow.edge]]
from = "draft"
to = "review"
on = "success"

[[workflow.edge]]
from = "review"
to = "done"
on = "success"
```

Notes:

- `workflow_files` paths are resolved relative to the main `enki.toml`
- included files can define `[[transform]]`, `[[task]]`, and `[[workflow]]`
- agents still come from the main `enki.toml`
- duplicate workflow, task, or transform IDs across included files fail validation
- `workflow_files` is supported under `[project]`, and the loader also accepts a top-level `workflow_files` key for compatibility

## Interactive mode

`enki join` is the current human-in-the-loop entry point for direct agent chats. It keeps session state, lets you talk to an agent repeatedly, and routes messages through the runtime instead of starting a new one-shot process every turn.

For workflows, use `enki workflow join --run <run-id>` to respond to runtime intervention prompts.

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

Workflow CLI behavior in the current implementation:

- workflow execution uses the Rust core workflow runtime
- workflow tasks run through the Rust-managed multi-agent runtime
- Python-scripted agents are not executed by `enki workflow` yet
- CLI transform support is limited to built-in transforms: `identity` and `extract_content`

`enki run` currently prints detailed execution steps for Rust-managed runs, which is useful when debugging loop phases and tool usage.
