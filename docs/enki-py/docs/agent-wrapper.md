---
sidebar_position: 3
slug: /agent-wrapper
---

# Getting Started Guide

Use `Agent` when you want the normal Python entry point.

The wrapper exposes:

- `Agent`
- `AgentRunResult`
- `RunContext`
- `Tool`

## Create an agent

```python
from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    deps_type=str,
    instructions="Use the player's name in the answer.",
    name="Dice Game",
    max_iterations=20,
    workspace_home=None,
)
```

Common constructor parameters:

- `model`: model identifier passed through to the backend
- `deps_type`: optional dependency type for tools that receive context
- `instructions`: system prompt preamble
- `name`: agent name
- `max_iterations`: backend iteration limit
- `workspace_home`: optional workspace root path
- `tools`: optional list of prebuilt `Tool` instances

## Register tools

There are two decorators:

### `@agent.tool_plain`

Use for tools with only explicit JSON arguments.

```python
@agent.tool_plain
def roll_dice() -> str:
    """Roll a six-sided die and return the result."""
    return "4"
```

### `@agent.tool`

Use when the first argument is a `RunContext`.

```python
from enki_py import RunContext

@agent.tool
def get_player_name(ctx: RunContext[str]) -> str:
    """Get the player's name."""
    return ctx.deps
```

## Running the agent

Async:

```python
result = await agent.run("My guess is 4", deps="Anne")
print(result.output)
```

Sync:

```python
result = agent.run_sync("My guess is 4", deps="Anne")
print(result.output)
```

`run_sync()` uses `asyncio.run()` when no loop is active, and falls back to a background thread if a loop is already running.

## Tool schemas

`Tool.from_function()` inspects Python type annotations and builds a JSON schema automatically for:

- `str`
- `int`
- `float`
- `bool`
- `list[T]`
- `tuple[T]`
- `dict[str, T]`
- optional values such as `str | None`

Example generated schema:

```python
@agent.tool_plain
def format_score(total: int, lucky: bool = False) -> str:
    return f"{total}:{lucky}"
```

Produces:

```json
{
  "type": "object",
  "properties": {
    "total": { "type": "integer" },
    "lucky": { "type": "boolean" }
  },
  "additionalProperties": false,
  "required": ["total"]
}
```

## Register tool objects directly

If you do not want decorators, create `Tool` instances directly:

```python
from enki_py import Agent, Tool

agent = Agent("test-model")

def format_score(total: int) -> str:
    return f"score:{total}"

agent.register_tool(Tool.from_function(format_score, uses_context=False))
```
