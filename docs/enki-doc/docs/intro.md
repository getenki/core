---
sidebar_position: 1
slug: /intro
---

# enki-py

`enki-py` is a Python package for building agents on top of Enki.

Install it from PyPI with `pip`:

```bash
pip install enki-py
```

Or add it to a project managed by `uv`:

```bash
uv add enki-py
```

It exposes two layers:

- A generated low-level API built around `EnkiAgent`, `EnkiTool`, and `EnkiToolHandler`
- A higher-level Python wrapper in `enki_py.agent` that adds decorator-based tools, custom memory backends, dependency injection, and sync helpers

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

## In this documentation

- [Installation](/docs/installation)
- [Getting Started Guide](/docs/agent-wrapper)
- [Memory Backends](/docs/memory-backends)
- [Memory Examples](/docs/memory-examples)
- [Low-level API](/docs/low-level-api)
- [Examples](/docs/examples)
- [FAQ](/docs/faq)
