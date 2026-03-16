---
sidebar_position: 5
slug: /memory-examples
---

# Memory Examples

This page shows a complete custom memory backend built on `MemoryBackend`.

## In-memory example

```python
from enki_py import Agent, MemoryBackend, MemoryEntry, MemoryKind


class ExampleMemory(MemoryBackend):
    name = "python_memory"

    def __init__(self) -> None:
        self._sessions: dict[str, list[tuple[str, str]]] = {}

    def record(self, session_id: str, user_msg: str, assistant_msg: str) -> None:
        exchanges = self._sessions.setdefault(session_id, [])
        exchanges.append(("user", user_msg))
        exchanges.append(("assistant", assistant_msg))

    def recall(
        self,
        session_id: str,
        query: str,
        max_entries: int,
    ) -> list[MemoryEntry]:
        exchanges = self._sessions.get(session_id, [])
        recent = exchanges[-max_entries:]
        return [
            MemoryEntry(
                key=f"recent-{index}",
                content=f"{role}: {content}",
                kind=MemoryKind.RECENT_MESSAGE,
                relevance=0.8,
                timestamp_ns=index,
            )
            for index, (role, content) in enumerate(recent)
        ]

    def flush(self, session_id: str) -> None:
        self._sessions.setdefault(session_id, [])


memory = ExampleMemory()

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    memories=[memory.as_memory_module()],
)

result = agent.run_sync("Explain what this project does.")
print(result.output)
```

## Notes

- This example keeps memory in process-local Python state.
- Real backends can store entries in a database, file, cache, or vector store.
- `recall()` decides which `MemoryEntry` values to return for the current query.
