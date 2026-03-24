---
sidebar_position: 3
slug: /typescript-multi-agent
---

# TypeScript Multi-Agent

Use `NativeMultiAgentRuntime` when you want typed agent orchestration, capability discovery, and delegated execution from TypeScript.

```ts
import {
  JsAgentStatus,
  NativeMultiAgentRuntime,
  type JsAgentCard,
  type JsMultiAgentMember,
} from '@getenki/ai'

const members: JsMultiAgentMember[] = [
  {
    agentId: 'coordinator',
    name: 'Coordinator',
    systemPromptPreamble: 'Use discover_agents before delegating work.',
    model: 'ollama::qwen3.5:latest',
    maxIterations: 20,
    capabilities: ['planning', 'orchestration'],
  },
  {
    agentId: 'researcher',
    name: 'Researcher',
    systemPromptPreamble: 'Handle delegated research tasks clearly and briefly.',
    model: 'ollama::qwen3.5:latest',
    maxIterations: 20,
    capabilities: ['research', 'analysis'],
  },
]

const runtime = new NativeMultiAgentRuntime(members, process.cwd())

const cards = (await runtime.discover('research', JsAgentStatus.Online)) as JsAgentCard[]
console.log(cards)

const result = await runtime.process(
  'coordinator',
  'basic-ts-multi-agent-session',
  'Use discover_agents first, then delegate this question to the researcher.',
)
console.log(result)
```

Repository examples:

- `example/basic-js/index.js`
- `example/basic-ts/index.ts`
