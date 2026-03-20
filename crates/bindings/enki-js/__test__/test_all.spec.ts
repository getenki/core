import test from 'ava'

import {readFileSync} from 'node:fs'

import {NativeEnkiAgent} from '../index.js'

test('test_all: package.json points to the generated native entrypoints', (t) => {
  const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))

  t.is(packageJson.main, 'index.js')
  t.is(packageJson.types, 'index.d.ts')
})

test('test_all: the package entrypoint exposes NativeEnkiAgent', (t) => {
  const agent = new NativeEnkiAgent('Agent', 'Prompt', 'ollama::llama3.2:latest', 4, './test')

  t.is(typeof agent.run, 'function')
})
