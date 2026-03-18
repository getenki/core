import test from 'ava'

import {Agent} from '../client'

class FakeLowLevelEnkiAgent {
  static lastArgs: any

  static withTools(args: any) {
    FakeLowLevelEnkiAgent.lastArgs = args
    return {
      run: async () => {
        const handler = args.handler
        const status = JSON.parse(await handler.execute('projectStatus', '{}', 'agent', 'workspace', 'sessions'))
        const total = await handler.execute('sumNumbers', JSON.stringify({ values: [7, 8, 9] }), '', '', '')
        const slug = await handler.execute('makeSlug', JSON.stringify({ text: 'Enki Python Tools' }), '', '', '')
        const note = await handler.execute(
          'echoNote',
          JSON.stringify({ title: 'Tool Summary', body: 'custom tools are wired' }),
          '',
          '',
          '',
        )
        return `${status.status}:${total}:${slug}:${note.includes('Tool Summary')}`
      },
    }
  }
}

test('test_all: creates an agent with multiple custom tools', async (t) => {
  const agent = new Agent('ollama::llama3.2:latest', {
    instructions: 'Prefer custom Python tools when they directly answer the request.',
    maxIterations: 4,
    workspaceHome: './test',
    lowLevelAgent: FakeLowLevelEnkiAgent,
  })

  agent.toolPlain(
    function projectStatus() {
      return {
        agentDir: 'agent',
        workspaceDir: 'workspace',
        sessionsDir: 'sessions',
        status: 'ready',
      }
    },
    {
      description: 'Return the current agent and workspace paths plus a ready status.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  agent.toolPlain(
    function sumNumbers(values) {
      return values.reduce((sum: number, value: number) => sum + value, 0)
    },
    {
      description: 'Sum a list of integers and return the total as text.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          values: {
            type: 'array',
            items: { type: 'integer' },
          },
        },
        additionalProperties: false,
        required: ['values'],
      }),
    },
  )

  agent.toolPlain(
    function makeSlug(text) {
      return String(text).trim().toLowerCase().replaceAll(' ', '-')
    },
    {
      description: 'Convert text into a lowercase dash-separated slug.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          text: { type: 'string' },
        },
        additionalProperties: false,
        required: ['text'],
      }),
    },
  )

  agent.toolPlain(
    function echoNote(title, body) {
      return `# ${title}\n\n${body}`
    },
    {
      description: 'Format a title and body as a markdown note.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          title: { type: 'string' },
          body: { type: 'string' },
        },
        additionalProperties: false,
        required: ['title', 'body'],
      }),
    },
  )

  const result = await agent.run('Use all custom tools.', { sessionId: 'session-custom-tools' })

  t.is(result.output, 'ready:24:enki-python-tools:true')
})

test('test_all: real native binding is still missing custom tool support', async (t) => {
  const agent = new Agent('ollama::llama3.2:latest')

  agent.toolPlain(
    function projectStatus() {
      return { status: 'ready' }
    },
    {
      description: 'Return a ready status.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  await t.throwsAsync(agent.run('Use all custom tools.', { sessionId: 'session-custom-tools-native' }), {
    instanceOf: Error,
    message: 'NativeEnkiAgent does not support custom tools',
  })
})
