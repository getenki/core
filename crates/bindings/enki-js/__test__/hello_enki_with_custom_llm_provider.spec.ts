import test from 'ava'

import {Agent, LlmProviderBackend} from '../client'

class DemoProvider extends LlmProviderBackend {
  complete(model: string, messages: Array<Record<string, unknown>>, tools: Array<Record<string, unknown>>) {
    return {
      content: `${model}:${String(messages.at(-1)?.content)}:${tools.length}`,
    }
  }
}

class FakeLowLevelEnkiAgent {
  static lastArgs: any

  static withLlm(args: any) {
    FakeLowLevelEnkiAgent.lastArgs = args
    return {
      run: async (_sessionId: string, userMessage: string) => {
        const raw = await args.llmHandler.complete(
          args.model,
          JSON.stringify([{ role: 'user', content: userMessage }]),
          JSON.stringify([]),
        )
        return JSON.parse(raw).content
      },
    }
  }
}

test('hello_enki_with_custom_llm_provider: wires a custom provider through the JS wrapper', async (t) => {
  const agent = new Agent('qwen3.5:latest', {
    instructions: 'Answer clearly and keep responses short.',
    llm: new DemoProvider(),
    lowLevelAgent: FakeLowLevelEnkiAgent,
  })

  const result = await agent.run('Explain what this project does.', {
    sessionId: 'hello-enki-provider',
  })

  t.is(result.output, 'qwen3.5:latest:Explain what this project does.:0')
  t.is(FakeLowLevelEnkiAgent.lastArgs.model, 'qwen3.5:latest')
})

test('hello_enki_with_custom_llm_provider: real native binding is still missing custom llm support', async (t) => {
  const agent = new Agent('qwen3.5:latest', {
    llm: new DemoProvider(),
  })

  await t.throwsAsync(
    agent.run('Explain what this project does.', {
      sessionId: 'hello-enki-provider-native',
    }),
    {
      instanceOf: Error,
      message: 'NativeEnkiAgent does not support custom llm',
    },
  )
})
