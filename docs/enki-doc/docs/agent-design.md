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

## Example agent use case

One practical use case is a support triage agent that classifies an incoming issue, checks internal runbooks, and proposes the next action for the human team.

In this design:

- the agent owns the conversation and response format
- tools fetch structured operational context
- memory can preserve recent customer incidents across the same session
- execution steps make it easy to audit why the recommendation was produced

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

This kind of agent is a good fit for Enki because the reasoning loop stays in Rust, while the domain-specific logic stays in normal Python tools that your team can update quickly.

## Related docs

- [Rust](/docs/rust)
- [Builder CLI](/docs/builder-cli)
- [Python](/docs/python)
- [JavaScript](/docs/javascript)
