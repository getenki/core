const {JsAgentStatus, NativeMultiAgentRuntime} = require('@getenki/ai')

function printCards(label, cards) {
    console.log(label)
    for (const card of cards) {
        console.log(
            `- ${card.agentId} (${card.name}) capabilities=${card.capabilities.join(', ')} status=${card.status}`,
        )
    }
}

async function main() {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'

    const runtime = new NativeMultiAgentRuntime(
        [
            {
                agentId: 'coordinator',
                name: 'Coordinator',
                systemPromptPreamble: [
                    'You are a coordinator agent.',
                    'Use discover_agents to inspect peers.',
                    'Use delegate_task when research or investigation should be handled by another agent.',
                    'Keep the final answer concise.',
                ].join(' '),
                model,
                maxIterations: 20,
                capabilities: ['planning', 'orchestration'],
            },
            {
                agentId: 'researcher',
                name: 'Researcher',
                systemPromptPreamble: [
                    'You are a researcher agent.',
                    'Handle delegated investigation tasks carefully and return short factual answers.',
                ].join(' '),
                model,
                maxIterations: 20,
                capabilities: ['research', 'analysis'],
            },
        ],
        process.cwd(),
    )

    const allCards = await runtime.registry()
    printCards('Registered agents:', allCards)

    const researchCards = await runtime.discover('research', JsAgentStatus.Online)
    printCards('\nResearch-capable agents:', researchCards)

    const response = await runtime.process(
        'coordinator',
        'basic-js-multi-agent-session',
        [
            'Please use discover_agents first.',
            'Then delegate_task to the researcher to answer this question: what is the purpose of this example?',
            'Return the delegated answer and mention which agent handled it.',
        ].join(' '),
    )

    console.log('\nCoordinator response:\n', response)
}

main().catch((error) => {
    console.error(error)
    process.exitCode = 1
})
