---
sidebar_position: 2
slug: /python-workflow
---

# Python Workflow

Python exposes the Rust workflow runtime through the generated low-level bindings.

Use `EnkiWorkflowRuntime` when you want to register workflow agents, tasks, and workflow definitions directly from Python. The current surface is low-level and async: agents are configured first, tasks and workflows are passed in as JSON strings, and workflow responses are returned as JSON strings.

```python
import asyncio
import json

import enki_py


async def main() -> None:
    researcher = enki_py.EnkiAgent(
        name="Researcher",
        system_prompt_preamble="Return short factual notes.",
        model="ollama::qwen3.5:latest",
        max_iterations=4,
        workspace_home="./.enki",
    )
    researcher.configure_workflow(
        agent_id="researcher",
        capabilities=["research"],
    )

    writer = enki_py.EnkiAgent(
        name="Writer",
        system_prompt_preamble="Turn notes into a concise summary.",
        model="ollama::qwen3.5:latest",
        max_iterations=4,
        workspace_home="./.enki",
    )
    writer.configure_workflow(
        agent_id="writer",
        capabilities=["writing"],
    )

    tasks_json = [
        json.dumps(
            {
                "id": "research_topic",
                "target": {"type": "capabilities", "value": ["research"]},
                "prompt": "Research {{topic}} and return 3 concise bullet points.",
                "input_bindings": {"topic": "input.topic"},
            }
        ),
        json.dumps(
            {
                "id": "write_summary",
                "target": {"type": "agent_id", "value": "writer"},
                "prompt": "Write a short summary for {{topic}} using {{research.content}}",
                "input_bindings": {
                    "topic": "input.topic",
                    "research": "research",
                },
            }
        ),
    ]

    workflows_json = [
        json.dumps(
            {
                "id": "research-to-summary",
                "name": "Research To Summary",
                "nodes": [
                    {
                        "id": "research",
                        "kind": "task",
                        "task_id": "research_topic",
                        "output_key": "research",
                    },
                    {
                        "id": "summary",
                        "kind": "task",
                        "task_id": "write_summary",
                        "output_key": "summary",
                    },
                ],
                "edges": [
                    {
                        "from": "research",
                        "to": "summary",
                        "transition": {"type": "always"},
                    }
                ],
            }
        )
    ]

    runtime = enki_py.EnkiWorkflowRuntime(
        agents=[researcher, writer],
        tasks_json=tasks_json,
        workflows_json=workflows_json,
        workspace_home="./.enki",
    )

    response = json.loads(
        await runtime.start_json(
            json.dumps(
                {
                    "workflow_id": "research-to-summary",
                    "input": {"topic": "agent workflows in enki-py"},
                }
            )
        )
    )

    persisted = json.loads(await runtime.inspect_json(response["run_id"]))
    print(persisted["status"])


asyncio.run(main())
```

Supported workflow methods:

- `list_workflows_json()`
- `list_runs_json()`
- `inspect_json(run_id)`
- `start_json(request_json)`
- `resume_json(run_id)`
- `submit_intervention_json(run_id, intervention_id, response=None)`
