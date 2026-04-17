---
sidebar_position: 5
slug: /examples
---

# Examples

These examples show the main ways to use Enki across Python, JavaScript, and TypeScript, including workflow orchestration and human-in-the-loop execution.

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

## Repository examples

The repository currently includes these runnable examples:

### Python

- `example/enki-py/simple_agent.py`: minimal high-level `Agent` usage with `run_sync()`
- `example/enki-py/simple_multi_agent.py`: coordinator plus researcher setup with `MultiAgentRuntime`
- `example/enki-py/multi_agent_with_memory_and_tools.py`: shared memory backend, decorator tools, and delegated multi-agent execution
- `example/enki-py/agent_workflow.py`: workflow runtime example using `Agent(...).as_workflow_agent(...)`
- `example/enki-py/human_intervention_workflow.py`: interactive workflow example showing `human_gate` pauses and failure escalation interventions

### TypeScript and JavaScript

- `example/basic-ts/index.ts`: basic TypeScript multi-agent runtime example
- `example/basic-ts/multi-agent-tools-memory.ts`: TypeScript example with researcher/coordinator agents, tool calling, and shared memory
- `example/basic-ts/agent-workflow.ts`: TypeScript workflow runtime example using `NativeEnkiAgent` and `NativeWorkflowRuntime`
- `example/basic-ts/human-intervention-workflow.ts`: interactive TypeScript workflow example showing `human_gate` pauses and failure escalation interventions
- `example/basic-js/index.js`: basic JavaScript multi-agent runtime example
- `example/basic-js/multi-agent-tools-memory.js`: JavaScript example with researcher/coordinator agents, tool calling, and shared memory

For workflow-specific docs, see:

- [Python Workflow](/docs/python-workflow)
- [JavaScript Workflow](/docs/javascript-workflow)
- [TypeScript Workflow](/docs/typescript-workflow)
