import { NativeEnkiAgent } from '@getenki/ai'

async function main(): Promise<void> {
    const model = 'ollama::qwen3.5:latest'

    if (!model) {
        throw new Error(
            'Set ENKI_MODEL to a provider/model string, for example `ollama::qwen3.5` or `openai::gpt-4.1-mini`.',
        )
    }

    const agent = new NativeEnkiAgent(
        'Basic TS Agent',
        'Answer clearly and keep responses short.',
        model,
        20,
        process.cwd(),
    )

    const output = await agent.run('basic-ts-session', 'Calculate 8 + 13 and explain the result briefly.')
    console.log(output)
}

main().catch((error: unknown) => {
    console.error(error)
    process.exitCode = 1
})
