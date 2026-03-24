import {
    JsMemoryKind,
    NativeEnkiAgent,
    type JsMemoryEntry,
    type JsMemoryModule,
} from '@getenki/ai'

declare const process: {
    cwd(): string
    env: Record<string, string | undefined>
    exitCode?: number
}

type MemoryHandlers = {
    modules: JsMemoryModule[]
    store: Map<string, JsMemoryEntry[]>
    record: (memoryName: string, sessionId: string, userMsg: string, assistantMsg: string) => void
    recall: (memoryName: string, sessionId: string, query: string, maxEntries: number) => JsMemoryEntry[]
    flush: (memoryName: string, sessionId: string) => void
    consolidate: (memoryName: string, sessionId: string) => void
}

function createSharedMemory(): MemoryHandlers {
    const store = new Map<string, JsMemoryEntry[]>()

    function key(memoryName: string, sessionId: string): string {
        return `${memoryName}:${sessionId}`
    }

    function getEntries(memoryName: string, sessionId: string): JsMemoryEntry[] {
        const memoryKey = key(memoryName, sessionId)
        const existing = store.get(memoryKey)
        if (existing) {
            return existing
        }

        const entries: JsMemoryEntry[] = []
        store.set(memoryKey, entries)
        return entries
    }

    return {
        modules: [{name: 'shared-session-memory'}],
        store,
        record(memoryName: string, sessionId: string, userMsg: string, assistantMsg: string): void {
            const entries = getEntries(memoryName, sessionId)
            const timestampNs = `${Date.now() * 1_000_000}`

            entries.push(
                {
                    key: `${sessionId}-user-${entries.length + 1}`,
                    content: userMsg,
                    kind: JsMemoryKind.RecentMessage,
                    relevance: 1,
                    timestampNs,
                },
                {
                    key: `${sessionId}-assistant-${entries.length + 1}`,
                    content: assistantMsg,
                    kind: JsMemoryKind.RecentMessage,
                    relevance: 0.9,
                    timestampNs,
                },
            )
        },
        recall(memoryName: string, sessionId: string, query: string, maxEntries: number): JsMemoryEntry[] {
            const entries = getEntries(memoryName, sessionId)
            const terms = query
                .toLowerCase()
                .split(/\s+/)
                .map((term) => term.trim())
                .filter(Boolean)

            if (terms.length === 0) {
                return entries.slice(-maxEntries)
            }

            return entries
                .filter((entry) => terms.some((term) => entry.content.toLowerCase().includes(term)))
                .slice(-maxEntries)
        },
        flush(memoryName: string, sessionId: string): void {
            store.delete(key(memoryName, sessionId))
        },
        consolidate(): void {
        },
    }
}

function printMemory(label: string, entries: JsMemoryEntry[]): void {
    console.log(label)
    for (const entry of entries) {
        console.log(`- ${entry.kind}: ${entry.content}`)
    }
}

async function runAndRecord(
    agent: NativeEnkiAgent,
    sharedMemory: MemoryHandlers,
    sessionId: string,
    userMessage: string,
): Promise<string> {
    const output = String(await agent.run(sessionId, userMessage))
    sharedMemory.record('shared-session-memory', sessionId, userMessage, output)
    return output
}

async function main(): Promise<void> {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
    const workspaceHome = process.cwd()
    const sharedMemory = createSharedMemory()
    const sessionId = 'basic-ts-multi-agent-tools-memory'
    let latestResearchBriefing = 'No research briefing is available yet.'
    const preferencePrompt = 'Remember that the user cares about memory and tool calling in Enki examples.'
    const researchPrompt = [
        'Use the lookup_example_topics tool.',
        'Give short explanations for memory and tools in Enki.',
        'Mention that you are the researcher.',
    ].join(' ')
    const coordinatorPrompt = [
        'Use the read_research_briefing tool first.',
        'Summarize the researcher handoff about memory and tools in Enki.',
        'Also mention the remembered user preference if available.',
    ].join(' ')

    const researcher = NativeEnkiAgent.withToolsAndMemory(
        'Researcher',
        [
            'You are a researcher agent.',
            'Use your tools to answer factual questions.',
            'Keep the response concise and grounded in tool output.',
        ].join(' '),
        model,
        20,
        workspaceHome,
        [
            {
                id: 'lookup_example_topics',
                description: 'Return a prepared fact about memory, tools, or multi-agent runtimes.',
                inputSchema: {
                    type: 'object',
                    properties: {
                        topic: {type: 'string'},
                    },
                    required: ['topic'],
                },
                execute(inputJson: string): string {
                    const args = inputJson ? (JSON.parse(inputJson) as {topic?: string}) : {}
                    const topic = String(args.topic ?? '').toLowerCase()
                    const facts: Record<string, string> = {
                        memory: 'Memory lets the agent persist and recall useful session context.',
                        tools: 'Tools let the agent call TypeScript functions for structured results.',
                        'multi-agent': 'Multi-agent setups let a coordinator route tasks to specialized agents.',
                    }

                    return JSON.stringify({
                        topic,
                        fact: facts[topic] ?? `No prepared fact exists for '${topic}'.`,
                    })
                },
            },
        ],
        null,
        sharedMemory.modules,
        sharedMemory.record,
        sharedMemory.recall,
        sharedMemory.flush,
        sharedMemory.consolidate,
    )

    const coordinator = NativeEnkiAgent.withToolsAndMemory(
        'Coordinator',
        [
            'You are a coordinator agent.',
            'Use the read_research_briefing tool to inspect the researcher handoff.',
            'Use recalled memory when it helps.',
            'Mention which agent handled the research.',
        ].join(' '),
        model,
        20,
        workspaceHome,
        [
            {
                id: 'read_research_briefing',
                description: 'Return the latest handoff prepared by the researcher agent.',
                inputSchema: {
                    type: 'object',
                    properties: {},
                },
                execute(): string {
                    return JSON.stringify({
                        agentId: 'researcher',
                        briefing: latestResearchBriefing,
                    })
                },
            },
        ],
        null,
        sharedMemory.modules,
        sharedMemory.record,
        sharedMemory.recall,
        sharedMemory.flush,
        sharedMemory.consolidate,
    )

    console.log(`Model: ${model}`)
    console.log('Researcher: saving user preference...')
    await runAndRecord(researcher, sharedMemory, sessionId, preferencePrompt)

    console.log('Researcher: preparing briefing...')
    latestResearchBriefing = await runAndRecord(researcher, sharedMemory, sessionId, researchPrompt)

    console.log('Coordinator: preparing final response...')
    const response = await runAndRecord(coordinator, sharedMemory, sessionId, coordinatorPrompt)

    console.log('Coordinator response:\n')
    console.log(response)

    const remembered = sharedMemory.recall(
        'shared-session-memory',
        sessionId,
        'memory tool calling user cares',
        6,
    )
    printMemory('\nShared memory snapshot:', remembered)
}

main().catch((error: unknown) => {
    console.error(error)
    process.exitCode = 1
})
