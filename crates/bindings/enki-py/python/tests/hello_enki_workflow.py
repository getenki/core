import asyncio
import json
import os
from pathlib import Path

import enki_py


MODEL = os.getenv("ENKI_MODEL", "ollama::qwen3.5:latest")
WORKSPACE_HOME = Path("./test/workflow-example")


def build_members() -> list[enki_py.EnkiWorkflowMember]:
    return [
        enki_py.EnkiWorkflowMember(
            agent_id="researcher",
            name="Researcher",
            system_prompt_preamble=(
                "You are a concise researcher. Return short factual notes that are easy to summarize."
            ),
            model=MODEL,
            max_iterations=4,
            capabilities=["research"],
        ),
        enki_py.EnkiWorkflowMember(
            agent_id="writer",
            name="Writer",
            system_prompt_preamble=(
                "You turn research notes into short polished summaries."
            ),
            model=MODEL,
            max_iterations=4,
            capabilities=["writing"],
        ),
    ]


def build_tasks_json() -> list[str]:
    tasks = [
        {
            "id": "research_topic",
            "target": {"type": "capabilities", "value": ["research"]},
            "prompt": "Research {{topic}} and return 3 concise bullet points.",
            "input_bindings": {"topic": "input.topic"},
        },
        {
            "id": "write_summary",
            "target": {"type": "agent_id", "value": "writer"},
            "prompt": (
                "Write a short summary for {{topic}} using these notes:\n"
                "{{research.content}}"
            ),
            "input_bindings": {
                "topic": "input.topic",
                "research": "research",
            },
        },
    ]
    return [json.dumps(task) for task in tasks]


def build_workflows_json() -> list[str]:
    workflows = [
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
    ]
    return [json.dumps(workflow) for workflow in workflows]


async def main() -> None:
    WORKSPACE_HOME.mkdir(parents=True, exist_ok=True)

    runtime = enki_py.EnkiWorkflowRuntime(
        members=build_members(),
        tasks_json=build_tasks_json(),
        workflows_json=build_workflows_json(),
        workspace_home=str(WORKSPACE_HOME),
    )

    print("Registered workflows:")
    print(json.dumps(json.loads(await runtime.list_workflows_json()), indent=2))

    response = json.loads(
        await runtime.start_json(
            json.dumps(
                {
                    "workflow_id": "research-to-summary",
                    "input": {"topic": "workflow bindings in enki-py"},
                }
            )
        )
    )

    print("\nWorkflow response:")
    print(json.dumps(response, indent=2))

    run_id = response["run_id"]
    inspected = json.loads(await runtime.inspect_json(run_id))
    print("\nPersisted run state:")
    print(json.dumps(inspected, indent=2))

    runs = json.loads(await runtime.list_runs_json())
    print("\nKnown runs:")
    print(json.dumps(runs, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
