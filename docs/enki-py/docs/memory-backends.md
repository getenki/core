---
sidebar_position: 4
slug: /memory-backends
---

# Memory Backends

Use `MemoryBackend` when you want to plug custom memory into the high-level `Agent` wrapper.

## What it provides

`MemoryBackend` is the Python-side contract for agent memory. You implement the storage and retrieval behavior, then register it with `Agent` through `as_memory_module()`.

The class defines:

- `record(session_id, user_msg, assistant_msg)`
- `recall(session_id, query, max_entries)`
- `flush(session_id)`
- `consolidate(session_id)`: optional
- `as_memory_module()`

## Minimal shape

```python
from enki_py import MemoryBackend, MemoryEntry


class MyMemory(MemoryBackend):
    name = "my_memory"

    def record(self, session_id: str, user_msg: str, assistant_msg: str) -> None:
        ...

    def recall(
        self,
        session_id: str,
        query: str,
        max_entries: int,
    ) -> list[MemoryEntry]:
        ...

    def flush(self, session_id: str) -> None:
        ...
```

## Registering with an agent

Create the backend instance, convert it to a `MemoryModule`, and pass it to `Agent`.

```python
from enki_py import Agent

memory = MyMemory()

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    memories=[memory.as_memory_module()],
)
```

## When to use `consolidate()`

Override `consolidate()` if your backend needs a compaction or summarization hook. If you do not need it, inherit the default no-op implementation.

```python
def consolidate(self, session_id: str) -> None:
    ...
```

## Related types

- `MemoryEntry`: the unit returned by `recall()`
- `MemoryKind`: categorizes entries such as recent messages and summaries
- `MemoryModule`: the registration object built by `as_memory_module()`

See [Memory Examples](/docs/memory-examples) for a full implementation.
