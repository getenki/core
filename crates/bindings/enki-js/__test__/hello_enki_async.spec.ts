import test from 'ava'

import {NativeEnkiAgent} from '../index.js'

const testWithOllama = process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip

test('hello_enki_async: run returns a promise', (t) => {
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
  )

  const result = agent.run('hello-enki-async', 'Explain what this project does.')

  t.is(typeof result.then, 'function')
})

testWithOllama('hello_enki_async: awaited runs resolve to text', async (t) => {
  t.timeout(10 * 60 * 1000)

  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
  )

  const result = await agent.run('hello-enki-async', 'Explain what this project does.')

  t.is(typeof result, 'string')
  t.true(result.trim().length > 0)
})
