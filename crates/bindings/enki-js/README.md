# `@getenki/ai`

Node.js bindings for Enki's Rust agent runtime, published as a native package via `napi-rs`.

## Install

```bash
npm install @getenki/ai
```

The package ships prebuilt native binaries for:

- Windows x64 and arm64
- macOS x64 and arm64
- Linux x64 and arm64 (GNU libc)

## What It Exports

The current package surface is:

- `NativeEnkiAgent`
- `NativeMultiAgentRuntime`
- `JsAgentStatus`
- `JsMemoryKind`
- `JsMemoryModule`
- `JsMemoryEntry`
- `JsAgentCard`
- `JsAgentRunResult`
- `JsExecutionStep`

`NativeEnkiAgent` is the main entrypoint. It can be created in four modes:

- `new(...)` for a plain agent
- `NativeEnkiAgent.withTools(...)`
- `NativeEnkiAgent.withMemory(...)`
- `NativeEnkiAgent.withToolsAndMemory(...)`

`NativeMultiAgentRuntime` supports:

- `new(...)`
- `process(...)`
- `processWithTrace(...)`
- `registry(...)`
- `discover(...)`

## Basic Agent

Use the constructor when you only need a session-based agent backed by the native runtime.

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

TypeScript version:

```ts
import { NativeEnkiAgent } from '@getenki/ai'

const agent = new NativeEnkiAgent(
  'Assistant',
  'Answer clearly and keep responses short.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
)

const output = await agent.run('session-1', 'Explain what this project does.')
console.log(output)
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

Example:

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

TypeScript tool example:

```ts
import { NativeEnkiAgent } from '@getenki/ai'

type SumArgs = {
  a?: number
  b?: number
}

type ExampleTool = {
  id: string
  description: string
  inputSchema: Record<string, unknown>
  execute: (inputJson: string, contextJson: string) => string
}

const tools: ExampleTool[] = [
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
    execute: (inputJson: string, contextJson: string): string => {
      const args = inputJson ? (JSON.parse(inputJson) as SumArgs) : {}
      const ctx = contextJson
        ? (JSON.parse(contextJson) as { workspaceDir?: string })
        : {}
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

Instead of putting `execute` on every tool, you can pass a shared `toolHandler` as the final argument to `withTools(...)` or `withToolsAndMemory(...)`. The shared handler receives:

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

```ts
type JsMemoryEntry = {
  key: string
  content: string
  kind: JsMemoryKind
  relevance: number
  timestampNs: string
}
```

Supported memory kinds:

- `JsMemoryKind.RecentMessage`
- `JsMemoryKind.Summary`
- `JsMemoryKind.Entity`
- `JsMemoryKind.Preference`

TypeScript memory typing example:

```ts
import {
  JsMemoryKind,
  type JsMemoryEntry,
  type JsMemoryModule,
} from '@getenki/ai'

const memories: JsMemoryModule[] = [{ name: 'example-memory' }]
const memoryStore = new Map<string, JsMemoryEntry[]>()

function memoryKey(memoryName: string, sessionId: string): string {
  return `${memoryName}:${sessionId}`
}

function getMemoryEntries(memoryName: string, sessionId: string): JsMemoryEntry[] {
  const key = memoryKey(memoryName, sessionId)
  const existing = memoryStore.get(key)
  if (existing) {
    return existing
  }

  const empty: JsMemoryEntry[] = []
  memoryStore.set(key, empty)
  return empty
}

const recordHandler = (
  memoryName: string,
  sessionId: string,
  userMsg: string,
  assistantMsg: string,
): void => {
  const entries = getMemoryEntries(memoryName, sessionId)
  entries.push({
    key: `entry-${entries.length + 1}`,
    content: `User: ${userMsg}\nAssistant: ${assistantMsg}`,
    kind: JsMemoryKind.RecentMessage,
    relevance: 1,
    timestampNs: `${Date.now() * 1000000}`,
  })
}
```

## Tools And Memory Example

The repository examples in [`example/basic-js/index.js`](/I:/projects/enki/core-next/example/basic-js/index.js) and [`example/basic-ts/index.ts`](/I:/projects/enki/core-next/example/basic-ts/index.ts) use `NativeEnkiAgent.withToolsAndMemory(...)` with:

- a `calculate_sum` tool
- a `get_today` tool
- an in-memory `Map` for session memory storage

There are also richer examples in [`example/basic-js/multi-agent-tools-memory.js`](/I:/projects/enki/core-next/example/basic-js/multi-agent-tools-memory.js) and [`example/basic-ts/multi-agent-tools-memory.ts`](/I:/projects/enki/core-next/example/basic-ts/multi-agent-tools-memory.ts). Those examples show:

- a researcher agent with a custom `lookup_example_topics` tool
- a coordinator agent consuming a researcher handoff via its own tool
- shared in-process memory across both agents
- progress logging so long-running model calls do not look stalled

Minimal JavaScript version:

```js
const { JsMemoryKind, NativeEnkiAgent } = require('@getenki/ai')

const tools = [
  {
    id: 'get_today',
    description: 'Return the current local date in ISO format.',
    inputSchema: { type: 'object', properties: {} },
    execute: () => JSON.stringify({ today: new Date().toISOString().slice(0, 10) }),
  },
]

const memories = [{ name: 'example-memory' }]
const memoryStore = new Map()

function memoryKey(memoryName, sessionId) {
  return `${memoryName}:${sessionId}`
}

const agent = NativeEnkiAgent.withToolsAndMemory(
  'Basic JS Agent',
  'Answer clearly and keep responses short.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
  tools,
  null,
  memories,
  (memoryName, sessionId, userMsg, assistantMsg) => {
    const key = memoryKey(memoryName, sessionId)
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
    const entries = memoryStore.get(memoryKey(memoryName, sessionId)) ?? []
    return entries.filter((entry) => entry.content.includes(query)).slice(-maxEntries)
  },
  (memoryName, sessionId) => {
    memoryStore.delete(memoryKey(memoryName, sessionId))
  },
  () => {},
)
```

Minimal TypeScript version:

```ts
import {
  JsMemoryKind,
  type JsMemoryEntry,
  type JsMemoryModule,
  NativeEnkiAgent,
} from '@getenki/ai'

type ExampleTool = {
  id: string
  description: string
  inputSchema: Record<string, unknown>
  execute: (inputJson: string, contextJson: string) => string
}

const tools: ExampleTool[] = [
  {
    id: 'get_today',
    description: 'Return the current local date in ISO format.',
    inputSchema: { type: 'object', properties: {} },
    execute: (): string =>
      JSON.stringify({ today: new Date().toISOString().slice(0, 10) }),
  },
]

const memories: JsMemoryModule[] = [{ name: 'example-memory' }]
const memoryStore = new Map<string, JsMemoryEntry[]>()

const agent = NativeEnkiAgent.withToolsAndMemory(
  'Basic TS Agent',
  'Answer clearly and keep responses short.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
  tools,
  null,
  memories,
  (memoryName: string, sessionId: string, userMsg: string, assistantMsg: string): void => {
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
  (memoryName: string, sessionId: string, query: string, maxEntries: number): JsMemoryEntry[] => {
    const entries = memoryStore.get(`${memoryName}:${sessionId}`) ?? []
    return entries.filter((entry) => entry.content.includes(query)).slice(-maxEntries)
  },
  (memoryName: string, sessionId: string): void => {
    memoryStore.delete(`${memoryName}:${sessionId}`)
  },
  (): void => {},
)
```

## Running The Examples

JavaScript example:

```bash
cd example/basic-js
npm install
npm start
npm run start:multi-agent-tools-memory
```

TypeScript example:

```bash
cd example/basic-ts
npm install
npm start
npm run start:multi-agent-tools-memory
```

The checked-in examples currently hardcode `ollama::qwen3.5:latest` as the model, so make sure that model is available in your local provider before running them.

## Development

From [`crates/bindings/enki-js`](/I:/projects/enki/core-next/crates/bindings/enki-js):

```bash
npm install
npm run build
npm test
```

Useful scripts:

- `npm run build`: build the native addon in release mode
- `npm run build:debug`: build without release optimizations
- `npm test`: run the AVA test suite
- `npm run lint`: run `oxlint`
- `npm run format`: run Prettier, `cargo fmt`, and `taplo format`
