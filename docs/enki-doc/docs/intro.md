---
sidebar_position: 1
slug: /intro
---

# Enki

Enki is an async-first agent framework built around a Rust runtime, with separate bindings for Python and JavaScript.

This docs site now splits the main entry points by language so each stack has its own place:

- [Python](/docs/python): published `enki-py` package, high-level `Agent` wrapper, low-level bindings, and memory APIs
- [JavaScript](/docs/javascript): browser-oriented `enki-js` WASM bindings with JavaScript callbacks for LLMs and tools
- [Rust](/docs/rust): core runtime, workspace layout, and local build workflow

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

### JavaScript

Use JavaScript when you want to run Enki through WebAssembly in a browser or another JS runtime that can load WASM.

Start here:

- [JavaScript overview](/docs/javascript)
- [WASM Usage](/docs/wasm-usage)

### Rust

Use Rust when you want the underlying runtime, workspace crates, or contributor build flow.

Start here:

- [Rust overview](/docs/rust)
- [Build from Source](/docs/build-from-source)
