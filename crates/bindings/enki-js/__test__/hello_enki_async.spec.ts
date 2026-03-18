import test from 'ava'

import {Agent, EnkiAgent} from '../client'

const testWithOllama = process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip

test('hello_enki_async: supports awaited async runs', async (t) => {
  const agent = Object.create(EnkiAgent.prototype) as EnkiAgent & {
    _native: {
      run: (sessionId: string, userMessage: string) => Promise<string>
    }
  }

  agent._native = {
    run: async (sessionId, userMessage) => {
      t.is(sessionId, 'hello-enki')
      t.is(userMessage, 'Explain what this project does.')
      return 'Enki helps build and run coding agents.'
    },
  }

  const result = await agent.run('hello-enki', 'Explain what this project does.')

  t.is(result, 'Enki helps build and run coding agents.')
})

test('hello_enki_async: high-level Agent awaits async low-level runs', async (t) => {
  class FakeLowLevelAgent {
    async run(sessionId: string, userMessage: string) {
      t.is(sessionId, 'hello-enki-async-wrapper')
      t.is(userMessage, 'Explain what this project does.')
      return 'async wrapper response'
    }
  }

  const agent = new Agent('ollama::llama3.2:latest', {
    lowLevelAgent: FakeLowLevelAgent,
  })

  const result = await agent.run('Explain what this project does.', {
    sessionId: 'hello-enki-async-wrapper',
  })

  t.is(result.output, 'async wrapper response')
})

testWithOllama('hello_enki_async: runs against Ollama when enabled', async (t) => {
  t.timeout(10 * 60 * 1000)

  const agent = new EnkiAgent({
    model: 'ollama::llama3.2:latest',
    systemPromptPreamble: 'Answer clearly and keep responses short.',
  })

  const result = await agent.run('hello-enki-async', 'Explain what this project does.')

  t.is(typeof result, 'string')
  t.true(result.trim().length > 0)
})
