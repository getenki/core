---
sidebar_position: 5
slug: /examples
---

# Examples

These examples show the main ways to use `enki-py`, including the newer multi-agent flows added in the current workspace.

## Decorator-style tools

```python
from enki_py import Agent, RunContext

agent = Agent(
    "ollama::qwen3.5:latest",
    deps_type=str,
    instructions="Use the player's name in the response.",
)

@agent.tool_plain
def roll_dice() -> str:
    return "4"

@agent.tool
def get_player_name(ctx: RunContext[str]) -> str:
    return ctx.deps

result = agent.run_sync("My guess is 4", deps="Anne")
print(result.output)
```

## Explicit low-level tools

```python
import json
import enki_py

class DemoToolHandler:
    def execute(
        self,
        tool_name: str,
        args_json: str,
        agent_dir: str,
        workspace_dir: str,
        sessions_dir: str,
    ) -> str:
        args = json.loads(args_json or "{}")
        if tool_name == "sum_numbers":
            return str(sum(args.get("values", [])))
        return ""

tools = [
    enki_py.EnkiToolSpec(
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
]

agent = enki_py.EnkiAgent.with_tools(
    name="Test Agent",
    system_prompt_preamble="Use custom Python tools when possible.",
    model="ollama::qwen3.5:latest",
    max_iterations=4,
    workspace_home="./test",
    tools=tools,
    handler=DemoToolHandler(),
)
```

## Custom memory backend

See [Memory Backends](/docs/memory-backends) for the API and [Memory Examples](/docs/memory-examples) for a full example.

Custom memory callbacks support both synchronous functions and `async def` methods.

## File-organization review agent

A larger end-to-end agent can combine:

- a typed dependency object with a review root
- multiple filesystem-oriented tools
- safe path resolution inside the review root
- a reusable `review_folder()` helper around `agent.run_sync()`

## Support triage agent

A useful production-style pattern is a support triage agent that:

- classifies issue severity
- maps the problem to a subsystem such as `billing`, `auth`, or `api`
- uses tools to pull runbook context before answering
- returns a concise recommendation for the human operator

See [Agent Design](/docs/agent-design) for a full example of this use case and how it maps onto the Enki runtime model.

## Repository examples

The repository currently includes these runnable Python examples:

- `example/enki-py/simple_agent.py`: minimal high-level `Agent` usage with `run_sync()`
- `example/enki-py/simple_multi_agent.py`: coordinator plus researcher setup with `MultiAgentRuntime`
- `example/enki-py/multi_agent_with_memory_and_tools.py`: shared memory backend, decorator tools, and delegated multi-agent execution
