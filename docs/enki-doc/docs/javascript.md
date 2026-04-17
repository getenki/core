---
sidebar_position: 1
slug: /javascript
---

# JavaScript

`@getenki/ai` is the JavaScript package for Enki. It exposes native Node.js bindings built from the Rust runtime with `napi-rs`.

This package is the current JavaScript surface. We are not publishing a WASM binding in this docs set.

## What it exposes

- `NativeEnkiAgent`
- `NativeMultiAgentRuntime`
- `NativeWorkflowRuntime`
- `JsAgentRunResult`
- `JsExecutionStep`
- `JsAgentStatus`
- `JsAgentCard`
- `JsMemoryKind`
- `JsMemoryModule`
- `JsMemoryEntry`
- `JsMultiAgentMember`

`NativeEnkiAgent` can be created in four modes:

- `new NativeEnkiAgent(...)`
- `NativeEnkiAgent.withTools(...)`
- `NativeEnkiAgent.withMemory(...)`
- `NativeEnkiAgent.withToolsAndMemory(...)`

For traced runs, the package also exposes:

- `agent.runWithTrace(sessionId, userMessage)`
- `runtime.processWithTrace(agentId, sessionId, userMessage)`

## Install

```bash
npm install @getenki/ai
```

The package ships prebuilt native binaries for:

- Windows x64 and arm64
- macOS x64 and arm64
- Linux x64 and arm64 using GNU libc

## Basic agent

```js
const { NativeEnkiAgent } = require('@getenki/ai')

async function main() {
  const agent = new NativeEnkiAgent(
    'Assistant',
    'Answer clearly and keep responses short.',
    'ollama::qwen3.5:latest',
    20,
    process.cwd(),
  )

  const output = await agent.run('session-1', 'Explain what this project does.')
  console.log(output)
}

main().catch(console.error)
```

If you need execution steps, use `runWithTrace(...)` instead of `run(...)`:

```js
const { NativeEnkiAgent } = require('@getenki/ai')

async function main() {
  const agent = new NativeEnkiAgent(
    'Assistant',
    'Answer clearly and keep responses short.',
    'ollama::qwen3.5:latest',
    20,
    process.cwd(),
  )

  const result = await agent.runWithTrace('session-1', 'Explain what this project does.')
  console.log(result.output)
  console.log(result.steps)
}
```

Constructor arguments:

- `name?: string`
- `systemPromptPreamble?: string`
- `model?: string`
- `maxIterations?: number`
- `workspaceHome?: string`

If omitted, the runtime falls back to built-in defaults for name, prompt, and max iterations.

## Tools

Tools can be attached with `NativeEnkiAgent.withTools(...)`. Each tool object must provide:

- `id` or `name`
- `description`
- one of `inputSchema`, `inputSchemaJson`, `parameters`, or `parametersJson`
- either `execute(inputJson, contextJson)` or a shared `toolHandler`

```js
const { NativeEnkiAgent } = require('@getenki/ai')

const tools = [
  {
    id: 'calculate_sum',
    description: 'Add two numbers and return a short text result.',
    inputSchema: {
      type: 'object',
      properties: {
        a: { type: 'number' },
        b: { type: 'number' },
      },
      required: ['a', 'b'],
    },
    execute: (inputJson, contextJson) => {
      const args = inputJson ? JSON.parse(inputJson) : {}
      const ctx = contextJson ? JSON.parse(contextJson) : {}
      const result = Number(args.a) + Number(args.b)

      return JSON.stringify({
        result,
        workspaceDir: ctx.workspaceDir,
        text: `${args.a} + ${args.b} = ${result}`,
      })
    },
  },
]

const agent = NativeEnkiAgent.withTools(
  'Tool Agent',
  'Use tools when they help.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
  tools,
  null,
)
```

Per-tool `execute` receives:

- `inputJson`: serialized tool arguments
- `contextJson`: serialized runtime context with `agentDir`, `workspaceDir`, and `sessionsDir`

You can also pass a shared `toolHandler` to `withTools(...)` or `withToolsAndMemory(...)`. That callback receives:

- `toolName`
- `inputJson`
- `agentDir`
- `workspaceDir`
- `sessionsDir`

## Memory

Memory modules are plain objects:

```js
const memories = [{ name: 'example-memory' }]
```

When using `withMemory(...)` or `withToolsAndMemory(...)`, you supply four callbacks:

- `recordHandler(memoryName, sessionId, userMsg, assistantMsg)`
- `recallHandler(memoryName, sessionId, query, maxEntries)`
- `flushHandler(memoryName, sessionId)`
- `consolidateHandler(memoryName, sessionId)`

`recallHandler` must return an array of `JsMemoryEntry` objects:

```js
const { JsMemoryKind, NativeEnkiAgent } = require('@getenki/ai')

const memories = [{ name: 'example-memory' }]
const memoryStore = new Map()

const agent = NativeEnkiAgent.withMemory(
  'Memory Agent',
  'Answer clearly and keep responses short.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
  memories,
  (memoryName, sessionId, userMsg, assistantMsg) => {
    const key = `${memoryName}:${sessionId}`
    const entries = memoryStore.get(key) ?? []
    entries.push({
      key: `entry-${entries.length + 1}`,
      content: `User: ${userMsg}\nAssistant: ${assistantMsg}`,
      kind: JsMemoryKind.RecentMessage,
      relevance: 1,
      timestampNs: `${Date.now() * 1000000}`,
    })
    memoryStore.set(key, entries)
  },
  (memoryName, sessionId, query, maxEntries) => {
    const key = `${memoryName}:${sessionId}`
    const entries = memoryStore.get(key) ?? []
    return entries.filter((entry) => entry.content.includes(query)).slice(-maxEntries)
  },
  (memoryName, sessionId) => {
    memoryStore.delete(`${memoryName}:${sessionId}`)
  },
  () => {},
)
```

Supported memory kinds:

- `JsMemoryKind.RecentMessage`
- `JsMemoryKind.Summary`
- `JsMemoryKind.Entity`
- `JsMemoryKind.Preference`

For multi-agent orchestration from Node.js, see [JavaScript Multi-Agent](/docs/javascript-multi-agent).

## Multi-agent runtime

`NativeMultiAgentRuntime` is the Rust-backed registry and delegation runtime exposed to JavaScript.

Current methods:

- `process(agentId, sessionId, userMessage)`
- `processWithTrace(agentId, sessionId, userMessage)`
- `registry()`
- `discover(capability?, status?)`

## Development

From `crates/bindings/enki-js`:

```bash
npm install
npm run build
npm test
```

## Related docs

- [JavaScript Workflow](/docs/javascript-workflow)
- [TypeScript](/docs/typescript)
- [Builder CLI](/docs/builder-cli)