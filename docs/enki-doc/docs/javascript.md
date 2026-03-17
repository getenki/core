---
sidebar_position: 1
slug: /javascript
---

# JavaScript

`enki-js` is the browser-oriented JavaScript binding for Enki, built with `wasm-bindgen`.

Use it when you want to run the Rust agent loop through WebAssembly and keep LLM and tool execution in JavaScript callbacks.

## What it exposes

- `EnkiJsTool`: tool metadata registered from JavaScript
- `EnkiJsAgent`: a WASM-backed agent instance with JavaScript-provided LLM and tool handlers

## Runtime model

The current JavaScript binding is intentionally browser-safe:

- Session state is stored in memory
- Filesystem-backed persistence is not used
- LLM execution is delegated to a JavaScript callback
- Tool execution is delegated to an optional JavaScript callback

## Install

Install the published package from npm:

```bash
npm install enki-js
```

Then import it directly from your application:

```js
import init, { EnkiJsAgent } from "enki-js";
```

## Build from source

From `crates/bindings/enki-js`:

```bash
wasm-pack build --target bundler --out-dir pkg
```

If you need the `web` target instead:

```bash
wasm-pack build --target web --out-dir pkg-web
```

## JavaScript docs

- [WASM Usage](/docs/wasm-usage)
