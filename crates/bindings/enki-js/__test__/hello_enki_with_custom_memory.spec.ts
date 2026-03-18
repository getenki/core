import test from 'ava'

import {Agent, MemoryBackend} from '../client'

class ExampleMemory extends MemoryBackend {
  name = 'python_memory'
  sessions = new Map<string, Array<{ role: string; content: string }>>()

  async record(sessionId: string, userMsg: string, assistantMsg: string) {
    const exchanges = this.sessions.get(sessionId) ?? []
    exchanges.push({ role: 'user', content: userMsg })
    exchanges.push({ role: 'assistant', content: assistantMsg })
    this.sessions.set(sessionId, exchanges)
  }

  async recall(sessionId: string, _query: string, maxEntries: number) {
    const exchanges = this.sessions.get(sessionId) ?? []
    return exchanges.slice(-maxEntries).map((entry, index) => ({
      key: `recent-${index}`,
      content: `${entry.role}: ${entry.content}`,
      kind: 'RECENT_MESSAGE',
      relevance: 0.8,
      timestampNs: index,
    }))
  }

  async flush(sessionId: string) {
    this.sessions.set(sessionId, this.sessions.get(sessionId) ?? [])
  }
}

class FakeLowLevelEnkiAgent {
  static lastArgs: any

  static withMemory(args: any) {
    FakeLowLevelEnkiAgent.lastArgs = args
    return {
      run: async () => 'memory response',
    }
  }
}

test('hello_enki_with_custom_memory: wires memory modules through the JS wrapper', async (t) => {
  const memory = new ExampleMemory()
  const agent = new Agent('ollama::llama3.2:latest', {
    instructions: 'Answer clearly and keep responses short.',
    memories: [memory.asMemoryModule()],
    lowLevelAgent: FakeLowLevelEnkiAgent,
  })

  const result = await agent.run('Explain what this project does.', {
    sessionId: 'hello-enki-memory',
  })

  t.is(result.output, 'memory response')
  t.deepEqual(FakeLowLevelEnkiAgent.lastArgs.memories, [{ name: 'python_memory' }])

  const handler = FakeLowLevelEnkiAgent.lastArgs.handler
  await handler.record('python_memory', 'hello-enki-memory', 'hello', 'world')
  t.deepEqual(await handler.recall('python_memory', 'hello-enki-memory', 'hello', 4), [
    {
      key: 'recent-0',
      content: 'user: hello',
      kind: 'RECENT_MESSAGE',
      relevance: 0.8,
      timestampNs: 0,
    },
    {
      key: 'recent-1',
      content: 'assistant: world',
      kind: 'RECENT_MESSAGE',
      relevance: 0.8,
      timestampNs: 1,
    },
  ])

  await handler.flush('python_memory', 'hello-enki-memory')
  t.true(memory.sessions.has('hello-enki-memory'))
})

test('hello_enki_with_custom_memory: real native binding is still missing custom memory support', async (t) => {
  const memory = new ExampleMemory()
  const agent = new Agent('ollama::llama3.2:latest', {
    memories: [memory.asMemoryModule()],
  })

  await t.throwsAsync(
    agent.run('Explain what this project does.', {
      sessionId: 'hello-enki-memory-native',
    }),
    {
      instanceOf: Error,
      message: 'NativeEnkiAgent does not support custom memory',
    },
  )
})
