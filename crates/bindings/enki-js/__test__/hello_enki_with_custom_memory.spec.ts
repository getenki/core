import test from 'ava'

import {mkdtempSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'

import {NativeEnkiAgent} from '../index.js'

test('hello_enki_with_custom_memory: accepts a workspaceHome path', (t) => {
  const workspaceHome = mkdtempSync(join(tmpdir(), 'enki-js-memory-'))
  const agent = new NativeEnkiAgent(
    'Agent',
    'Answer clearly and keep responses short.',
    'ollama::llama3.2:latest',
    20,
    workspaceHome,
  )

  t.is(typeof agent.run, 'function')
})

test('hello_enki_with_custom_memory: constructor still requires a model', (t) => {
  const error = t.throws(() => new NativeEnkiAgent())

  t.true((error?.message ?? '').includes('Missing model'))
})
