---
sidebar_position: 2
slug: /typescript-workflow
---

# TypeScript Workflow

Use `NativeWorkflowRuntime` when you want typed access to the Rust workflow engine from TypeScript. Like the JavaScript API, it is a low-level JSON-based surface: you provide workflow agents plus JSON strings for task and workflow definitions, and the runtime returns JSON strings for workflow lists, responses, and persisted run state.

```ts
import { NativeEnkiAgent, NativeWorkflowRuntime } from '@getenki/ai'

const researcher = new NativeEnkiAgent(
  'Researcher',
  'Return short factual notes.',
  'ollama::qwen3.5:latest',
  4,
  './.enki',
)
researcher.configureWorkflow('researcher', ['research'])

const writer = new NativeEnkiAgent(
  'Writer',
  'Turn notes into a concise summary.',
  'ollama::qwen3.5:latest',
  4,
  './.enki',
)
writer.configureWorkflow('writer', ['writing'])

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
    edges: [{ from: 'research', to: 'summary', transition: { type: 'always' } }],
  }),
]

const runtime = new NativeWorkflowRuntime([researcher, writer], tasksJson, workflowsJson, './.enki')

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

See the full runnable TypeScript workflow sample in `example/basic-ts/agent-workflow.ts`.

## Human Intervention

Workflow runs persist pending interventions inside the run state, which keeps human approvals and failure escalations resumable and inspectable.

Each pending intervention includes:

- `workflow_id`
- `run_id`
- `node_id`
- `prompt`
- `reason`
- `response`
- `created_at` and `resolved_at`

Two common patterns are supported:

- `human_gate` nodes pause immediately and wait for a human response
- task nodes with `failure_policy: "pause_for_intervention"` convert a terminal failure into an intervention asking the human to `retry`, `skip`, `continue`, or `fail`

The runnable interactive example is `example/basic-ts/human-intervention-workflow.ts`. It waits on stdin before resolving each intervention.

It demonstrates:

- a `human_gate` approval flow
- a failure escalation flow where a human response such as `skip` controls how the run continues

The interaction loop is:

1. `startJson(...)` returns a paused workflow response
2. `inspectJson(runId)` exposes `pending_interventions`
3. read the intervention prompt and collect human input
4. `submitInterventionJson(runId, interventionId, response)` resolves the intervention
5. `resumeJson(runId)` continues the persisted run

Current workflow methods:

- `listWorkflowsJson()`
- `listRunsJson()`
- `inspectJson(runId)`
- `startJson(requestJson)`
- `resumeJson(runId)`
- `submitInterventionJson(runId, interventionId, response?)`

