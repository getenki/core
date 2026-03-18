import test from 'ava'

import {Agent, EnkiAgent, NativeEnkiAgent} from '../client'

const testWithOllama = process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip

test('exports NativeEnkiAgent from native binding', (t) => {
  t.is(typeof NativeEnkiAgent, 'function')
})

test('exports EnkiAgent from client wrapper', (t) => {
  t.is(typeof EnkiAgent, 'function')
})

test('hello_enki: high-level Agent uses the low-level constructor for plain runs', async (t) => {
  class FakeLowLevelAgent {
    static lastConstructorArgs: unknown[]

    constructor(...args: unknown[]) {
      FakeLowLevelAgent.lastConstructorArgs = args
    }

    async run(sessionId: string, userMessage: string) {
      t.is(sessionId, 'hello-enki-wrapper')
      t.is(userMessage, 'Explain what this project does.')
      return 'Enki helps build and run coding agents.'
    }
  }

  const agent = new Agent('ollama::llama3.2:latest', {
    instructions: 'Answer clearly and keep responses short.',
    lowLevelAgent: FakeLowLevelAgent,
  })

  const result = await agent.run('Explain what this project does.', {
    sessionId: 'hello-enki-wrapper',
  })

  t.deepEqual(FakeLowLevelAgent.lastConstructorArgs, [
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
    undefined,
  ])
  t.is(result.output, 'Enki helps build and run coding agents.')
})

testWithOllama('hello_enki: runs a synchronous-style hello prompt through the async JS client', async (t) => {
  t.timeout(10 * 60 * 1000)

  const agent = new EnkiAgent({
    model: 'ollama::llama3.2:latest',
    systemPromptPreamble: 'Answer clearly and keep responses short.',
  })

  const result = await agent.run('hello-enki', 'Explain what this project does.')

  t.is(typeof result, 'string')
  t.true(result.trim().length > 0)
})
