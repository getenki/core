---
sidebar_position: 7
slug: /build-from-source
---

# Build from Source

This page is for contributors working in the `core-next` workspace.

## Requirements

- Python `>=3.8`
- Rust toolchain
- `maturin`
- Node.js `>=18` if you want to run the docs site

## Build the Python package locally

From `crates/bindings/enki-py`:

```bash
pip install maturin
maturin develop
```

If you use the existing virtual environment in the crate, activate it first and run `maturin develop` there.

## Build the JavaScript package locally

From `crates/bindings/enki-js`:

```bash
npm install
npm run build
```

This crate publishes the native Node.js package `@getenki/ai` via `napi-rs`.

## Run the docs site

From `docs/enki-doc`:

```bash
npm install
npm start
```

## Build static docs

```bash
npm run build
```
