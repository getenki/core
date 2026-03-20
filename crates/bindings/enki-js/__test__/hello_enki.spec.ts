import test from 'ava'

import {NativeEnkiAgent} from '../index.js'

const testWithOllama = process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip

test('hello_enki: exports NativeEnkiAgent from the package entrypoint', (t) => {
  t.is(typeof NativeEnkiAgent, 'function')
})

test('hello_enki: constructs a native agent with a valid model string', (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
  )

  t.is(typeof agent.run, 'function')
})

testWithOllama('hello_enki: runs a hello prompt through NativeEnkiAgent', async (t) => {
  t.timeout(10 * 60 * 1000)

  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
  )

  const result = await agent.run('hello-enki', 'Explain what this project does.')

  t.is(typeof result, 'string')
  t.true(result.trim().length > 0)
})
