# Enki Framework: Full Architecture Guide

This document is a comprehensive guide to the internal architecture of the Enki framework. Enki is designed as a polyglot multi-agent framework where the heavy lifting (execution loops, concurrency, state management) is written in Rust (`core`), while providing ergonomic bindings for high-level languages like Python and Node.js (`bindings`).

---

## 1. System Philosophy

The fundamental design philosophy of Enki is **"Rust for performance and determinism, SDKs for extensibility."**

By maintaining the control loop in Rust, we ensure:
- Extremely strict and predictable agent state machines.
- Safe concurrency models when executing multiple agents.
- Uniform observability (tracing) across all language targets.

The architecture is divided into three primary crates:
1. `crates/core`: The pure Rust engine containing all logical abstractions.
2. `crates/bindings`: FFI layers (`napi-rs` for Node, UniFFI for Python) wrapping the core.
3. `crates/builder`: A CLI that orchestrates local developer workflows (`enki.toml`).

```mermaid
graph TD
    CLI["Builder CLI (enki run)"] -->|"Reads enki.toml & Spawns"| Bindings
    
    subgraph Bindings ["Language SDKs (Python / Node.js)"]
        BuilderPY["enki_py.builder (Python)"]
        AgentJS["NativeEnkiAgent (JS)"]
        Tools["User Defined Tools"]
    end
    
    Bindings -->|"FFI (PyO3 / napi-rs)"| Core
    
    subgraph Core ["Rust Core (crates/core)"]
        AgentLoop["Agent State Machine"]
        Memory["Memory Providers"]
        LLM["LLM Integration"]
        Registry["Agent Registry"]
    end
    
    BuilderPY --> Tools
    AgentJS --> Tools
    AgentLoop --> LLM
    AgentLoop --> Memory
    AgentLoop --> Registry
```

---

## 2. Core Engine (`crates/core`)

### The State Machine: `AgentLoop`
At the very center of Enki is `DefaultAgentLoop` inside `core/src/agent/agent_loop.rs`. It does not trust the LLM implicitly. Instead, it processes execution as a strict State Machine defined by the `LoopPhase` enum.

```mermaid
stateDiagram-v2
    [*] --> Understand
    Understand --> Plan : Continue
    Plan --> Act : Continue
    Act --> Observe : Tool Called
    Observe --> Act : Continue
    Act --> Finalize : Final
    
    Understand --> Recover : Error
    Plan --> Recover : Error
    Act --> Recover : Error
    Observe --> Recover : Error
    
    Recover --> Understand : Retry
    Recover --> Finalize : Fatal Error
    
    Finalize --> [*]
```

**How it works:**
1. **Initialize**: A session context and `ExecutionState` (budget, retries) are loaded.
2. **Execute Turn**: The LLM is invoked. The resulting `StepOutcome` is translated by the loop into a `LoopDirective`.
3. **Handle Directive**:
   - `Continue(next_phase)`: Advances the state machine.
   - `Retry`: Increments `budget.retries`. If it exceeds limits, hard fails. Otherwise, falls back to `Recover` phase.
   - `Final`: Halts execution and commits memory.

### Pluggable Abstractions
The agent itself (`Agent`) is highly modular, depending on interfaces rather than implementations:
- **`LlmProvider`**: Abstraction over the LLM. Enki allows bindings to inject virtual LLM providers that route requests out to Python or Node.js logic.
- **`MemoryProvider` & `MemoryRouter`**: Pluggable long-term context.
- **`ToolRegistry` & `ToolExecutor`**: Centralized mapping of available functions.

### Observability
The core loop defines `ExecutionStep` instances that track index, phase, kind, and detail. These trigger `on_step` callbacks, bubble through the FFI boundary, and power the trace APIs exposed by Python (`on_step`, `AgentRunResult.steps`) and JavaScript (`runWithTrace`, `processWithTrace`).

---

## 3. Multi-Agent Ecosystem (`crates/core/src/runtime`)

Enki natively scales from a single `Agent` to a distributed `MultiAgentRuntime`.

### `AgentRegistry`
When the Multi-Agent runtime starts, every agent registers an `AgentCard` describing its ID, name, status (`Online/Offline/Busy`), and semantic capabilities (`code-gen`, `search`, etc.).

### Intrinsic Tools
The multi-agent runtime automatically injects foundational tools into all agents:
1. **`DiscoverAgentsTool`**: Plugs into the LLM, allowing it to dynamically query the registry.
2. **`DelegateTaskTool`**: Allows an agent to spin up an isolated session context targeted at another agent. The delegated task runs natively in its own thread/loop, safely isolated from the calling agent's context.
3. **`AskHumanTool`**: Lets an agent pause execution and request a human reply when the runtime is serving an interactive channel.

---

## 4. Bindings Architecture (`crates/bindings`)

The most complex and powerful part of Enki is how it exposes the Rust runtime to Garbage-Collected languages without compromising thread safety.

**Key implementations:**
- `enki-js` uses `napi-rs`.
- `enki-py` uses UniFFI-backed bindings plus a Python wrapper layer in `python/enki_py/agent.py`.

### The Tokio Worker Thread Pattern
Languages like JavaScript are strictly single-threaded (per isolate), and Python code frequently runs under an active event loop or the GIL. You cannot safely run the Rust async runtime directly on the host application's main thread.

To solve this, Enki uses the **Worker Thread Pattern**:
1. When an `EnkiAgent` is instantiated in JS/Python, Rust spawns a dedicated background OS thread.
2. Inside this thread, a completely fresh, isolated `tokio::runtime` is established.
3. A heavily protected `mpsc::channel` (Message Passing) is established between the host language thread and the Tokio runtime.

```mermaid
sequenceDiagram
    participant JS as Node.js / Python Main Thread
    participant FFI as FFI Boundary
    participant Rust as Rust Tokio Worker Thread
    participant LLM as Target LLM

    JS->>FFI: "new NativeEnkiAgent()"
    FFI->>Rust: "Spawn Thread & Tokio Runtime"
    JS->>FFI: "process(session, user_message)"
    FFI->>Rust: "Send RunRequest (mpsc channel)"
    
    loop Agent Loop execution
        Rust->>LLM: "complete_with_tools()"
        LLM-->>Rust: "Tool Call Directive"
        
        Rust->>FFI: "Dispatch Tool Callback"
        FFI->>JS: "Call user Python/JS tool func"
        JS-->>FFI: "Return Tool String result"
        FFI-->>Rust: "Resume execution loop"
    end
    
    Rust->>FFI: "Request Finished"
    FFI-->>JS: "Promise / Future Resolves"
```

### Trait Translation (Callbacks)
When an agent calls a tool or memory backend written in Python or JavaScript, the Rust engine hits an FFI wall. Enki solves this via bridge implementations:
- Rust bridge types serialize tool context, memory payloads, and execution steps.
- The callback is routed back into Node.js or Python.
- The Rust loop awaits the response and resumes without changing the core execution model.

For human-in-the-loop execution, the runtime also injects an `AskHumanFn` into tool context. The `ask_human` intrinsic sends a query over an internal channel, the serving runtime emits a human-request event, and the agent resumes once a reply is posted back on that channel.

---

## 5. The Builder CLI (`crates/builder`)

The CLI (`enki.toml`) is the developer interface that knits everything together into an ergonomic local environment.

### Project Composition
`manifest.rs` maps a desired environment configuration into a deployable schema.

### Dynamic Embedded Execution
To achieve zero-friction usage (`enki run`), the Rust CLI acts as an orchestrator. If the project type is Python:
1. `project_runtime.rs` automatically walks the tree to locate a local `.venv` or global system python.
2. It executes a pre-built internal stub (`enki_py.builder`).
3. It passes the **entire manifest configuration** (Agents, Tools logic, models) via a highly compressed, serialized CLI argument vector to avoid writing temp files.

The builder now also exposes:
- `enki monitor` for manifest inspection
- `enki test` for quick connectivity checks
- `enki join` for interactive human-in-the-loop sessions
- `enki tool new` and `enki agent add` for scaffolding project assets

### Introspection Magic
Inside `enki_py.builder`, Python uses `importlib` and `inspect.signature` to parse user-defined tools. It automatically detects if a tool needs standard `args` or deep Enki `Context`, wraps it transparently, attaches it to the FFI `Agent` layer, and starts the internal stream.

---

## Summary

`enki` achieves high performance and reliability through a strictly typed Rust state machine, while offering infinite extensibility through safe FFI threading models. The Builder CLI completes the package by abstracting these complex integration points entirely away from the end developer.
