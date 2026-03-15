import asyncio
import json
from pathlib import Path

import enki_py


WORKSPACE_HOME = Path("./test")


class DemoToolHandler:
    def __init__(self) -> None:
        self.calls: list[tuple[str, dict]] = []

    def execute(
        self,
        tool_name: str,
        args_json: str,
        agent_dir: str,
        workspace_dir: str,
        sessions_dir: str,
    ) -> str:
        args = json.loads(args_json or "{}")
        self.calls.append((tool_name, args))

        if tool_name == "project_status":
            return json.dumps(
                {
                    "agent_dir": agent_dir,
                    "workspace_dir": workspace_dir,
                    "sessions_dir": sessions_dir,
                    "status": "ready",
                }
            )

        if tool_name == "sum_numbers":
            values = args.get("values", [])
            return str(sum(values))

        if tool_name == "make_slug":
            text = args.get("text", "")
            return text.strip().lower().replace(" ", "-")

        if tool_name == "echo_note":
            title = args.get("title", "note")
            body = args.get("body", "")
            return f"# {title}\n\n{body}"

        return f"Unknown custom tool: {tool_name}"


def build_tools() -> list[enki_py.EnkiToolSpec]:
    return [
        enki_py.EnkiToolSpec(
            name="project_status",
            description="Return the current agent and workspace paths plus a ready status.",
            parameters_json=json.dumps(
                {
                    "type": "object",
                    "properties": {},
                }
            ),
        ),
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
        ),
        enki_py.EnkiToolSpec(
            name="make_slug",
            description="Convert text into a lowercase dash-separated slug.",
            parameters_json=json.dumps(
                {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"},
                    },
                    "required": ["text"],
                }
            ),
        ),
        enki_py.EnkiToolSpec(
            name="echo_note",
            description="Format a title and body as a markdown note.",
            parameters_json=json.dumps(
                {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "body": {"type": "string"},
                    },
                    "required": ["title", "body"],
                }
            ),
        ),
    ]


async def main() -> tuple[str, DemoToolHandler]:
    print("Testing agent creation with Python-defined tools...")
    handler = DemoToolHandler()
    agent = enki_py.EnkiAgent.with_tools(
        name="Test Agent",
        system_prompt_preamble=(
            "You are a concise test agent. "
            "Prefer the custom Python tools when they directly answer the request."
        ),
        model="ollama::qwen3.5:latest",
        max_iterations=4,
        workspace_home=str(WORKSPACE_HOME),
        tools=build_tools(),
        handler=handler,
        include_builtin_tools=True,
    )
    res = await agent.run(
        "session-custom-tools",
        (
            "Use project_status, sum_numbers, make_slug, and echo_note. "
            "Sum 7, 8, and 9. Make a slug from 'Enki Python Tools'. "
            "Create a note titled 'Tool Summary' with body 'custom tools are wired'. "
            "Then answer with a short summary."
        ),
    )
    print(res)
    print("Custom tool calls:", handler.calls)
    return res, handler


def test_create_agent_with_custom_tools():
    result, handler = asyncio.run(main())
    assert isinstance(result, str)
    assert handler.calls


if __name__ == "__main__":
    test_create_agent_with_custom_tools()
