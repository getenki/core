import json
from typing import Any

from enki_py import Agent


def register_assistant_tools(agent: Agent, config: dict[str, Any] | None = None) -> None:
    """Register project-specific tools or MCP bridges for the assistant agent."""
    agent_config = config or {}

    @agent.tool_plain
    def project_runtime_info() -> str:
        """Return the current agent setup from Python-defined runtime code."""
        return json.dumps(
            {
                "agent_id": agent_config.get("id", "assistant"),
                "name": agent_config.get("name", agent.name),
                "model": agent_config.get("model", agent.model),
                "capabilities": agent_config.get("capabilities", []),
            }
        )
