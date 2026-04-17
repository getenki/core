import test from 'ava'

import {readFileSync} from 'node:fs'

import {NativeEnkiAgent, NativeMultiAgentRuntime, NativeWorkflowRuntime} from '../index.js'

test('test_all: package.json points to the generated native entrypoints', (t) => {
  const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))

  t.is(packageJson.main, 'index.js')
  t.is(packageJson.types, 'index.d.ts')
  // t.is(packageJson.optionalDependencies['@getenki/ai-win32-x64-msvc'], packageJson.version)
  // t.is(packageJson.optionalDependencies['@getenki/ai-win32-arm64-msvc'], packageJson.version)
  // t.is(packageJson.optionalDependencies['@getenki/ai-darwin-x64'], packageJson.version)
  // t.is(packageJson.optionalDependencies['@getenki/ai-darwin-arm64'], packageJson.version)
  // t.is(packageJson.optionalDependencies['@getenki/ai-linux-x64-gnu'], packageJson.version)
  // t.is(packageJson.optionalDependencies['@getenki/ai-linux-arm64-gnu'], packageJson.version)
})

test('test_all: the package entrypoint exposes NativeEnkiAgent', (t) => {
  const agent = new NativeEnkiAgent('Agent', 'Prompt', 'ollama::llama3.2:latest', 4, './test')

  t.is(typeof agent.run, 'function')
})

test('test_all: the package entrypoint exposes NativeMultiAgentRuntime', (t) => {
  const runtime = new NativeMultiAgentRuntime(
    [{agentId: 'agent-1', name: 'Agent', model: 'ollama::llama3.2:latest', capabilities: []}],
    './test',
  )

  t.is(typeof runtime.process, 'function')
  t.is(typeof runtime.processWithTrace, 'function')
})

test('test_all: the package entrypoint exposes NativeWorkflowRuntime', (t) => {
  const agent = new NativeEnkiAgent('Agent', 'Prompt', 'ollama::llama3.2:latest', 4, './test')
  agent.configureWorkflow('agent-1', [])

  const runtime = new NativeWorkflowRuntime(
    [agent],
    [],
    [],
    './test',
  )

  t.is(typeof runtime.listWorkflowsJson, 'function')
  t.is(typeof runtime.startJson, 'function')
})

