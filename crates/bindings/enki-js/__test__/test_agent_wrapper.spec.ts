import test from 'ava'

import {NativeEnkiAgent} from '../index.js'

test.serial('test_agent_wrapper: constructor fails without a model', (t) => {
  const error = t.throws(() => new NativeEnkiAgent())

  t.true((error?.message ?? '').includes('Missing model'))
})

test.serial('test_agent_wrapper: constructor rejects malformed model identifiers', (t) => {
  const error = t.throws(() => new NativeEnkiAgent('Agent', 'Prompt', 'demo-model'))

  t.true((error?.message ?? '').includes("Invalid model format. Use 'provider::model-name'"))
})

test.serial('test_agent_wrapper: constructor accepts valid native settings', (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    0,
    null,
    '1. Plan.\n2. Act.\n3. Verify.',
  )

  t.is(typeof agent.run, 'function')
})

test.serial('test_agent_wrapper: run starts an async native task', (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
    null,
  )

  const result = agent.run('session-1', 'Explain what this project does.')

  t.is(typeof result.then, 'function')
})

test.serial('test_agent_wrapper: custom loop handler overrides default loop', async (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
    null,
  )

  agent.setAgentLoopHandler((requestJson: string) => {
  agent.setAgentLoopHandler((requestJson:any) => {
    const request = JSON.parse(requestJson)
    return JSON.stringify({
      content: `custom:${request.user_message}`,
      steps: [
        {
          index: 1,
          phase: 'Custom',
          kind: 'final',
          detail: 'Handled in JavaScript loop',
        },
      ],
    })
  })

  const result = await agent.runWithTrace('session-2', 'Explain the override')

  t.is(result.output, 'custom:Explain the override')
  t.deepEqual(result.steps, [
    {
      index: 1,
      phase: 'Custom',
      kind: 'final',
      detail: 'Handled in JavaScript loop',
    },
  ])
})
