const {execFileSync} = require('node:child_process')
const {NativeEnkiAgent} = require('@getenki/ai')

function normalizeOllamaModel(model) {
    if (typeof model !== 'string') {
        return String(model ?? '')
    }

    if (model.includes('::')) {
        const [provider, backendModel] = model.split('::', 2)
        if (provider.trim().toLowerCase() === 'ollama') {
            return backendModel.trim()
        }
    }

    return model
}

function chatWithOllama(model, messages) {
    const baseUrl = (process.env.OLLAMA_URL ?? 'http://127.0.0.1:11434').replace(/\/$/, '')
    const payload = JSON.stringify({
        model: normalizeOllamaModel(model),
        messages,
        stream: false,
    })

    const raw = execFileSync(
        'curl',
        [
            '-sS',
            '-X',
            'POST',
            `${baseUrl}/api/chat`,
            '-H',
            'Content-Type: application/json',
            '-d',
            payload,
        ],
        {encoding: 'utf8'},
    )

    const body = JSON.parse(raw)
    return String(body?.message?.content ?? '').trim()
}

function extractJson(content) {
    const raw = String(content ?? '').trim()
    if (!raw) {
        throw new Error('Expected JSON response from the model, got an empty string.')
    }

    if (raw.startsWith('```')) {
        const parts = raw.split('```')
        for (const part of parts) {
            let candidate = part.trim()
            if (!candidate || candidate.toLowerCase() === 'json') {
                continue
            }
            if (candidate.includes('\n')) {
                candidate = candidate.split('\n').slice(1).join('\n').trim()
            }
            try {
                return JSON.parse(candidate)
            } catch {
            }
        }
    }

    try {
        return JSON.parse(raw)
    } catch {
        const start = raw.indexOf('{')
        const end = raw.lastIndexOf('}')
        if (start === -1 || end === -1 || end <= start) {
            throw new Error(`Expected JSON object, got: ${raw}`)
        }
        return JSON.parse(raw.slice(start, end + 1))
    }
}

async function main() {
    const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
    const toolDefinitions = [
        {
            id: 'lookup_example_topic',
            description: 'Return a canned fact about memory, tools, or the agent loop.',
            inputSchema: {
                type: 'object',
                properties: {
                    topic: {type: 'string'},
                },
                required: ['topic'],
            },
            execute(inputJson) {
                const args = inputJson ? JSON.parse(inputJson) : {}
                const topic = String(args.topic ?? '').toLowerCase()
                const facts = {
                    memory: 'Memory lets Enki retain useful context across turns and sessions.',
                    tools: 'Tools let Enki agents call JavaScript or Python functions for structured results.',
                    'agent-loop': 'The agent loop controls how the model reasons, acts, observes, retries, and finalizes.',
                }
                return facts[topic] ?? `No prepared fact exists for '${topic}'. Try memory, tools, or agent-loop.`
            },
        },
    ]

    const toolExecutors = Object.fromEntries(
        toolDefinitions.map((tool) => [
            tool.id,
            (args = {}) => tool.execute(JSON.stringify(args)),
        ]),
    )

    const agent = NativeEnkiAgent.withTools(
        'JavaScript ReAct Loop Agent',
        'Answer clearly. Use the lookup_example_topic tool when you need facts about memory, tools, or the agent loop.',
        model,
        8,
        process.cwd(),
        toolDefinitions,
        null,
    )

    agent.setAgentLoopHandler((requestJson) => {
        const request = JSON.parse(requestJson)
        const toolCatalog = Object.entries(request.tools ?? {}).map(([name, spec]) => ({
            name,
            description: spec.description ?? '',
            parameters: spec.parameters ?? {},
        }))
        const workingMessages = [
            {
                role: 'system',
                content: [
                    'You are operating a ReAct loop for an Enki agent.',
                    'You must respond with JSON only.',
                    'If you need a tool, reply with {"thought":"...","action":{"name":"tool_name","args":{...}}}.',
                    'If you are ready to finish, reply with {"thought":"...","final":"..."}.',
                    `Available tools: ${JSON.stringify(toolCatalog)}`,
                ].join('\n'),
            },
            {
                role: 'system',
                content: `Original agent instructions:\n${request.system_prompt}`,
            },
            ...(request.messages ?? []),
        ]
        const steps = []
        const maxTurns = Math.max(1, Math.min(Number(request.max_iterations ?? 6), 6))

        for (let turn = 1; turn <= maxTurns; turn += 1) {
            const content = chatWithOllama(request.model, workingMessages)
            const decision = extractJson(content)
            const thought = String(decision.thought ?? 'No thought provided.')

            steps.push({
                index: steps.length + 1,
                phase: 'ReAct',
                kind: 'thought',
                detail: `Turn ${turn}: ${thought}`,
            })

            if (typeof decision.final === 'string' && decision.final.trim()) {
                steps.push({
                    index: steps.length + 1,
                    phase: 'ReAct',
                    kind: 'final',
                    detail: 'Returned a final answer from the JavaScript ReAct loop',
                })

                return JSON.stringify({
                    content: decision.final.trim(),
                    steps,
                })
            }

            const action = decision.action ?? {}
            const toolName = String(action.name ?? '').trim()
            const toolArgs = action.args ?? {}
            steps.push({
                index: steps.length + 1,
                phase: 'ReAct',
                kind: 'action',
                detail: `Calling ${toolName} with ${JSON.stringify(toolArgs)}`,
            })

            const executeTool = toolExecutors[toolName]
            const observation = executeTool
                ? String(executeTool(toolArgs))
                : `Unknown tool '${toolName}'.`

            steps.push({
                index: steps.length + 1,
                phase: 'ReAct',
                kind: 'observation',
                detail: observation,
            })

            workingMessages.push({role: 'assistant', content})
            workingMessages.push({role: 'user', content: `Observation: ${observation}`})
        }

        steps.push({
            index: steps.length + 1,
            phase: 'ReAct',
            kind: 'stop',
            detail: 'Reached the example loop turn limit',
        })
        return JSON.stringify({
            content: 'Max ReAct turns reached without producing a final answer.',
            steps,
        })
    })

    const result = await agent.runWithTrace(
        'basic-js-react-custom-agent-loop-session',
        'Use ReAct to explain how Enki tools and the agent loop fit together.',
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
