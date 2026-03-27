import json
from typing import Any


def weather_info(tool_config: dict[str, Any]) -> str:
    """Return runtime metadata for the weather tool."""
    print(f"Getting weather info for tool config: {tool_config}")
    return json.dumps(
        {
            "tool": "weather",
            "agent_id": tool_config.get("id"),
            "agent_name": tool_config.get("name"),
            "model": tool_config.get("model"),
        }
    )
