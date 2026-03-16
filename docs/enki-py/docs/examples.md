---
sidebar_position: 5
slug: /examples
---

# Examples

The repository already contains a few strong examples of how `enki-py` is intended to be used.

## Decorator-style tools

Based on `python/tests/test_agent_wrapper.py`:

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

Based on `python/tests/test_all.py`:

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

## File-organization review agent

`python/tests/simple_file_organization_agent.py` is a larger end-to-end example showing:

- a typed dependency object with a review root
- multiple filesystem-oriented tools
- safe path resolution inside the review root
- a reusable `review_folder()` helper around `agent.run_sync()`
