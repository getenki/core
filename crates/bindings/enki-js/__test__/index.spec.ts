import test from 'ava'

import {EnkiAgent} from '../client'
import {NativeEnkiAgent} from '../index'

const testWithOllama =
    process.env.ENKI_RUN_OLLAMA_TESTS === '1' ? test : test.skip


test('exports NativeEnkiAgent from native binding', (t) => {
    t.is(typeof NativeEnkiAgent, 'function')
})

test('exports EnkiAgent from client wrapper', (t) => {
    t.is(typeof EnkiAgent, 'function')
})

test('EnkiAgent validates constructor options', (t) => {
    t.throws(() => new EnkiAgent(null as never), {
        instanceOf: TypeError,
        message: 'EnkiAgent options must be an object',
    })
})

test('EnkiAgent validates run arguments before calling native binding', (t) => {
    const agent = Object.create(EnkiAgent.prototype) as EnkiAgent & {
        _native: {
            run: (sessionId: string, userMessage: string) => Promise<string>
        }
    }

    agent._native = {
        run: async () => 'unused',
    }

    t.throws(() => agent.run('', 'Explain what this project does.'), {
        instanceOf: TypeError,
        message: 'sessionId must be a non-empty string',
    })

    t.throws(() => agent.run('session-1', ''), {
        instanceOf: TypeError,
        message: 'userMessage must be a non-empty string',
    })
})

test('EnkiAgent supports awaited async runs', async (t) => {
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

test('EnkiAgent runs asynchronously with Ollama', async (t) => {
    t.timeout(10 * 60 * 1000)

    const agent = new EnkiAgent({
        model: 'ollama::qwen3.5:latest',
        systemPromptPreamble: 'Answer clearly and keep responses short.',
    })

    const result = await agent.run('hello-enki', 'Explain what this project does.')

    t.is(typeof result, 'string')
    t.true(result.trim().length > 0)
    console.log('Agent response:', result)
})
