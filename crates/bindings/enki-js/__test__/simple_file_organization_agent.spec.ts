import test from 'ava'

import {mkdirSync, writeFileSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'

import {NativeEnkiAgent} from '../index.js'

test('simple_file_organization_agent: constructs with a dedicated workspace', (t) => {
  const root = join(tmpdir(), `enki-js-folder-review-${Date.now()}`)
  mkdirSync(join(root, 'src'), { recursive: true })
  writeFileSync(join(root, 'README.md'), '# Demo\n')
  writeFileSync(join(root, 'src', 'index.ts'), 'export const demo = true\n')

  const agent = new NativeEnkiAgent(
    'Folder Reviewer',
    'Review folders using the native runtime.',
    'ollama::llama3.2:latest',
    20,
    root,
  )

  t.is(typeof agent.run, 'function')
})

test('simple_file_organization_agent: run can be started for a folder review prompt', (t) => {
  const root = join(tmpdir(), `enki-js-folder-review-run-${Date.now()}`)
  mkdirSync(root, { recursive: true })

  const agent = new NativeEnkiAgent(
    'Folder Reviewer',
    'Review folders using the native runtime.',
    'ollama::llama3.2:latest',
    20,
    root,
  )

  const result = agent.run('folder-review', 'Review this folder.')

  t.is(typeof result.then, 'function')
})
