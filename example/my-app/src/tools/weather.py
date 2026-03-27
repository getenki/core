from typing import Any
from enki_py import RunContext


def weather_info(ctx: RunContext[Any], query: str) -> str:
    """Return runtime metadata for the weather tool."""
    return f"Execution of weather successful for query: '{query}'"
