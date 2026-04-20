import test from 'ava'

import {JsMemoryKind, NativeEnkiAgent, NativeToolRegistry} from '../index.js'

test('hello_enki_with_tools_and_memory: exposes custom tool and memory factories', (t) => {
    t.is(typeof NativeEnkiAgent.withTools, 'function')
    t.is(typeof NativeEnkiAgent.withToolRegistry, 'function')
    t.is(typeof NativeEnkiAgent.withMemory, 'function')
    t.is(typeof NativeEnkiAgent.withToolsAndMemory, 'function')
    t.is(typeof NativeToolRegistry, 'function')
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
        (toolName: any, argsJson: any) => JSON.stringify({toolName, argsJson}),
        '1. Inspect.\n2. Execute.',
    )

    t.is(typeof agent.run, 'function')
})

test('hello_enki_with_tools_and_memory: connects a reusable tool registry to an agent', (t) => {
    const registry = new NativeToolRegistry()
    registry.registerTools(
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
        (toolName: any, argsJson: any) => JSON.stringify({toolName, argsJson}),
    )

    const agent = new NativeEnkiAgent(
        'Agent',
        'Answer clearly and keep responses short.',
        'ollama::llama3.2:latest',
        20,
        null,
        null,
    )
    agent.connectToolRegistry(registry)

    t.deepEqual(registry.toolNames(), ['echo'])
    t.is(registry.size, 1)
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
