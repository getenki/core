---
sidebar_position: 1
slug: /intro
---

# enki-py

`enki-py` is the Python binding for the Enki core crate in this repository.

It exposes two layers:

- A generated low-level API built around `EnkiAgent`, `EnkiTool`, and `EnkiToolHandler`
- A higher-level Python wrapper in `enki_py.agent` that adds decorator-based tools, dependency injection, and sync helpers

## What to use

Use the high-level `Agent` wrapper when you want Python ergonomics.

For synchronous scripts, use `run_sync()`:

```python
from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
)

result = agent.run_sync("Explain what this project does.")
print(result.output)
```

For async applications, use `await agent.run(...)`:

```python
from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
)


async def hello_enki():
    result = await agent.run("Explain what this project does.")
    print(result.output)


if __name__ == "__main__":
    import asyncio

    asyncio.run(hello_enki())
```

Use the generated low-level API when you need exact control over tool specs and the tool handler callback:

```python
import enki_py

agent = enki_py.EnkiAgent(
    name="Minimal Agent",
    system_prompt_preamble="You are concise.",
    model="ollama::qwen3.5:latest",
    max_iterations=4,
    workspace_home=None,
)
```

## Package layout

The current crate layout is:

- `crates/bindings/enki-py/src`: Rust and UniFFI definitions
- `crates/bindings/enki-py/python/enki_py/enki_py`: generated Python bindings and native library
- `crates/bindings/enki-py/python/enki_py/agent.py`: Python-first wrapper API
- `crates/bindings/enki-py/python/tests`: usage examples and tests

## In this documentation

- [Installation](/docs/installation)
- [Agent wrapper](/docs/agent-wrapper)
- [Low-level API](/docs/low-level-api)
- [Examples](/docs/examples)
