# enki-py

Python bindings for Enki's Rust agent runtime.

`enki-py` exposes two layers:

- A high-level Python API built around `Agent` and `MultiAgentRuntime`
- The lower-level native bindings generated from the Rust runtime, including workflow APIs

The package is designed for Python-first usage without giving up the native runtime features that already exist in the Rust core.

## Requirements

- Python `>=3.8`
- A supported model string such as `openai::gpt-4o`, `anthropic::claude-sonnet-4-6`, or `ollama::qwen3.5:latest`

## Install

Published package:

```bash
pip install enki-py
```

With the built-in LiteLLM adapter:

```bash
pip install "enki-py[litellm]"
```

Using `uv`:

```bash
uv add enki-py
uv add "enki-py[litellm]"
```

## What It Exports

### High-level Python API

The main Python-facing exports are:

- `Agent`
- `AgentLoopRequest`
- `AgentLoopResult`
- `AgentRunResult`
- `ExecutionStep`
- `Tool`
- `RunContext`
- `MemoryBackend`
- `MemoryModule`
- `MemoryEntry`
- `MemoryKind`
- `MultiAgentRuntime`
- `MultiAgentMember`
- `AgentCard`
- `LiteLlmProvider`
- `LlmProviderBackend`

### Low-level native bindings

The generated module is also re-exported, so lower-level runtime types remain available when you want to work closer to the Rust layer. Commonly used native types include:

- `EnkiAgent`
- `EnkiWorkflowRuntime`
- `EnkiTool` and `EnkiToolSpec`
- `EnkiMemoryEntry`
- `EnkiMemoryModule`

## Agent API

`Agent` is the main Python entrypoint. Its constructor supports:

- `model`: required provider and model string
- `deps_type`: optional dependency type used with `RunContext`
- `instructions`: system guidance / prompt preamble
- `agentic_loop`: optional prompt-level loop instructions
- `agent_loop_handler`: optional Python callback that overrides the runtime loop
- `name`: display name for the agent
- `max_iterations`: iteration cap for the runtime loop
- `workspace_home`: optional workspace root for persisted runtime state
- `tools`: optional list of pre-registered `Tool` objects
- `memories`: optional list of `MemoryModule` objects
- `llm`: optional Python-side LLM backend or callback

Main methods:

- `run(...)`: async single-agent execution
- `run_sync(...)`: sync wrapper around `run(...)`
- `set_agent_loop_handler(...)`
- `clear_agent_loop_handler(...)`
- `tool_plain(...)`: decorator for plain Python tools
- `tool(...)`: decorator for tools that receive `RunContext`
- `register_tool(...)`
- `register_memory(...)`
- `as_workflow_agent(...)`: converts the wrapper into a workflow-configured low-level agent

The result of `run(...)` or `run_sync(...)` is `AgentRunResult`, which contains:

- `output`: final model response text
- `steps`: `ExecutionStep` entries describing runtime progress

Both `run(...)` and `run_sync(...)` accept:

- `session_id`: optional explicit session id
- `deps`: optional dependency object passed into `RunContext`
- `on_step`: optional callback for streaming step visibility

## Basic Agent

Synchronous example:

```python
import uuid

from enki_py import Agent


agent = Agent(
    "anthropic::claude-sonnet-4-6",
    name="Simple Agent",
    instructions="Answer clearly and keep responses short.",
)

result = agent.run_sync(
    "Explain what this Enki Python example demonstrates.",
    session_id=f"simple-agent-{uuid.uuid4()}",
)

print(result.output)
for step in result.steps:
    print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
```

Async version:

```python
import asyncio

from enki_py import Agent


async def main() -> None:
    agent = Agent(
        "ollama::qwen3.5:latest",
        name="Async Agent",
        instructions="Answer clearly and keep responses short.",
    )

    result = await agent.run("What does enki-py provide?")
    print(result.output)


asyncio.run(main())
```

## Tools

The high-level wrapper lets you register Python functions directly as tools.

### `@agent.tool_plain`

Use this when the tool only needs its declared arguments:

```python
from enki_py import Agent


agent = Agent(
    "ollama::qwen3.5:latest",
    name="Researcher",
    instructions="Use tools when they help answer factual questions.",
)


@agent.tool_plain
def lookup_example_topics(topic: str) -> str:
    """Return a canned fact for an example topic."""
    facts = {
        "memory": "Memory lets the agent persist and recall useful session context.",
        "tools": "Tools let the agent call Python functions to fetch or compute structured results.",
        "multi-agent": "Multi-agent runtimes let a coordinator route work to specialized agents.",
    }
    return facts.get(topic.lower(), f"No prepared fact exists for '{topic}'.")
```

### `@agent.tool`

Use this when the tool should receive a `RunContext` with typed dependencies:

```python
from dataclasses import dataclass

from enki_py import Agent, RunContext


@dataclass
class AppDeps:
    project_name: str


agent = Agent(
    "ollama::qwen3.5:latest",
    deps_type=AppDeps,
    name="Context Agent",
    instructions="Use the provided context when it helps answer.",
)


@agent.tool
def project_name(ctx: RunContext[AppDeps]) -> str:
    """Return the active project name from runtime dependencies."""
    return ctx.deps.project_name
```

Tool schemas are inferred from Python signatures and type annotations. Required parameters come from arguments without defaults, and optional parameters come from arguments with defaults.

If you need lower-level control, you can also build `Tool(...)` values directly and register them with `register_tool(...)`.

## Memory

`enki-py` supports Python-defined memory modules through `MemoryBackend` and `MemoryModule`.

Implement `MemoryBackend` when you want custom record and recall behavior:

```python
from dataclasses import dataclass, field
from time import time_ns

from enki_py import MemoryBackend, MemoryEntry, MemoryKind


@dataclass
class SessionMemory(MemoryBackend):
    name: str = "session_memory"
    _entries: dict[str, list[MemoryEntry]] = field(default_factory=dict)

    def record(self, session_id: str, user_msg: str, assistant_msg: str) -> None:
        entries = self._entries.setdefault(session_id, [])
        entries.append(
            MemoryEntry(
                key=f"{session_id}-{len(entries)}",
                content=f"User: {user_msg}\nAssistant: {assistant_msg}",
                kind=MemoryKind.RECENT_MESSAGE,
                relevance=1.0,
                timestamp_ns=time_ns(),
            )
        )

    def recall(self, session_id: str, query: str, max_entries: int) -> list[MemoryEntry]:
        entries = self._entries.get(session_id, [])
        return entries[-max_entries:]

    def flush(self, session_id: str) -> None:
        return None
```

Register it like this:

```python
memory = SessionMemory()
agent = Agent(
    "ollama::qwen3.5:latest",
    memories=[memory.as_memory_module()],
)
```

Supported memory kinds:

- `MemoryKind.RECENT_MESSAGE`
- `MemoryKind.SUMMARY`
- `MemoryKind.ENTITY`
- `MemoryKind.PREFERENCE`

Memory callbacks may be synchronous or `async def` coroutines.

## Multi-Agent Runtime

`MultiAgentRuntime` wires multiple `Agent` instances together and installs the coordination tools used for agent discovery and delegation.

Available methods:

- `registry()`
- `discover(...)`
- `process(...)`
- `process_sync(...)`

The runtime automatically installs two reserved tools on member agents:

- `discover_agents`
- `delegate_task`

Minimal example:

```python
from enki_py import Agent, MultiAgentMember, MultiAgentRuntime


coordinator = Agent(
    "ollama::qwen3.5:latest",
    name="Coordinator",
    instructions=(
        "Use discover_agents first. "
        "Delegate research work to the researcher with delegate_task."
    ),
)

researcher = Agent(
    "ollama::qwen3.5:latest",
    name="Researcher",
    instructions="Answer delegated questions clearly and briefly.",
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
    "Use discover_agents first, then delegate to the researcher.",
    session_id="simple-multi-agent-example",
)

print(result.output)
```

Repository examples:

- [`example/enki-py/simple_multi_agent.py`](../../../example/enki-py/simple_multi_agent.py)
- [`example/enki-py/multi_agent_with_memory_and_tools.py`](../../../example/enki-py/multi_agent_with_memory_and_tools.py)

## Workflow Runtime

For workflow execution, the lower-level `EnkiWorkflowRuntime` is the main entrypoint. A common pattern is:

1. Create Python `Agent` wrappers
2. Convert them with `as_workflow_agent(agent_id=..., capabilities=[...])`
3. Provide JSON task definitions and JSON workflow definitions
4. Start and inspect runs through the workflow runtime

Methods used in the checked-in Python examples include:

- `list_workflows_json()`
- `list_runs_json()`
- `inspect_json(run_id)`
- `start_json(payload_json)`
- `resume_json(run_id)`
- `submit_intervention_json(run_id, intervention_id, response)`

Workflow example:

```python
import asyncio
import json
from pathlib import Path

import enki_py


WORKSPACE_HOME = Path("./example/enki-py/.enki-workflow")
MODEL = "ollama::qwen3.5:latest"


async def main() -> None:
    researcher = enki_py.Agent(
        MODEL,
        name="Researcher",
        instructions="Return short factual notes.",
        workspace_home=str(WORKSPACE_HOME),
    )
    writer = enki_py.Agent(
        MODEL,
        name="Writer",
        instructions="Turn notes into a concise summary.",
        workspace_home=str(WORKSPACE_HOME),
    )

    runtime = enki_py.EnkiWorkflowRuntime(
        agents=[
            researcher.as_workflow_agent(agent_id="researcher", capabilities=["research"]),
            writer.as_workflow_agent(agent_id="writer", capabilities=["writing"]),
        ],
        tasks_json=[],
        workflows_json=[],
        workspace_home=str(WORKSPACE_HOME),
    )

    print(json.loads(await runtime.list_workflows_json()))


asyncio.run(main())
```

For a complete runnable version with task and edge definitions, see [`example/enki-py/agent_workflow.py`](../../../example/enki-py/agent_workflow.py).

## Human Intervention

Workflow runs can pause for human input and then resume from persisted state.

Two built-in patterns are demonstrated in the repository examples:

- `human_gate` workflow nodes that pause and wait for a human response
- task nodes with `failure_policy: "pause_for_intervention"` that convert a failure into a human resolution step

The runtime interaction loop is:

1. `start_json(...)` starts the workflow and may return a paused response
2. `inspect_json(run_id)` exposes `pending_interventions`
3. `submit_intervention_json(run_id, intervention_id, response)` resolves the intervention
4. `resume_json(run_id)` continues the persisted run

See [`example/enki-py/human_intervention_workflow.py`](../../../example/enki-py/human_intervention_workflow.py).

## Custom LLM Providers

You can keep model execution on the Python side by passing either:

- an instance of `LlmProviderBackend`
- a compatible Python callable matching the LLM completion shape

The built-in `LiteLlmProvider` is the default adapter for LiteLLM-backed usage. It is useful when you want:

- provider-specific configuration in Python
- LiteLLM-managed credentials or routing
- a Python extension point without changing Rust code

Notes from the current implementation:

- `LiteLlmProvider` raises a clear error if `litellm` is not installed
- for Ollama, the default base URL comes from `OLLAMA_URL` or falls back to `http://127.0.0.1:11434`
- `ENKI_LITELLM_TIMEOUT` controls the request timeout used by the adapter
- Ollama tool calling is only forwarded when `ENKI_OLLAMA_TOOLS` is truthy

The repository workflow example also includes a custom `OllamaProvider` implementation in [`example/enki-py/agent_workflow.py`](../../../example/enki-py/agent_workflow.py).

## Custom Agentic Loop

There are two different customization levels.

Use `agentic_loop=` when you want to keep the normal Rust runtime loop but replace the default loop instructions that the model sees:

```python
from enki_py import Agent


agent = Agent(
    "anthropic::claude-sonnet-4-6",
    name="Custom Agentic Loop",
    instructions="Answer clearly and keep responses short.",
    agentic_loop=(
        "1. Understand the user request.\n"
        "2. Decide whether a tool is necessary before acting.\n"
        "3. If you use a tool, summarize what you learned.\n"
        "4. Verify that the answer is complete.\n"
        "5. Return the final response."
    ),
)
```

Use `agent_loop_handler=` when you want Python to own the turn-by-turn control flow:

```python
from enki_py import Agent, AgentLoopRequest, AgentLoopResult, ExecutionStep


def custom_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    return AgentLoopResult(
        output=f"Handled in Python for: {request.user_message}",
        steps=[
            ExecutionStep(
                index=1,
                phase="Custom",
                kind="final",
                detail="Returned a final answer from Python",
            )
        ],
    )


agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    agent_loop_handler=custom_loop,
)
```

The loop request includes:

- `session_id`
- `user_message`
- `system_prompt`
- `messages`
- `tools`
- `agent_dir`
- `workspace_dir`
- `sessions_dir`
- `model`
- `max_iterations`
- `deps`

You can also update the loop handler after construction with:

- `agent.set_agent_loop_handler(handler)`
- `agent.clear_agent_loop_handler()`

Repository examples:

- [`example/enki-py/custom_agentic_loop.py`](../../../example/enki-py/custom_agentic_loop.py)
- [`example/enki-py/react_custom_agentic_loop.py`](../../../example/enki-py/react_custom_agentic_loop.py)
- [`example/enki-py/compare_agent_loops.py`](../../../example/enki-py/compare_agent_loops.py)

## Running The Examples

After building the local package with `maturin develop`, you can run the checked-in examples from the repository root:

```powershell
python example\enki-py\simple_agent.py
python example\enki-py\simple_agent_ollama.py
python example\enki-py\simple_multi_agent.py
python example\enki-py\multi_agent_with_memory_and_tools.py
python example\enki-py\agent_workflow.py
python example\enki-py\human_intervention_workflow.py
python example\enki-py\custom_agentic_loop.py
python example\enki-py\react_custom_agentic_loop.py
python example\enki-py\compare_agent_loops.py
```

Model notes:

- `simple_agent.py` uses `anthropic::claude-sonnet-4-6`
- `simple_agent_ollama.py` uses `ollama::qwen3.5`
- several other checked-in examples use `ollama::qwen3.5:latest`
- `agent_workflow.py` reads `ENKI_MODEL` and falls back to `ollama::qwen3.5:latest`

Make sure the selected provider and model are available in your local environment before running the examples.

## Development

From `crates/bindings/enki-py`:

```powershell
pip install maturin
maturin develop
pytest python/tests
```

Useful commands:

- `maturin develop`: build the Rust extension and install the package in editable form
- `pytest python/tests`: run the Python-side tests and examples under test coverage

## Project Layout

```text
crates/bindings/enki-py/
|-- Cargo.toml
|-- pyproject.toml
|-- python/enki_py/
|-- python/tests/
`-- src/
```

- `src/`: Rust UniFFI binding implementation
- `python/enki_py/`: Python wrapper and generated native module
- `python/tests/`: Python-side tests and usage coverage

## Related Docs

- [Repository README](../../../README.md)
- [Enki docs](https://docs.getenki.com)
- [Getting started](https://docs.getenki.com/docs/intro)
