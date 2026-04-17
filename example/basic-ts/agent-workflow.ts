import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'

import { NativeEnkiAgent, NativeWorkflowRuntime } from '@getenki/ai'

declare const process: {
  cwd(): string
  env: Record<string, string | undefined>
  exitCode?: number
}

const MODEL = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
const WORKSPACE_HOME = join(process.cwd(), '.enki-workflow')

function buildAgents(): NativeEnkiAgent[] {
  const researcher = new NativeEnkiAgent(
    'Researcher',
    'You are a concise researcher. Return short factual notes that are easy to summarize.',
    MODEL,
    4,
    WORKSPACE_HOME,
  )
  researcher.configureWorkflow('researcher', ['research'])

  const writer = new NativeEnkiAgent(
    'Writer',
    'You turn research notes into short polished summaries.',
    MODEL,
    4,
    WORKSPACE_HOME,
  )
  writer.configureWorkflow('writer', ['writing'])

  return [researcher, writer]
}

function buildTasksJson(): string[] {
  return [
    JSON.stringify({
      id: 'research_topic',
      target: { type: 'capabilities', value: ['research'] },
      prompt: 'Research {{topic}} and return 3 concise bullet points.',
      input_bindings: { topic: 'input.topic' },
    }),
    JSON.stringify({
      id: 'write_summary',
      target: { type: 'agent_id', value: 'writer' },
      prompt: 'Write a short summary for {{topic}} using these notes:\n{{research.content}}',
      input_bindings: {
        topic: 'input.topic',
        research: 'research',
      },
    }),
  ]
}

function buildWorkflowsJson(): string[] {
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
      edges: [{ from: 'research', to: 'summary', transition: { type: 'always' } }],
    }),
  ]
}

async function main(): Promise<void> {
  await mkdir(WORKSPACE_HOME, { recursive: true })

  const runtime = new NativeWorkflowRuntime(
    buildAgents(),
    buildTasksJson(),
    buildWorkflowsJson(),
    WORKSPACE_HOME,
  )

  console.log('Registered workflows:')
  console.log(JSON.stringify(JSON.parse(await runtime.listWorkflowsJson()), null, 2))

  const response = JSON.parse(
    await runtime.startJson(
      JSON.stringify({
        workflow_id: 'research-to-summary',
        input: { topic: 'workflow bindings in enki-js' },
      }),
    ),
  )

  console.log('\nWorkflow response:')
  console.log(JSON.stringify(response, null, 2))

  const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log('\nPersisted run state:')
  console.log(JSON.stringify(persisted, null, 2))
}

main().catch((error: unknown) => {
  console.error(error)
  process.exitCode = 1
})
