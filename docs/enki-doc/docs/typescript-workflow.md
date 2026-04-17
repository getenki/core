---
sidebar_position: 2
slug: /typescript-workflow
---

# TypeScript Workflow

Use `NativeWorkflowRuntime` when you want typed access to the Rust workflow engine from TypeScript. Like the JavaScript API, it is a low-level JSON-based surface: you provide typed workflow members plus JSON strings for task and workflow definitions, and the runtime returns JSON strings for workflow lists, responses, and persisted run state.

```ts
import { NativeWorkflowRuntime, type JsMultiAgentMember } from '@getenki/ai'

const members: JsMultiAgentMember[] = [
  {
    agentId: 'researcher',
    name: 'Researcher',
    systemPromptPreamble: 'Return short factual notes.',
    model: 'ollama::qwen3.5:latest',
    maxIterations: 4,
    capabilities: ['research'],
  },
  {
    agentId: 'writer',
    name: 'Writer',
    systemPromptPreamble: 'Turn notes into a concise summary.',
    model: 'ollama::qwen3.5:latest',
    maxIterations: 4,
    capabilities: ['writing'],
  },
]

const tasksJson: string[] = [
  JSON.stringify({
    id: 'research_topic',
    target: { type: 'capabilities', value: ['research'] },
    prompt: 'Research {{topic}} and return 3 concise bullet points.',
    input_bindings: { topic: 'input.topic' },
  }),
  JSON.stringify({
    id: 'write_summary',
    target: { type: 'agent_id', value: 'writer' },
    prompt: 'Write a short summary for {{topic}} using {{research.content}}',
    input_bindings: {
      topic: 'input.topic',
      research: 'research',
    },
  }),
]

const workflowsJson: string[] = [
  JSON.stringify({
    id: 'research-to-summary',
    name: 'Research To Summary',
    nodes: [
      { id: 'research', kind: 'task', task_id: 'research_topic', output_key: 'research' },
      { id: 'summary', kind: 'task', task_id: 'write_summary', output_key: 'summary' },
    ],
    edges: [
      { from: 'research', to: 'summary', transition: { type: 'always' } },
    ],
  }),
]

const runtime = new NativeWorkflowRuntime(members, tasksJson, workflowsJson, './.enki')

const response = JSON.parse(
  await runtime.startJson(
    JSON.stringify({
      workflow_id: 'research-to-summary',
      input: { topic: 'workflow bindings in enki-ts' },
    }),
  ),
)

const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
console.log(persisted.status)
```

Current workflow methods:

- `listWorkflowsJson()`
- `listRunsJson()`
- `inspectJson(runId)`
- `startJson(requestJson)`
- `resumeJson(runId)`
- `submitInterventionJson(runId, interventionId, response?)`