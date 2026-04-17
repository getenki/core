import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { stdin as input, stdout as output } from 'node:process'
import { createInterface } from 'node:readline/promises'

import { NativeEnkiAgent, NativeWorkflowRuntime } from '@getenki/ai'

declare const process: {
  cwd(): string
  env: Record<string, string | undefined>
  exitCode?: number
}

const MODEL = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
const WORKSPACE_HOME = join(process.cwd(), '.enki-human-intervention')

function buildPlaceholderAgents(): NativeEnkiAgent[] {
  const observer = new NativeEnkiAgent(
    'Workflow Observer',
    'You are a placeholder workflow agent and should never be called in this example.',
    MODEL,
    1,
    WORKSPACE_HOME,
  )
  observer.configureWorkflow('workflow-observer', ['support'])
  return [observer]
}

function buildHumanGateWorkflowsJson(): string[] {
  return [
    JSON.stringify({
      id: 'approval-flow',
      name: 'Approval Flow',
      nodes: [
        {
          id: 'approval',
          kind: 'human_gate',
          prompt: 'Approve publishing these release notes?',
          output_key: 'approval',
        },
      ],
      edges: [],
    }),
  ]
}

function buildFailureInterventionTasksJson(): string[] {
  return [
    JSON.stringify({
      id: 'missing-agent-task',
      target: { type: 'agent_id', value: 'missing-agent' },
      prompt: 'This task intentionally targets a missing agent.',
      failure_policy: 'pause_for_intervention',
    }),
  ]
}

function buildFailureInterventionWorkflowsJson(): string[] {
  return [
    JSON.stringify({
      id: 'failure-escalation-flow',
      name: 'Failure Escalation Flow',
      nodes: [
        {
          id: 'missing-agent-step',
          kind: 'task',
          task_id: 'missing-agent-task',
          output_key: 'missing_agent_step',
        },
      ],
      edges: [],
    }),
  ]
}

async function promptForIntervention(prompt: string, allowed: string): Promise<string> {
  console.log(`\nHuman input required: ${prompt}`)
  console.log(`Allowed responses: ${allowed}`)
  const rl = createInterface({ input, output })
  try {
    return (await rl.question('Your response: ')).trim()
  } finally {
    rl.close()
  }
}

async function runHumanGateExample(): Promise<void> {
  const runtime = new NativeWorkflowRuntime(
    buildPlaceholderAgents(),
    [],
    buildHumanGateWorkflowsJson(),
    WORKSPACE_HOME,
  )

  const response = JSON.parse(
    await runtime.startJson(
      JSON.stringify({
        workflow_id: 'approval-flow',
        input: { requester: 'release-bot' },
      }),
    ),
  )

  console.log('Human gate response:')
  console.log(JSON.stringify(response, null, 2))

  const paused = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log('\nPending interventions for human gate:')
  console.log(JSON.stringify(paused.pending_interventions, null, 2))

  const intervention = paused.pending_interventions[0]
  const interventionId = intervention.id
  const humanResponse = await promptForIntervention(intervention.prompt, 'yes / no')
  const resolved = JSON.parse(
    await runtime.submitInterventionJson(response.run_id, interventionId, humanResponse),
  )
  console.log('\nState after submitting human response:')
  console.log(JSON.stringify(resolved, null, 2))

  const resumed = JSON.parse(await runtime.resumeJson(response.run_id))
  console.log('\nResumed human gate workflow:')
  console.log(JSON.stringify(resumed, null, 2))

  const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log('\nPersisted human gate state:')
  console.log(JSON.stringify(persisted, null, 2))
}

async function runFailureEscalationExample(): Promise<void> {
  const runtime = new NativeWorkflowRuntime(
    buildPlaceholderAgents(),
    buildFailureInterventionTasksJson(),
    buildFailureInterventionWorkflowsJson(),
    WORKSPACE_HOME,
  )

  const response = JSON.parse(
    await runtime.startJson(
      JSON.stringify({
        workflow_id: 'failure-escalation-flow',
        input: { ticket: 'OPS-42' },
      }),
    ),
  )

  console.log('\nFailure escalation response:')
  console.log(JSON.stringify(response, null, 2))

  const paused = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log('\nPending interventions for failed node:')
  console.log(JSON.stringify(paused.pending_interventions, null, 2))

  const intervention = paused.pending_interventions[0]
  const interventionId = intervention.id
  const humanResponse = await promptForIntervention(
    intervention.prompt,
    'retry / skip / continue / fail',
  )
  const resolved = JSON.parse(
    await runtime.submitInterventionJson(response.run_id, interventionId, humanResponse),
  )
  console.log('\nState after submitting human response:')
  console.log(JSON.stringify(resolved, null, 2))

  const resumed = JSON.parse(await runtime.resumeJson(response.run_id))
  console.log('\nResumed failure escalation workflow:')
  console.log(JSON.stringify(resumed, null, 2))

  const persisted = JSON.parse(await runtime.inspectJson(response.run_id))
  console.log('\nPersisted failure escalation state:')
  console.log(JSON.stringify(persisted, null, 2))
}

async function main(): Promise<void> {
  await mkdir(WORKSPACE_HOME, { recursive: true })

  await runHumanGateExample()
  await runFailureEscalationExample()
}

main().catch((error: unknown) => {
  console.error(error)
  process.exitCode = 1
})
