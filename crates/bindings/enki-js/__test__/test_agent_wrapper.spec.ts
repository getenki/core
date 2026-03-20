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
  )

  t.is(typeof agent.run, 'function')
})

test.serial('test_agent_wrapper: run starts an async native task', (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
  )

  const result = agent.run('session-1', 'Explain what this project does.')

  t.is(typeof result.then, 'function')
})
