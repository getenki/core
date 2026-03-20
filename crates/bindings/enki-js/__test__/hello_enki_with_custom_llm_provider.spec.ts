import test from 'ava'

import {NativeEnkiAgent} from '../index.js'

test('hello_enki_with_custom_llm_provider: invalid model strings fail fast', (t) => {
  const error = t.throws(() => new NativeEnkiAgent('Agent', 'Prompt', 'demo-model'))

  t.true((error?.message ?? '').includes("Invalid model format. Use 'provider::model-name'"))
})

test('hello_enki_with_custom_llm_provider: provider-qualified model strings are accepted', (t) => {
  const agent = new NativeEnkiAgent('Agent', 'Prompt', 'ollama::llama3.2:latest')

  t.is(typeof agent.run, 'function')
})
