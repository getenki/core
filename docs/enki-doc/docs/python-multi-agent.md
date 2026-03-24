---
sidebar_position: 2
slug: /python-multi-agent
---

# Python Multi-Agent

Use `MultiAgentRuntime` when you want multiple Python `Agent` instances to discover peers and delegate work inside one shared runtime.

Enki injects `discover_agents` and `delegate_task` automatically so your coordinator agent can inspect available members and hand work off to a specialist.

```python
from enki_py import Agent, MultiAgentMember, MultiAgentRuntime

coordinator = Agent(
    "ollama::qwen3.5:latest",
    name="Coordinator",
    instructions="Delegate research work when appropriate.",
)

researcher = Agent(
    "ollama::qwen3.5:latest",
    name="Researcher",
    instructions="Handle research tasks and return concise answers.",
)

runtime = MultiAgentRuntime(
    [
        MultiAgentMember(
            agent_id="coordinator",
            agent=coordinator,
            capabilities=["planning", "orchestration"],
        ),
        MultiAgentMember(
            agent_id="researcher",
            agent=researcher,
            capabilities=["research"],
        ),
    ]
)

result = runtime.process_sync(
    "coordinator",
    "Find the answer and delegate research work if needed.",
)
print(result.output)
```

Repository examples:

- `example/enki-py/simple_multi_agent.py`
- `example/enki-py/multi_agent_with_memory_and_tools.py`
