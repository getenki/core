import {
    JsAgentStatus,
    NativeMultiAgentRuntime,
    type JsAgentCard,
    type JsAgentRunResult,
    type JsMultiAgentMember,
} from '@getenki/ai'

declare const process: {
    cwd(): string
    env: Record<string, string | undefined>
    exitCode?: number
}

function printCards(label: string, cards: JsAgentCard[]): void {
    console.log(label)
    for (const card of cards) {
        console.log(
            `- ${card.agentId} (${card.name}) capabilities=${card.capabilities.join(', ')} status=${card.status}`,
        )
    }
}

async function main(): Promise<void> {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'

    const members: JsMultiAgentMember[] = [
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
    ]

    const runtime = new NativeMultiAgentRuntime(members, process.cwd())

    const allCards = (await runtime.registry()) as JsAgentCard[]
    printCards('Registered agents:', allCards)

    const researchCards = (await runtime.discover('research', JsAgentStatus.Online)) as JsAgentCard[]
    printCards('\nResearch-capable agents:', researchCards)

    const result = await runtime.processWithTrace(
        'coordinator',
        'basic-ts-multi-agent-session',
        [
            'Please use discover_agents first.',
            'Then delegate_task to the researcher to answer this question: what is the purpose of this example?',
            'Return the delegated answer and mention which agent handled it.',
        ].join(' '),
    ) as JsAgentRunResult

    console.log('\nExecution steps:')
    for (const step of result.steps) {
        console.log(`${step.index}. [${step.phase}] ${step.kind}: ${step.detail}`)
    }

    console.log('\nCoordinator response:\n', result.output)
}

main().catch((error: unknown) => {
    console.error(error)
    process.exitCode = 1

})
