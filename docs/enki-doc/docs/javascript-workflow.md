---
sidebar_position: 2
slug: /javascript-workflow
---

# JavaScript Workflow

`NativeWorkflowRuntime` exposes the Rust workflow engine to Node.js. It is a low-level JSON-based API: you provide workflow agents plus JSON strings for task and workflow definitions, and the runtime returns JSON strings for workflow lists, responses, and persisted run state.

```js
const { NativeEnkiAgent, NativeWorkflowRuntime } = require('@getenki/ai')

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

const tasksJson = [
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

const workflowsJson = [
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

async function main() {
  const runtime = new NativeWorkflowRuntime([researcher, writer], tasksJson, workflowsJson, './.enki')

  const response = JSON.parse(
    await runtime.startJson(
      JSON.stringify({
        workflow_id: 'research-to-summary',
        input: { topic: 'workflow bindings in enki-js' },
      }),
    ),
  )

  const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log(persisted.status)
}

main().catch(console.error)
```

See the full runnable workflow sample in `example/basic-ts/agent-workflow.ts`.

## Human Intervention

Workflow runs persist pending interventions as part of the run state, so approvals and failure escalations can pause and resume without moving state into a separate coordinator service.

Each pending intervention includes:

- `workflow_id`
- `run_id`
- `node_id`
- `prompt`
- `reason`
- `response`
- `created_at` and `resolved_at`

Two built-in patterns are supported:

- `human_gate` nodes pause immediately and wait for a human response
- task nodes with `failure_policy: "pause_for_intervention"` convert a terminal failure into an intervention asking the human to `retry`, `skip`, `continue`, or `fail`

The runnable interactive example is `example/basic-ts/human-intervention-workflow.ts`. It waits on stdin before resolving each intervention.

It demonstrates:

- a `human_gate` approval flow that pauses, resolves, and resumes to `approval.approved = true`
- a missing-agent failure that pauses for intervention and resumes after a human response such as `skip`

The runtime interaction loop is:

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

