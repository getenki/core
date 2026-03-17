---
sidebar_position: 4
slug: /low-level-api
---

# Low-Level API

Use the low-level API when you want direct control over the backend agent, tool schema, and tool execution callback.

The package exposes:

- `EnkiTool`
- `EnkiToolHandler`
- `EnkiAgent`

## `EnkiTool`

`EnkiTool` is a simple spec object with:

- `name`
- `description`
- `parameters_json`

Example:

```python
import json
import enki_py

tool = enki_py.EnkiTool(
    name="sum_numbers",
    description="Sum a list of integers and return the total as text.",
    parameters_json=json.dumps(
        {
            "type": "object",
            "properties": {
                "values": {
                    "type": "array",
                    "items": {"type": "integer"},
                }
            },
            "required": ["values"],
        }
    ),
)
```

## `EnkiToolHandler`

Implement `execute()` to handle tool calls from the Rust backend:

```python
class DemoToolHandler:
    def execute(
        self,
        tool_name: str,
        args_json: str,
        agent_dir: str,
        workspace_dir: str,
        sessions_dir: str,
    ) -> str:
        ...
```

The handler receives:

- `tool_name`: selected tool
- `args_json`: serialized JSON arguments
- `agent_dir`: agent working directory
- `workspace_dir`: workspace directory
- `sessions_dir`: session storage directory

## `EnkiAgent`

The main constructors are:

### Plain constructor

```python
agent = enki_py.EnkiAgent(
    name="Minimal Agent",
    system_prompt_preamble="You are concise.",
    model="ollama::qwen3.5:latest",
    max_iterations=4,
    workspace_home=None,
)
```

### `with_tools`

```python
agent = enki_py.EnkiAgent.with_tools(
    name="Test Agent",
    system_prompt_preamble="Prefer custom Python tools.",
    model="ollama::qwen3.5:latest",
    max_iterations=4,
    workspace_home="./test",
    tools=[tool],
    handler=DemoToolHandler(),
)
```

## Running the backend

`run()` is async and takes a session id plus the user message:

```python
result = await agent.run("session-custom-tools", "Use the available tools.")
print(result)
```

Use [Getting Started Guide](/docs/agent-wrapper) instead if you want decorator-based tools and a simpler Python-first interface.
