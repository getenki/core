import {
    JsMemoryKind,
    type JsMemoryEntry,
    type JsMemoryModule,
    type JsToolDefinition,
    NativeEnkiAgent,
} from '@getenki/ai'

declare const process: {
    cwd(): string
    exitCode?: number
}

const tools: JsToolDefinition[] = [
    {
        name: 'calculate_sum',
        description: 'Add two numbers and return a short text result.',
        parametersJson: JSON.stringify({
            type: 'object',
            properties: {
                a: {type: 'number'},
                b: {type: 'number'},
            },
            required: ['a', 'b'],
        }),
    },
    {
        name: 'get_today',
        description: 'Return the current local date in ISO format.',
        parametersJson: JSON.stringify({
            type: 'object',
            properties: {},
        }),
    },
]

const memories: JsMemoryModule[] = [{name: 'example-memory'}]
const memoryStore = new Map<string, JsMemoryEntry[]>()

function memoryKey(memoryName: string, sessionId: string): string {
    return `${memoryName}:${sessionId}`
}

function getMemoryEntries(memoryName: string, sessionId: string): JsMemoryEntry[] {
    const key = memoryKey(memoryName, sessionId)
    const existing = memoryStore.get(key)
    if (existing) {
        return existing
    }

    const empty: JsMemoryEntry[] = []
    memoryStore.set(key, empty)
    return empty
}

async function main(): Promise<void> {
    const model = 'ollama::qwen3.5:latest'

    if (!model) {
        throw new Error(
            'Set ENKI_MODEL to a provider/model string, for example `ollama::qwen3.5` or `openai::gpt-4.1-mini`.',
        )
    }

    const agent = NativeEnkiAgent.withToolsAndMemory(
        'Basic TS Agent',
        [
            'Answer clearly and keep responses short.',
            'Use the provided tools when arithmetic or the current date would help.',
            'Use memory to remember stable user preferences between turns.',
        ].join(' '),
        model,
        20,
        process.cwd(),
        tools,
        (toolName: string, argsJson: string): string => {
            const args = argsJson ? (JSON.parse(argsJson) as {a?: number; b?: number}) : {}

            if (toolName === 'calculate_sum') {
                const result = Number(args.a) + Number(args.b)
                return JSON.stringify({result, text: `${args.a} + ${args.b} = ${result}`})
            }

            if (toolName === 'get_today') {
                return JSON.stringify({today: new Date().toISOString().slice(0, 10)})
            }

            return `Unknown tool: ${toolName}`
        },
        memories,
        (memoryName: string, sessionId: string, userMsg: string, assistantMsg: string): void => {
            const entries = getMemoryEntries(memoryName, sessionId)
            entries.push({
                key: `entry-${entries.length + 1}`,
                content: `User: ${userMsg}\nAssistant: ${assistantMsg}`,
                kind: JsMemoryKind.RecentMessage,
                relevance: 1,
                timestampNs: `${Date.now() * 1000000}`,
            })
        },
        (memoryName: string, sessionId: string, query: string, maxEntries: number): JsMemoryEntry[] => {
            const normalizedQuery = query.toLowerCase()

            return getMemoryEntries(memoryName, sessionId)
                .filter((entry) => entry.content.toLowerCase().includes(normalizedQuery))
                .slice(-maxEntries)
        },
        (memoryName: string, sessionId: string): void => {
            memoryStore.delete(memoryKey(memoryName, sessionId))
        },
        (): void => {},
    )

    const sessionId = 'basic-ts-session'

    const first = await agent.run(
        sessionId,
        'My favorite response style is concise. Please remember that. Also calculate 8 + 13.',
    )
    console.log('First run:\n', first)

    const second = await agent.run(
        sessionId,
        'What is today and what response style did I ask you to remember?',
    )
    console.log('\nSecond run:\n', second)
}

main().catch((error: unknown) => {
    console.error(error)
    process.exitCode = 1
})
