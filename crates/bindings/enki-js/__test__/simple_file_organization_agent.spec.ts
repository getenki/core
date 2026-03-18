import test from 'ava'

import {mkdirSync, readFileSync, writeFileSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join, resolve} from 'node:path'

import {Agent} from '../client'

class FakeLowLevelEnkiAgent {
  static lastArgs: any

  static withTools(args: any) {
    FakeLowLevelEnkiAgent.lastArgs = args
    return {
      run: async () => 'folder review ready',
    }
  }
}

function resolvePath(root: string, relativePath: string) {
  const candidate = resolve(root, relativePath)
  const normalizedRoot = resolve(root)
  if (candidate !== normalizedRoot && !candidate.startsWith(`${normalizedRoot}\\`) && !candidate.startsWith(`${normalizedRoot}/`)) {
    throw new Error(`Path escapes the review root: ${relativePath}`)
  }
  return candidate
}

test('simple_file_organization_agent: folder review tools work through the JS wrapper', async (t) => {
  const root = join(tmpdir(), `enki-js-folder-review-${Date.now()}`)
  mkdirSync(join(root, 'src'), { recursive: true })
  writeFileSync(join(root, 'README.md'), '# Demo\n')
  writeFileSync(join(root, 'src', 'index.ts'), 'export const demo = true\n')

  const agent = new Agent('ollama::llama3.2:latest', {
    instructions: 'Review folders using only provided tools.',
    lowLevelAgent: FakeLowLevelEnkiAgent,
  })

  agent.tool(
    function listDirectory(ctx, relativePath = '.', maxEntries = 200) {
      const directory = resolvePath(ctx.deps.root, relativePath)
      return JSON.stringify(
        ['README.md', 'src', 'src/index.ts']
          .slice(0, maxEntries)
          .filter((entry) => entry === 'README.md' || entry === 'src'),
      )
    },
    {
      description: 'List directory entries relative to the review root.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          relativePath: { type: 'string' },
          maxEntries: { type: 'integer' },
        },
        additionalProperties: false,
      }),
    },
  )

  agent.tool(
    function readTextFile(ctx, relativePath, maxChars = 6000) {
      const filePath = resolvePath(ctx.deps.root, relativePath)
      const text = readFileSync(filePath, 'utf8')
      return text.length <= maxChars ? text : `${text.slice(0, maxChars)}\n\n[truncated]`
    },
    {
      description: 'Read a UTF-8 text file from the review root.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          relativePath: { type: 'string' },
          maxChars: { type: 'integer' },
        },
        additionalProperties: false,
        required: ['relativePath'],
      }),
    },
  )

  agent.tool(
    function folderSummary(ctx, relativePath = '.') {
      void relativePath
      return JSON.stringify({
        path: '.',
        directories: 1,
        files: 2,
        extensions: {
          '.md': 1,
          '.ts': 1,
        },
      })
    },
    {
      description: 'Summarize file counts by extension within a folder.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {
          relativePath: { type: 'string' },
        },
        additionalProperties: false,
      }),
    },
  )

  const result = await agent.run('Review this folder.', {
    sessionId: 'folder-review',
    deps: { root },
  })

  t.is(result.output, 'folder review ready')

  const handler = FakeLowLevelEnkiAgent.lastArgs.handler
  handler.setDeps({ root })
  try {
    t.deepEqual(JSON.parse(await handler.execute('listDirectory', '{}', '', '', '')), ['README.md', 'src'])
    t.is(await handler.execute('readTextFile', JSON.stringify({ relativePath: 'README.md' }), '', '', ''), '# Demo\n')
    t.deepEqual(JSON.parse(await handler.execute('folderSummary', '{}', '', '', '')), {
      path: '.',
      directories: 1,
      files: 2,
      extensions: {
        '.md': 1,
        '.ts': 1,
      },
    })
  } finally {
    handler.clearDeps()
  }
})

test('simple_file_organization_agent: real native binding is still missing custom tool support', async (t) => {
  const agent = new Agent('ollama::llama3.2:latest', {
    instructions: 'Review folders using only provided tools.',
  })

  agent.tool(
    function listDirectory(ctx) {
      return JSON.stringify([ctx.deps.root])
    },
    {
      description: 'List directory entries relative to the review root.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  await t.throwsAsync(
    agent.run('Review this folder.', {
      sessionId: 'folder-review-native',
      deps: { root: 'C:/tmp' },
    }),
    {
      instanceOf: Error,
      message: 'NativeEnkiAgent does not support custom tools',
    },
  )
})
