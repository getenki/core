---
sidebar_position: 3
slug: /agent-design
---

# Agent Design

Enki's agent model is built around a strict Rust execution loop with language bindings layered on top. The design goal is simple: keep the control path deterministic, make tool execution composable, and let Python or JavaScript extend behavior without owning the runtime.

## Design goals

- Keep the execution loop predictable and observable
- Isolate concurrency and state management inside Rust
- Allow tools, memory backends, and LLM providers to be swapped without changing the core loop
- Support both single-agent and multi-agent runtimes with the same primitives
- Expose trace data cleanly to Python and JavaScript SDKs

## Core architecture

The runtime is split across three layers:

- `crates/core`: agent loop, memory, tool execution, registry, runtime, and provider abstractions
- `crates/bindings`: Python and Node.js bridges over the Rust core
- `crates/builder`: manifest-driven CLI for running and scaffolding local Enki projects

This split keeps execution semantics in one place while still giving SDK users a higher-level developer experience.

## Agent loop

The agent loop is implemented as a state machine rather than an open-ended prompt cycle. A typical run moves through phases such as:

- Understand
- Plan
- Act
- Observe
- Recover
- Finalize

The loop converts each model response into a directive:

- continue into the next phase
- execute a tool and observe the result
- retry through recovery when possible
- finalize with an output

This is what lets Enki enforce retry budgets, bound iterations, and emit consistent step traces.

## Custom loop instructions

The runtime also allows the default loop prompt to be overridden per agent without changing the Rust state machine itself. Enki parses a tagged block from the agent's instruction preamble:

```text
Keep responses concise.
<enki:agentic-loop>
- Think briefly.
- Call tools only after planning.
</enki:agentic-loop>
```

At runtime:

- text outside the block stays in the normal system prompt preamble
- text inside the block replaces Enki's default agentic loop instructions
- if the block is missing, empty, or malformed, Enki falls back to the default loop prompt

This gives SDK users a controlled way to tune how the model interprets the loop while still keeping iteration limits, phase transitions, retries, and tool execution inside the Rust runtime.

## Runtime primitives

Every agent is composed from the same runtime building blocks:

- `LlmProvider`: resolves model completions and tool-call responses
- `ToolRegistry` and `ToolExecutor`: expose callable functions to the agent
- `MemoryProvider` and `MemoryRouter`: manage persistent or scoped memory
- session state and workspace storage: preserve transcripts, task state, and artifacts across runs

Because these are abstractions, the same runtime can back local tools, custom memory stores, and SDK-specific provider adapters.

## Multi-agent design

The multi-agent runtime extends the same core loop instead of creating a second execution model. Agents register capabilities in an `AgentRegistry`, and the runtime injects intrinsic tools so agents can collaborate:

- discover other agents
- delegate work to another agent
- request human input when interactive execution is enabled

Delegation runs in isolated session contexts, which keeps agent state separate while allowing coordination inside one runtime.

## Language binding strategy

Python and JavaScript do not run the async engine directly on the host thread. Enki uses a worker-thread pattern:

- create the SDK-side agent
- spawn a dedicated Rust worker thread
- create an isolated Tokio runtime inside that worker
- exchange requests and callbacks over message channels

When a tool or memory backend is defined in Python or JavaScript, the Rust runtime pauses, crosses the FFI boundary, waits for the callback result, and then resumes the same core loop. This keeps the host SDK ergonomic without duplicating runtime logic.

## Observability

Each run produces structured execution steps that include phase, step kind, and detail. These events power:

- Python `on_step` callbacks and traced run results
- JavaScript traced execution helpers
- future monitoring and debugging workflows in the builder CLI

The practical benefit is that tool calls, retries, and finalization are visible as runtime events instead of being hidden inside a prompt transcript.

## Builder CLI relationship

The `enki` builder CLI sits above the runtime and turns project manifests into runnable agent systems. It does not replace the core agent design. It packages configuration, discovers environments, and launches the same underlying runtime for local development workflows.

## Example agents

These examples show different ways to combine the same Rust loop with different instruction styles and tool sets.

### Support triage agent

This pattern fits operational workflows where the model should gather context before making claims.

```python
from enki_py import Agent

agent = Agent(
    "openai::gpt-4o",
    name="Support Triage Agent",
    instructions=(
        "You triage incoming support issues. "
        "Classify severity, identify the likely subsystem, "
        "and recommend the next action for the support team. "
        "Use tools before making operational claims."
    ),
)


@agent.tool_plain
def lookup_runbook(service: str) -> str:
    """Return the internal runbook summary for a service."""
    runbooks = {
        "billing": "Check payment provider status, failed webhook retries, and recent invoice events.",
        "auth": "Check login error spikes, token expiry configuration, and identity provider health.",
        "api": "Check latency, deployment history, and upstream dependency failures.",
    }
    return runbooks.get(service.lower(), "No runbook entry found.")


@agent.tool_plain
def classify_severity(summary: str) -> str:
    """Return a simple severity label for a support issue."""
    text = summary.lower()
    if "all users" in text or "outage" in text:
        return "sev-1"
    if "payments failing" in text or "cannot login" in text:
        return "sev-2"
    return "sev-3"


result = agent.run_sync(
    "Customers report they cannot login after this morning's deployment.",
)

print(result.output)
```

### Research agent with custom loop instructions

This pattern is useful when the model should always plan first, then gather evidence before answering.

```python
from enki_py import Agent

agent = Agent(
    "openai::gpt-4o",
    name="Research Agent",
    instructions="""
You summarize technical topics for engineers.
<enki:agentic-loop>
- Restate the question internally before acting.
- Prefer collecting evidence with tools before answering.
- If sources conflict, call that out explicitly in the final response.
</enki:agentic-loop>
""",
)


@agent.tool_plain
def lookup_note(topic: str) -> str:
    """Return a short internal research note."""
    notes = {
        "tokio": "Tokio is an async runtime for Rust with task scheduling, IO, and timers.",
        "pyo3": "PyO3 exposes Rust types and functions to Python through CPython bindings.",
    }
    return notes.get(topic.lower(), "No note found.")


print(agent.run_sync("Summarize Tokio for a backend engineer.").output)
```

### File-oriented workspace agent

This pattern works well for agents that iterate over artifacts in the task workspace and produce a concrete output.

```python
from enki_py import Agent

agent = Agent(
    "openai::gpt-4o",
    name="Release Notes Agent",
    instructions=(
        "You prepare release notes from workspace artifacts. "
        "Read source material before drafting. "
        "Write the final markdown only when the summary is complete."
    ),
)


@agent.tool_plain
def list_changes() -> str:
    """Return a simplified changelog feed."""
    return "- Added multi-agent registry support.\n- Improved execution step tracing.\n- Fixed retry handling in recovery."


@agent.tool_plain
def write_release_notes(content: str) -> str:
    """Pretend to write release notes."""
    return f"release-notes.md updated with {len(content)} characters"


print(agent.run_sync("Draft release notes for the latest internal build.").output)
```

All three examples keep the same Enki runtime guarantees: the Rust loop owns phase transitions, retries, and step tracing, while the SDK layer defines agent behavior through instructions, tools, and callbacks.

## Related docs

- [Rust](/docs/rust)
- [Builder CLI](/docs/builder-cli)
- [Python](/docs/python)
- [JavaScript](/docs/javascript)
