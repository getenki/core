---
sidebar_position: 1
slug: /python
---

# Python

`enki-py` is the Python package for building agents on top of Enki.

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
- A higher-level Python wrapper in `enki_py.agent` that adds decorator-based tools, custom memory backends, dependency injection, sync helpers, step tracing, Python-side LLM adapters, and Python-native multi-agent orchestration

The high-level wrapper also supports both prompt-level loop customization and Python-defined loop overrides.

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

## Execution tracing

`Agent.run()` and `Agent.run_sync()` return `AgentRunResult`, which includes both `output` and `steps`.

You can also stream steps as they happen with `on_step`:

```python
from enki_py import Agent, ExecutionStep

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
)

def print_step(step: ExecutionStep) -> None:
    print(f"[{step.index}] {step.phase}/{step.kind}: {step.detail}")

result = agent.run_sync("Explain what this project does.", on_step=print_step)
print(result.output)
```

The step stream mirrors the Rust runtime's execution phases and is the easiest way to inspect tool calls and loop progress from Python.

## Python-side LLM providers

The high-level `Agent` accepts `llm=` for a custom provider backend or callback.

Two common options are:

- use the built-in `LiteLlmProvider`
- pass your own callable that receives `model`, `messages`, and `tools`

```python
from enki_py import Agent, LiteLlmProvider

agent = Agent(
    "openai::gpt-4o",
    instructions="Be concise.",
    llm=LiteLlmProvider(),
)
```

## Custom loops

Use `agentic_loop=` when you want to change the loop instructions seen by the model while keeping the normal Rust runtime loop:

```python
from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    agentic_loop=(
        "1. Understand the request.\n"
        "2. Decide whether a tool is needed.\n"
        "3. Summarize observations.\n"
        "4. Return the final answer."
    ),
)
```

Use `agent_loop_handler=` when you want Python to drive the actual turn-by-turn loop:

```python
from enki_py import Agent, AgentLoopRequest, AgentLoopResult, ExecutionStep


def custom_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    return AgentLoopResult(
        output=f"Handled in Python for: {request.user_message}",
        steps=[
            ExecutionStep(
                index=1,
                phase="Custom",
                kind="final",
                detail="Returned a final answer from Python",
            )
        ],
    )


agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    agent_loop_handler=custom_loop,
)
```

This is the path to use for planner-executor loops, ReAct loops, or side-by-side loop comparisons.

## Low-Level API

For workflow orchestration from Python, see [Python Workflow](/docs/python-workflow).

For multi-agent orchestration with Python `Agent` instances, see [Python Multi-Agent](/docs/python-multi-agent).

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

## Python docs

- [Installation](/docs/installation)
- [Python Workflow](/docs/python-workflow)
- [Python Multi-Agent](/docs/python-multi-agent)
- [Getting Started Guide](/docs/agent-wrapper)
- [Memory Backends](/docs/memory-backends)
- [Memory Examples](/docs/memory-examples)
- [Low-level API](/docs/low-level-api)
- [Examples](/docs/examples)
- [FAQ](/docs/faq)
