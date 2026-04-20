const {NativeEnkiAgent, NativeToolRegistry} = require('@getenki/ai')

async function main() {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
    const sessionId = 'basic-js-tool-registry'

    const registry = new NativeToolRegistry()
    registry.registerTools(
        [
            {
                name: 'lookup_release_note',
                description: 'Return a short release note for a named feature.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        feature: {type: 'string'},
                    },
                    required: ['feature'],
                },
            },
            {
                name: 'summarize_priority',
                description: 'Explain whether a feature should be treated as high priority.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        feature: {type: 'string'},
                    },
                    required: ['feature'],
                },
            },
        ],
        (toolName, inputJson) => {
            const args = inputJson ? JSON.parse(inputJson) : {}
            const feature = String(args.feature ?? '').toLowerCase()

            if (toolName === 'lookup_release_note') {
                const notes = {
                    registry: 'Tool registries let teams define tools once and attach them to multiple agents.',
                    workflow: 'Workflow agents can now share reusable tool configuration instead of duplicating tool definitions.',
                    memory: 'Memory modules continue to work alongside connected tool registries.',
                }
                return notes[feature] ?? `No release note found for '${feature}'.`
            }

            if (toolName === 'summarize_priority') {
                if (feature === 'registry') {
                    return 'High priority: it removes duplicated tool wiring across agents.'
                }
                return `Normal priority: '${feature}' is useful but not flagged as urgent.`
            }

            return `Unknown tool '${toolName}'.`
        },
    )

    const agent = new NativeEnkiAgent(
        'Registry Agent',
        [
            'You explain Enki features clearly.',
            'Use the connected tools before making implementation claims.',
            'Keep the final answer concise.',
        ].join(' '),
        model,
        20,
        process.cwd(),
    )

    agent.connectToolRegistry(registry)

    console.log('Connected tools:', registry.toolNames().join(', '))

    const result = await agent.runWithTrace(
        sessionId,
        [
            'Explain what the tool registry example demonstrates.',
            'Use lookup_release_note for registry and summarize_priority for registry.',
            'Mention that the tools were attached dynamically to the agent.',
        ].join(' '),
    )

    console.log('\nExecution steps:')
    for (const step of result.steps) {
        console.log(`${step.index}. [${step.phase}] ${step.kind}: ${step.detail}`)
    }

    console.log('\nAgent output:')
    console.log(result.output)
}

main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
