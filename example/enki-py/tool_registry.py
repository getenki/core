import sys
import uuid
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
LOCAL_ENKI_PY = REPO_ROOT / "crates" / "bindings" / "enki-py" / "python"
if str(LOCAL_ENKI_PY) not in sys.path:
    sys.path.insert(0, str(LOCAL_ENKI_PY))

from enki_py import Agent, ToolRegistry


def main() -> None:
    model = "ollama::qwen3.5:latest"
    session_id = f"tool-registry-{uuid.uuid4()}"

    registry = ToolRegistry()

    @registry.tool_plain
    def lookup_release_note(feature: str) -> str:
        """Return a short release note for a named feature."""
        notes = {
            "registry": "Tool registries let teams define tools once and attach them to multiple agents.",
            "workflow": "Workflow agents can now share reusable tool configuration instead of duplicating tool definitions.",
            "memory": "Memory modules continue to work alongside connected tool registries.",
        }
        return notes.get(feature.lower(), f"No release note found for '{feature}'.")

    @registry.tool_plain
    def summarize_priority(feature: str) -> str:
        """Explain whether a feature should be treated as high priority."""
        if feature.lower() == "registry":
            return "High priority: it removes duplicated tool wiring across agents."
        return f"Normal priority: '{feature}' is useful but not flagged as urgent."

    agent = Agent(
        model,
        name="Registry Agent",
        instructions=(
            "You explain Enki features clearly. "
            "Use connected tools before making implementation claims. "
            "Keep the final answer concise."
        ),
    )

    agent.connect_tool_registry(registry)

    print("Connected tools:", ", ".join(tool.name for tool in registry.tools()))

    result = agent.run_sync(
        (
            "Explain what the tool registry example demonstrates. "
            "Use lookup_release_note for registry and summarize_priority for registry. "
            "Mention that the tools were attached dynamically to the agent."
        ),
        session_id=session_id,
    )

    print("\nExecution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")

    print("\nAgent output:")
    print(result.output)


if __name__ == "__main__":
    main()
