const {NativeEnkiAgent} = require('@getenki/ai')

async function main() {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
    const agent = new NativeEnkiAgent(
        'Custom JS Loop Agent',
        'Answer clearly and keep responses short.',
        model,
        20,
        process.cwd(),
    )

    agent.setAgentLoopHandler((requestJson) => {
        const request = JSON.parse(requestJson)

        return JSON.stringify({
            content: [
                'This response was produced by a JavaScript-defined agent loop override.',
                `The original user request was: ${request.user_message}`,
            ].join(' '),
            steps: [
                {
                    index: 1,
                    phase: 'Custom',
                    kind: 'inspect_request',
                    detail: `Read ${request.messages.length} message(s) and ${Object.keys(request.tools).length} available tool definition(s)`,
                },
                {
                    index: 2,
                    phase: 'Custom',
                    kind: 'final',
                    detail: 'Returned a final response directly from JavaScript',
                },
            ],
        })
    })

    const result = await agent.runWithTrace(
        'basic-js-custom-agent-loop-session',
        'Explain how this example overrides the default agentic loop.',
    )

    console.log('Execution steps:')
    for (const step of result.steps) {
        console.log(`${step.index}. [${step.phase}] ${step.kind}: ${step.detail}`)
    }

    console.log('\nResponse:\n', result.output)
}

main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
