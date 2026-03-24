---
sidebar_position: 2
slug: /typescript
---

# TypeScript

Use `@getenki/ai` from TypeScript when you want typed access to Enki's native Node.js bindings.

The generated declarations currently expose:

- `NativeEnkiAgent`
- `NativeMultiAgentRuntime`
- `JsAgentStatus`
- `JsAgentCard`
- `JsMemoryKind`
- `JsMemoryModule`
- `JsMemoryEntry`
- `JsMultiAgentMember`

## Install

```bash
npm install @getenki/ai
```

## Basic agent

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

## Typed tools

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

## Typed memory

```ts
import {
  JsMemoryKind,
  type JsMemoryEntry,
  type JsMemoryModule,
  NativeEnkiAgent,
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

const agent = NativeEnkiAgent.withToolsAndMemory(
  'Basic TS Agent',
  'Answer clearly and keep responses short.',
  'ollama::qwen3.5:latest',
  20,
  process.cwd(),
  [],
  null,
  memories,
  (memoryName: string, sessionId: string, userMsg: string, assistantMsg: string): void => {
    const entries = getMemoryEntries(memoryName, sessionId)
    entries.push({
      key: `entry-${entries.length + 1}`,
      content: `User: ${userMsg}\nAssistant: ${assistantMsg}`,
      kind: JsMemoryKind.RecentMessage,
      relevance: 1,
      timestampNs: `${Date.now() * 1000000}`,
    })
  },
  (memoryName: string, sessionId: string, query: string, maxEntries: number): JsMemoryEntry[] => {
    const entries = getMemoryEntries(memoryName, sessionId)
    return entries.filter((entry) => entry.content.includes(query)).slice(-maxEntries)
  },
  (memoryName: string, sessionId: string): void => {
    memoryStore.delete(memoryKey(memoryName, sessionId))
  },
  (): void => {},
)
```

For typed multi-agent orchestration, see [TypeScript Multi-Agent](/docs/typescript-multi-agent).
