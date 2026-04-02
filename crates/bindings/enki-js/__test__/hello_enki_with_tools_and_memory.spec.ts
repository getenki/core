import test from 'ava'

import {JsMemoryKind, NativeEnkiAgent} from '../index.js'

test('hello_enki_with_tools_and_memory: exposes custom tool and memory factories', (t) => {
    t.is(typeof NativeEnkiAgent.withTools, 'function')
    t.is(typeof NativeEnkiAgent.withMemory, 'function')
    t.is(typeof NativeEnkiAgent.withToolsAndMemory, 'function')
})

test('hello_enki_with_tools_and_memory: constructs an agent with a custom tool handler', (t) => {
    const agent = NativeEnkiAgent.withTools(
        'Agent',
        'Answer clearly and keep responses short.',
        'ollama::llama3.2:latest',
        20,
        null,
        [
            {
                name: 'echo',
                description: 'Echo a value',
                parametersJson: JSON.stringify({
                    type: 'object',
                    properties: {
                        value: {type: 'string'},
                    },
                    required: ['value'],
                }),
            },
        ],
        (toolName, argsJson) => JSON.stringify({toolName, argsJson}),
        '1. Inspect.\n2. Execute.',
    )

    t.is(typeof agent.run, 'function')
})

test('hello_enki_with_tools_and_memory: constructs an agent with custom memory handlers', (t) => {
    const agent = NativeEnkiAgent.withMemory(
        'Agent',
        'Answer clearly and keep responses short.',
        'ollama::llama3.2:latest',
        20,
        null,
        [{name: 'notes'}],
        () => {
        },
        () => [
            {
                key: 'greeting',
                content: 'hello',
                kind: JsMemoryKind.Preference,
                relevance: 1,
                timestampNs: '1',
            },
        ],
        () => {
        },
        () => {
        },
        null,
    )

    t.is(typeof agent.run, 'function')
})
