---
sidebar_position: 2
slug: /python-workflow
---

# Python Workflow

Python exposes the Rust workflow runtime through the generated low-level bindings.

Use `EnkiWorkflowRuntime` when you want to register workflow agents, tasks, and workflow definitions directly from Python. The recommended path is to build agents with the high-level `Agent` wrapper so your Python-side LLM provider, tools, and memories are attached, then convert them into workflow-ready low-level agents with `as_workflow_agent(...)`.
## Recommended pattern

- Build each participant as `enki_py.Agent(...)`
- Pass `llm=` explicitly when you want a custom provider, or let `LiteLlmProvider()` handle supported models
- Convert each configured agent with `agent.as_workflow_agent(agent_id=..., capabilities=[...])`
- Pass those converted low-level agents into `EnkiWorkflowRuntime(...)`

```python
import asyncio
import json
import enki_py


async def main() -> None:
    researcher = enki_py.Agent(
        "ollama::qwen3.5:latest",
        name="Researcher",
        instructions="Return short factual notes.",
        llm=enki_py.LiteLlmProvider(),
    )
    writer = enki_py.Agent(
        "ollama::qwen3.5:latest",
        name="Writer",
        instructions="Turn notes into a concise summary.",
        llm=enki_py.LiteLlmProvider(),
    )

    runtime = enki_py.EnkiWorkflowRuntime(
        agents=[
            researcher.as_workflow_agent(
                agent_id="researcher",
                capabilities=["research"],
            ),
            writer.as_workflow_agent(
                agent_id="writer",
                capabilities=["writing"],
            ),
        ],
        tasks_json=[...],
        workflows_json=[...],
        workspace_home="./.enki",
    )

    response = json.loads(await runtime.start_json(json.dumps({...})))
    print(response["status"])


asyncio.run(main())
```

## Why not construct raw `EnkiAgent` objects?

If you construct low-level `EnkiAgent(...)` values directly, they do not automatically gain a Python-side LLM provider. That can lead to workflow runs returning `Initialization error: No built-in LLM provider is available...` instead of real agent output.

For most Python usage, `Agent(...).as_workflow_agent(...)` is the safer default. See `example/enki-py/agent_workflow.py` for the full runnable sample, including a custom `OllamaProvider` fallback.

Supported workflow methods:

- `list_workflows_json()`
- `list_runs_json()`
- `inspect_json(run_id)`
- `start_json(request_json)`
- `resume_json(run_id)`
- `submit_intervention_json(run_id, intervention_id, response=None)`






