/**
 * Enki Multi-Agent Starter — TypeScript
 *
 * Run with:   npx tsx src/index.ts
 * Or via CLI: enki run --message "Hello!"
 */

import { NativeEnkiAgent, NativeMultiAgentRuntime, type JsMultiAgentMember } from '@getenki/ai'

declare const process: {
    env: Record<string, string | undefined>
    argv: string[]
}

const model = process.env.ENKI_MODEL ?? 'ollama::qwen3.5:latest'
const workspaceHome = '.'

// ── Define agents ─────────────────────────────────────────────────────────

const members: JsMultiAgentMember[] = [
    {
        agentId: 'assistant',
        name: 'Personal Assistant',
        systemPromptPreamble:
            'You are a helpful personal assistant. Answer questions clearly and concisely.',
        model,
        maxIterations: 20,
        capabilities: ['general', 'writing', 'analysis'],
    },
]

// ── Main ──────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
    console.log('⚡ Enki Multi-Agent Runtime')
    console.log()

    // Create a standalone agent
    const agent = new NativeEnkiAgent('Personal Assistant', model, 20, workspaceHome)

    const message = process.argv[2] ?? 'Hello! What can you help me with?'
    console.log(`> ${message}`)
    console.log()

    const response = await agent.run('session-1', message)
    console.log(String(response))
}

main().catch(console.error)
