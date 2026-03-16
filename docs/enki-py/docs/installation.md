---
sidebar_position: 2
slug: /installation
---

# Installation

The binding is packaged with `maturin` and points its Python sources at `python/`.

## Requirements

- Python `>=3.8`
- Rust toolchain for local builds
- Node.js `>=18` for this Docusaurus site

## Build the Python package locally

From `crates/bindings/enki-py`:

```bash
pip install maturin
maturin develop
```

If you use the existing virtual environment in the crate, activate it first and run `maturin develop` there.

## Install docs dependencies

From `docs/enki-py`:

```bash
npm install
```

## Run the docs site

```bash
npm start
```

## Build static docs

```bash
npm run build
```

This writes the static site to `docs/enki-py/build`.
