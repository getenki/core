import json
from typing import Any


def project_runtime_info(tool_config: dict[str, Any]) -> str:
    """Return the current agent setup from Python-defined runtime code."""
    return json.dumps(
        {
            "agent_id": tool_config.get("id", "assistant"),
            "name": tool_config.get("name", "Personal Assistant"),
            "model": tool_config.get("model"),
            "capabilities": tool_config.get("capabilities", []),
        }
    )
