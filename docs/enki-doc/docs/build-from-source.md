---
sidebar_position: 7
slug: /build-from-source
---

# Build from Source

This page is for contributors working on the binding itself.

## Requirements

- Python `>=3.8`
- Rust toolchain
- `maturin`
- Node.js `>=18` if you want to run the docs site

## Build the package locally

From `crates/bindings/enki-py`:

```bash
pip install maturin
maturin develop
```

If you use the existing virtual environment in the crate, activate it first and run `maturin develop` there.

## Run the docs site

From `docs/enki-py`:

```bash
npm install
npm start
```

## Build static docs

```bash
npm run build
```
