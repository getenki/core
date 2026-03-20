const {NativeEnkiAgent} = require('@getenki/ai')

async function main() {
    const model = "ollama::qwen3.5:latest"

    if (!model) {
        throw new Error(
            'Set ENKI_MODEL to a provider/model string, for example `ollama::qwen3.5` or `openai::gpt-4.1-mini`.',
        )
    }

    const agent = new NativeEnkiAgent(
        'Basic JS Agent',
        'Answer clearly and keep responses short.',
        model,
        20,
        process.cwd(),
    )

    const output = await agent.run('basic-js-session', 'Calculate 3 + 5 and explain the result briefly.')
    console.log(output)
}

main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
