"""
Enki Multi-Agent Starter - Python

Run with:   python src/main.py
Or via CLI: enki run --message "Hello!"
"""

import os
import sys

from enki_py import Agent
from tools.assistant import register_assistant_tools


def build_assistant(model: str) -> Agent:
    agent = Agent(
        model,
        name="Personal Assistant",
        instructions=(
            "You are a helpful personal assistant. "
            "Answer questions clearly and concisely. "
            "Use the configured tools when they help."
        ),
    )
    register_assistant_tools(
        agent,
        {
            "id": "assistant",
            "name": "Personal Assistant",
            "model": model,
            "capabilities": ["general", "writing", "analysis"],
        },
    )
    return agent


def main() -> None:
    model = os.environ.get("ENKI_MODEL", "ollama::qwen3.5:latest")

    print("Enki Multi-Agent Runtime")
    print()

    agent = build_assistant(model)

    message = sys.argv[1] if len(sys.argv) > 1 else "Hello! What can you help me with?"
    print(f"> {message}")
    print()

    result = agent.run_sync(message, session_id="session-1")
    print(result.output)


if __name__ == "__main__":
    main()
