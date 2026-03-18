import test from 'ava'

import {Agent, EnkiAgent, LlmProviderBackend, Tool} from '../client'

class FakeEnkiAgent {
  static lastArgs: any

  static withTools(args: any) {
    FakeEnkiAgent.lastArgs = args
    return new FakeEnkiAgent(args.handler)
  }

  static withLlm(args: any) {
    FakeEnkiAgent.lastArgs = args
    return new FakeEnkiAgent(undefined, args.llmHandler)
  }

  static withToolsAndLlm(args: any) {
    FakeEnkiAgent.lastArgs = args
    return new FakeEnkiAgent(args.handler, args.llmHandler)
  }

  handler: any
  llmHandler: any

  constructor(handler?: any, llmHandler?: any) {
    this.handler = handler
    this.llmHandler = llmHandler
  }

  async run(_sessionId: string, userMessage: string) {
    if (this.llmHandler) {
      const raw = await this.llmHandler.complete(
        FakeEnkiAgent.lastArgs.model,
        JSON.stringify([{ role: 'user', content: userMessage }]),
        JSON.stringify([]),
      )
      const payload = raw.startsWith('{') ? JSON.parse(raw) : raw
      return typeof payload === 'string' ? payload : payload.content
    }

    const toolNames = new Set(FakeEnkiAgent.lastArgs.tools.map((tool: { name: string }) => tool.name))
    if (toolNames.has('getPlayerName') && toolNames.has('rollDice')) {
      const guess = [...userMessage].filter((char) => /\d/.test(char)).join('')
      const playerName = await this.handler.execute('getPlayerName', '{}', '', '', '')
      const diceRoll = await this.handler.execute('rollDice', '{}', '', '', '')
      if (diceRoll === guess) {
        return `Congratulations ${playerName}, you guessed correctly! You're a winner!`
      }
      return `Sorry ${playerName}, you guessed ${guess} but rolled ${diceRoll}.`
    }

    if (toolNames.has('getPlayerName') && toolNames.has('formatScore')) {
      const playerName = await this.handler.execute('getPlayerName', '{}', '', '', '')
      return `Sorry ${playerName}, schema test.`
    }

    return 'No-op'
  }
}

test.serial('test_agent_wrapper: validates constructor options', (t) => {
  t.throws(() => new EnkiAgent(null as never), {
    instanceOf: TypeError,
    message: 'EnkiAgent options must be an object',
  })
})

test.serial('test_agent_wrapper: validates run arguments before calling native binding', (t) => {
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

test.serial('test_agent_wrapper: supports pydantic-ai-style tool usage', async (t) => {
  const agent = new Agent('gateway/gemini:gemini-3-flash-preview', {
    instructions:
      "You're a dice game, you should roll the die and see if the number you get back matches the user's guess.",
    lowLevelAgent: FakeEnkiAgent,
  })

  agent.toolPlain(
    function rollDice() {
      return '4'
    },
    {
      description: 'Roll a six-sided die and return the result.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  agent.tool(
    function getPlayerName(ctx) {
      return ctx.deps
    },
    {
      description: "Get the player's name.",
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  const result = await agent.run('My guess is 4', { deps: 'Anne', sessionId: 'session-tools-1' })

  t.is(result.output, "Congratulations Anne, you guessed correctly! You're a winner!")
})

test.serial('test_agent_wrapper: builds tool schemas and passes runtime deps', async (t) => {
  const agent = new Agent('test-model', {
    lowLevelAgent: FakeEnkiAgent,
  })

  agent.toolPlain(
    function formatScore(total, lucky = false) {
      return `${total}:${lucky}`
    },
    {
      description: 'Format a score summary.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          total: { type: 'integer' },
          lucky: { type: 'boolean' },
        },
        additionalProperties: false,
        required: ['total'],
      }),
    },
  )

  agent.tool(
    function getPlayerName(ctx) {
      return ctx.deps
    },
    {
      description: "Get the player's name.",
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  const result = await agent.run('My guess is 1', { deps: 'Anne', sessionId: 'session-tools-2' })
  t.true(result.output.startsWith('Sorry Anne'))

  const tools = FakeEnkiAgent.lastArgs.tools
  const scoreSpec = tools.find((tool: { name: string }) => tool.name === 'formatScore')
  t.deepEqual(JSON.parse(scoreSpec.parametersJson), {
    type: 'object',
    properties: {
      total: { type: 'integer' },
      lucky: { type: 'boolean' },
    },
    additionalProperties: false,
    required: ['total'],
  })

  const handler = FakeEnkiAgent.lastArgs.handler
  handler.setDeps('Anne')
  try {
    t.is(await handler.execute('formatScore', JSON.stringify({ total: 7, lucky: true }), '', '', ''), '7:true')
    t.is(await handler.execute('getPlayerName', '{}', '', '', ''), 'Anne')
  } finally {
    handler.clearDeps()
  }
})

test.serial('test_agent_wrapper: registers concrete tool objects', async (t) => {
  const agent = new Agent('test-model', {
    lowLevelAgent: FakeEnkiAgent,
  })

  const tool = Tool.fromFunction(
    function formatScore(total) {
      return `score:${total}`
    },
    {
      usesContext: false,
      description: 'Format a score summary.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          total: { type: 'integer' },
        },
        additionalProperties: false,
        required: ['total'],
      }),
    },
  )

  agent.registerTool(tool)
  const result = await agent.run('My guess is 1', { sessionId: 'session-tools-3' })

  t.is(result.output, 'No-op')

  const tools = FakeEnkiAgent.lastArgs.tools
  const scoreSpec = tools.find((entry: { name: string }) => entry.name === 'formatScore')
  t.deepEqual(JSON.parse(scoreSpec.parametersJson), {
    type: 'object',
    properties: {
      total: { type: 'integer' },
    },
    additionalProperties: false,
    required: ['total'],
  })

  const handler = FakeEnkiAgent.lastArgs.handler
  t.is(await handler.execute('formatScore', JSON.stringify({ total: 7 }), '', '', ''), 'score:7')
})

test.serial('test_agent_wrapper: supports custom llm providers', async (t) => {
  class DemoProvider extends LlmProviderBackend {
    complete(model: string, messages: Array<Record<string, unknown>>, tools: Array<Record<string, unknown>>) {
      t.is(model, 'demo-model')
      t.is(messages.at(-1)?.content, 'hello')
      t.deepEqual(tools, [])
      return { content: 'provider response' }
    }
  }

  const agent = new Agent('demo-model', {
    llm: new DemoProvider(),
    lowLevelAgent: FakeEnkiAgent,
  })

  const result = await agent.run('hello', { sessionId: 'session-llm-1' })

  t.is(result.output, 'provider response')
})

test.serial('test_agent_wrapper: real native binding still rejects custom tools', async (t) => {
  const agent = new Agent('test-model')

  agent.toolPlain(
    function formatScore(total) {
      return `score:${total}`
    },
    {
      description: 'Format a score summary.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          total: { type: 'integer' },
        },
        additionalProperties: false,
        required: ['total'],
      }),
    },
  )

  await t.throwsAsync(agent.run('hello', { sessionId: 'session-native-tools' }), {
    instanceOf: Error,
    message: 'NativeEnkiAgent does not support custom tools',
  })
})
