import json
from typing import Any

from enki_py import Agent


def register_weather_tools(agent: Agent, config: dict[str, Any] | None = None) -> None:
    """Register tools for weather."""
    tool_config = config or {}

    @agent.tool_plain
    def weather_info() -> str:
        """Return runtime metadata for this tool."""
        print("Weather tool called with config:", tool_config)
        return json.dumps(
            {
                "tool": "weather",
                "agent_id": tool_config.get("id"),
                "agent_name": tool_config.get("name"),
                "model": tool_config.get("model"),
            }
        )
