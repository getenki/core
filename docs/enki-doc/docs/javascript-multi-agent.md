---
sidebar_position: 2
slug: /javascript-multi-agent
---

# JavaScript Multi-Agent

Use `NativeMultiAgentRuntime` when you want a registry of specialized agents that can discover and delegate work to each other from Node.js.

```js
const { JsAgentStatus, NativeMultiAgentRuntime } = require('@getenki/ai')

async function main() {
  const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'

  const runtime = new NativeMultiAgentRuntime(
    [
      {
        agentId: 'coordinator',
        name: 'Coordinator',
        systemPromptPreamble: 'Use discover_agents before delegating work.',
        model,
        maxIterations: 20,
        capabilities: ['planning', 'orchestration'],
      },
      {
        agentId: 'researcher',
        name: 'Researcher',
        systemPromptPreamble: 'Handle delegated research tasks clearly and briefly.',
        model,
        maxIterations: 20,
        capabilities: ['research', 'analysis'],
      },
    ],
    process.cwd(),
  )

  const available = await runtime.discover('research', JsAgentStatus.Online)
  console.log(available)

  const result = await runtime.process(
    'coordinator',
    'basic-js-multi-agent-session',
    'Use discover_agents first, then delegate this question to the researcher.',
  )
  console.log(result)
}

main().catch(console.error)
```

Repository examples:

- `example/basic-js/index.js`
- `example/basic-ts/index.ts`
