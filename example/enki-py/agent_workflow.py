import asyncio
import json
import os
import urllib.request
from pathlib import Path

import enki_py


MODEL = os.getenv("ENKI_MODEL", "ollama::qwen3.5:latest")
WORKSPACE_HOME = Path("./example/enki-py/.enki-workflow")


class OllamaProvider(enki_py.LlmProviderBackend):
    def __init__(self, base_url: str | None = None) -> None:
        self.base_url = (
            base_url or os.getenv("OLLAMA_URL") or "http://127.0.0.1:11434"
        ).rstrip("/")

    def complete(
            self,
            model: str,
            messages: list[dict],
            tools: list[dict],
    ) -> dict:
        backend_model = model.split("::", 1)[1] if "::" in model else model
        payload = {
            "model": backend_model,
            "messages": [self._to_ollama_message(message) for message in messages],
            "stream": False,
        }

        if tools:
            payload["tools"] = tools

        request = urllib.request.Request(
            f"{self.base_url}/api/chat",
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )

        with urllib.request.urlopen(request) as response:
            body = json.loads(response.read().decode("utf-8"))

        message = body.get("message", {})
        return {
            "content": message.get("content", ""),
            "tool_calls": message.get("tool_calls", []),
        }

    @staticmethod
    def _to_ollama_message(message: dict) -> dict:
        role = str(message.get("role", "user")).lower()
        role = {
            "system": "system",
            "user": "user",
            "assistant": "assistant",
            "tool": "tool",
        }.get(role, role)

        normalized = {
            "role": role,
            "content": message.get("content", ""),
        }

        tool_call_id = message.get("tool_call_id")
        if tool_call_id:
            normalized["tool_call_id"] = tool_call_id

        return normalized


def build_llm_provider() -> enki_py.LlmProviderBackend:
    if MODEL.startswith("ollama::"):
        return OllamaProvider()
    return enki_py.LiteLlmProvider()


def build_agents() -> list[enki_py.EnkiAgent]:
    llm = build_llm_provider()
    researcher = enki_py.Agent(
        MODEL,
        name="Researcher",
        instructions=(
            "You are a concise researcher. Return short factual notes that are easy to summarize."
        ),
        max_iterations=4,
        workspace_home=str(WORKSPACE_HOME),
        llm=llm,
    )

    writer = enki_py.Agent(
        MODEL,
        name="Writer",
        instructions="You turn research notes into short polished summaries.",
        max_iterations=4,
        workspace_home=str(WORKSPACE_HOME),
        llm=llm,
    )

    return [
        researcher.as_workflow_agent(
            agent_id="researcher",
            capabilities=["research"],
        ),
        writer.as_workflow_agent(
            agent_id="writer",
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
        agents=build_agents(),
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
                    "input": {"topic": "agent workflows in enki-py"},
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


if __name__ == "__main__":
    asyncio.run(main())




