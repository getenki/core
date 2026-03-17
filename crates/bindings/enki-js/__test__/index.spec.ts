import test from 'ava'

import { NativeEnkiAgent } from '../index'

test('exports NativeEnkiAgent from native binding', (t) => {
  t.is(typeof NativeEnkiAgent, 'function')
})
