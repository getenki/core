import {
    JsMemoryKind,
    type JsMemoryEntry,
    type JsMemoryModule,
    NativeEnkiAgent,
} from '@getenki/ai'

declare const process: {
    cwd(): string
    exitCode?: number
}

type ExampleTool = {
    id: string
    description: string
    inputSchema: Record<string, unknown>
    execute: (inputJson: string, contextJson: string) => string
}

const tools: ExampleTool[] = [
    {
        id: 'calculate_sum',
        description: 'Add two numbers and return a short text result.',
        inputSchema: {
            type: 'object',
            properties: {
                a: {type: 'number'},
                b: {type: 'number'},
            },
            required: ['a', 'b'],
        },
        execute: (inputJson: string): string => {
            const args = inputJson ? (JSON.parse(inputJson) as {a?: number; b?: number}) : {}
            const result = Number(args.a) + Number(args.b)
            return JSON.stringify({result, text: `${args.a} + ${args.b} = ${result}`})
        },
    },
    {
        id: 'get_today',
        description: 'Return the current local date in ISO format.',
        inputSchema: {
            type: 'object',
            properties: {},
        },
        execute: (): string => JSON.stringify({today: new Date().toISOString().slice(0, 10)}),
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
        null,
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
