from __future__ import annotations

from dataclasses import dataclass, field
from time import time_ns

from enki_py import Agent, MemoryBackend, MemoryEntry, MemoryKind, MultiAgentMember, MultiAgentRuntime


@dataclass
class SessionMemory(MemoryBackend):
    name: str = "session_memory"
    _entries: dict[str, list[MemoryEntry]] = field(default_factory=dict)

    def record(self, session_id: str, user_msg: str, assistant_msg: str) -> None:
        session_entries = self._entries.setdefault(session_id, [])
        session_entries.append(
            MemoryEntry(
                key=f"{session_id}-user-{len(session_entries)}",
                content=user_msg,
                kind=MemoryKind.RECENT_MESSAGE,
                relevance=1.0,
                timestamp_ns=time_ns(),
            )
        )
        session_entries.append(
            MemoryEntry(
                key=f"{session_id}-assistant-{len(session_entries)}",
                content=assistant_msg,
                kind=MemoryKind.RECENT_MESSAGE,
                relevance=0.9,
                timestamp_ns=time_ns(),
            )
        )

    def recall(self, session_id: str, query: str, max_entries: int) -> list[MemoryEntry]:
        session_entries = self._entries.get(session_id, [])
        query_terms = {term.lower() for term in query.split() if term.strip()}

        if not query_terms:
            return session_entries[-max_entries:]

        matches = [
            entry
            for entry in session_entries
            if any(term in entry.content.lower() for term in query_terms)
        ]
        return matches[-max_entries:]

    def flush(self, session_id: str) -> None:
        return None

    def consolidate(self, session_id: str) -> None:
        return None


def main() -> None:
    model = "ollama::qwen3.5:latest"
    shared_memory = SessionMemory()

    coordinator = Agent(
        model,
        name="Coordinator",
        instructions=(
            "You are a coordinator agent. "
            "Use discover_agents first. "
            "When research is needed, delegate_task to the researcher. "
            "Ask the researcher to call its available tools when useful. "
            "Mention the answer and which agent handled it."
        ),
        memories=[shared_memory.as_memory_module()],
    )

    researcher = Agent(
        model,
        name="Researcher",
        instructions=(
            "You are a researcher agent. "
            "Use your tools to answer factual questions. "
            "Keep the response concise and grounded in tool output."
        ),
        memories=[shared_memory.as_memory_module()],
    )

    @researcher.tool_plain
    def lookup_example_topics(topic: str) -> str:
        """Return a canned fact for an example topic."""
        facts = {
            "memory": "Memory lets the agent persist and recall useful session context.",
            "tools": "Tools let the agent call Python functions to fetch or compute structured results.",
            "multi-agent": "Multi-agent runtimes let a coordinator route work to specialized agents.",
        }
        return facts.get(topic.lower(), f"No prepared fact exists for '{topic}'.")

    runtime = MultiAgentRuntime(
        [
            MultiAgentMember(
                agent_id="coordinator",
                agent=coordinator,
                capabilities=["planning", "orchestration"],
                description="Routes work across agents.",
            ),
            MultiAgentMember(
                agent_id="researcher",
                agent=researcher,
                capabilities=["research", "tools", "memory"],
                description="Uses tools and shared memory to answer delegated questions.",
            ),
        ]
    )

    session_id = "multi-agent-memory-tools-example"

    runtime.process_sync(
        "researcher",
        "Remember that the user cares about memory and tools in Enki examples.",
        session_id=session_id,
    )

    result = runtime.process_sync(
        "coordinator",
        (
            "Use discover_agents first. "
            "Then delegate_task to the researcher and ask it to use the "
            "lookup_example_topics tool for memory and tools. "
            "Also mention the remembered user preference if it is available."
        ),
        session_id=session_id,
    )
    print(result.output)


if __name__ == "__main__":
    main()
