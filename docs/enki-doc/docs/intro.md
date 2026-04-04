---
sidebar_position: 1
slug: /intro
---

# Enki

Enki is an async-first agent framework built around a Rust runtime, with bindings for Python and JavaScript.

This site tracks the current `core-next` workspace:

- [Python](/docs/python): the published `enki-py` package, high-level `Agent` wrapper, multi-agent runtime, low-level bindings, and memory APIs
- [JavaScript](/docs/javascript): the published `@getenki/ai` native Node.js package with single-agent and multi-agent runtimes
- [Rust](/docs/rust): the core runtime workspace, crate layout, execution tracing, and local build workflow
- [Builder CLI](/docs/builder-cli): manifest-driven project scaffolding, execution, monitoring, and interactive sessions
- [Agent Design](/docs/agent-design): the runtime architecture, state machine, binding strategy, and multi-agent design model

## Choose your entry point

### Python

Use Python if you want the most complete packaged experience today.

```bash
pip install enki-py
```

Start here:

- [Python overview](/docs/python)
- [Installation](/docs/installation)
- [Getting Started Guide](/docs/agent-wrapper)
- [Examples](/docs/examples)

### JavaScript

Use JavaScript when you want to run Enki from Node.js through the native `@getenki/ai` package, including multi-agent orchestration from JavaScript or TypeScript.

Start here:

- [JavaScript overview](/docs/javascript)
- [JavaScript Multi-Agent](/docs/javascript-multi-agent)
- [TypeScript](/docs/typescript)

### Rust

Use Rust when you want the underlying runtime, workspace crates, or contributor build flow.

Start here:

- [Rust overview](/docs/rust)
- [Builder CLI](/docs/builder-cli)
- [Build from Source](/docs/build-from-source)
