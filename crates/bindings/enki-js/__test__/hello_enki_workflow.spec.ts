import test from 'ava'

import {NativeEnkiAgent, NativeWorkflowRuntime} from '../index.js'

const testWithOllama = process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip
const model = process.env.ENKI_MODEL ?? 'ollama::llama3.2:latest'
const workspaceHome = './test/workflow-example-js'

function buildAgents() {
  const researcher = new NativeEnkiAgent(
    'Researcher',
    'You are a concise researcher. Return short factual notes that are easy to summarize.',
    model,
    4,
    workspaceHome,
  )
  researcher.configureWorkflow('researcher', ['research'])

  const writer = new NativeEnkiAgent(
    'Writer',
    'You turn research notes into short polished summaries.',
    model,
    4,
    workspaceHome,
  )
  writer.configureWorkflow('writer', ['writing'])

  return [researcher, writer]
}

function buildTasksJson() {
  return [
    JSON.stringify({
      id: 'research_topic',
      target: {type: 'capabilities', value: ['research']},
      prompt: 'Research {{topic}} and return 3 concise bullet points.',
      input_bindings: {topic: 'input.topic'},
    }),
    JSON.stringify({
      id: 'write_summary',
      target: {type: 'agent_id', value: 'writer'},
      prompt: 'Write a short summary for {{topic}} using these notes:\n{{research.content}}',
      input_bindings: {
        topic: 'input.topic',
        research: 'research',
      },
    }),
  ]
}

function buildWorkflowsJson() {
  return [
    JSON.stringify({
      id: 'research-to-summary',
      name: 'Research To Summary',
      nodes: [
        {
          id: 'research',
          kind: 'task',
          task_id: 'research_topic',
          output_key: 'research',
        },
        {
          id: 'summary',
          kind: 'task',
          task_id: 'write_summary',
          output_key: 'summary',
        },
      ],
      edges: [
        {
          from: 'research',
          to: 'summary',
          transition: {type: 'always'},
        },
      ],
    }),
  ]
}

test('hello_enki_workflow: constructs a workflow runtime', (t) => {
  const runtime = new NativeWorkflowRuntime(
    buildAgents(),
    buildTasksJson(),
    buildWorkflowsJson(),
    workspaceHome,
  )

  t.is(typeof runtime.listWorkflowsJson, 'function')
  t.is(typeof runtime.startJson, 'function')
  t.is(typeof runtime.inspectJson, 'function')
  t.is(typeof runtime.listRunsJson, 'function')
})

test('hello_enki_workflow: lists configured workflows', async (t) => {
  const runtime = new NativeWorkflowRuntime(
    buildAgents(),
    buildTasksJson(),
    buildWorkflowsJson(),
    workspaceHome,
  )

  const workflows = JSON.parse(await runtime.listWorkflowsJson())
  const firstWorkflow = JSON.parse(workflows[0])

  t.true(Array.isArray(workflows))
  t.is(workflows.length, 1)
  t.is(firstWorkflow.id, 'research-to-summary')
})

testWithOllama('hello_enki_workflow: starts a workflow and inspects the persisted run', async (t) => {
  t.timeout(10 * 60 * 1000)

  const runtime = new NativeWorkflowRuntime(
    buildAgents(),
    buildTasksJson(),
    buildWorkflowsJson(),
    workspaceHome,
  )

  const response = JSON.parse(
    await runtime.startJson(
      JSON.stringify({
        workflow_id: 'research-to-summary',
        input: {topic: 'workflow bindings in enki-js'},
      }),
    ),
  )

  t.is(response.workflow_id, 'research-to-summary')
  t.truthy(response.run_id)
  t.truthy(response.context.values.summary)

  const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
  t.is(persisted.run_id, response.run_id)

  const runs = JSON.parse(await runtime.listRunsJson())
  t.true(Array.isArray(runs))
  t.true(runs.some((run: {run_id: string}) => run.run_id === response.run_id))
})
