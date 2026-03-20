# `@getenki/ai`

JavaScript bindings for Enki's Rust agent runtime, published as a native Node.js package via `napi-rs`.

## Install

```bash
npm install @getenki/ai
```

The package ships prebuilt native binaries for:

- Windows x64 and arm64
- macOS x64 and arm64
- Linux x64 and arm64 (GNU libc)

## API

The package exposes two layers:

- `EnkiAgent`: thin wrapper over the native runtime
- `Agent`: higher-level JavaScript wrapper for tools, memories, and custom LLM providers

It also exports `NativeEnkiAgent`, `Tool`, `MemoryModule`, `MemoryBackend`, `LlmProviderBackend`, `RunContext`, and `AgentRunResult`.

## `EnkiAgent`

Use `EnkiAgent` when you want a simple session-oriented interface backed directly by the native runtime.

```js
const { EnkiAgent } = require('@getenki/ai')

async function main() {
  const agent = new EnkiAgent({
    name: 'Assistant',
    systemPromptPreamble: 'Answer clearly and keep responses short.',
    model: 'ollama::llama3.2:latest',
    maxIterations: 20,
    workspaceHome: process.cwd(),
  })

  const output = await agent.run('session-1', 'Explain what this project does.')
  console.log(output)
}

main().catch(console.error)
```

Constructor options:

- `name?: string`
- `systemPromptPreamble?: string`
- `model?: string`
- `maxIterations?: number`
- `workspaceHome?: string`

## `Agent`

Use `Agent` when you want to register JavaScript tools or plug in your own LLM provider.

```js
const { Agent } = require('@getenki/ai')

async function main() {
  const agent = new Agent('demo-model', {
    instructions: 'You are a dice game.',
    workspaceHome: process.cwd(),
  })

  agent.toolPlain(
    function rollDice() {
      return '4'
    },
    {
      description: 'Roll a six-sided die and return the result.',
      parametersJson: JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
    },
  )

  const result = await agent.run('My guess is 4', {
    sessionId: 'session-tools-1',
  })

  console.log(result.output)
}

main().catch(console.error)
```

### Context-aware tools

`agent.tool()` injects a `RunContext` as the first argument so your tool can access runtime dependencies.

```js
const { Agent } = require('@getenki/ai')

const agent = new Agent('demo-model')

agent.tool(
  function getPlayerName(ctx) {
    return ctx.deps.playerName
  },
  {
    description: "Get the player's name.",
    parametersJson: JSON.stringify({
      type: 'object',
      properties: {},
      additionalProperties: false,
    }),
  },
)
```

Then pass dependencies at run time:

```js
const result = await agent.run('Say hello.', {
  sessionId: 'session-ctx-1',
  deps: { playerName: 'Anne' },
})
```

## Custom LLM providers

Pass either a subclass of `LlmProviderBackend` or a function through the `llm` option.

```js
const { Agent, LlmProviderBackend } = require('@getenki/ai')

class DemoProvider extends LlmProviderBackend {
  complete(model, messages, tools) {
    return {
      model,
      content: `Received ${messages.length} message(s) and ${tools.length} tool(s).`,
    }
  }
}

const agent = new Agent('demo-model', {
  llm: new DemoProvider(),
})
```

## Development

From [`crates/bindings/enki-js`](/I:/projects/enki/core-next/crates/bindings/enki-js):

```bash
npm install
npm run build
npm test
```

Useful scripts:

- `npm run build`: build the native addon in release mode
- `npm run build:debug`: build without release optimizations
- `npm test`: run the AVA test suite
- `npm run lint`: run `oxlint`
- `npm run format`: run Prettier, `cargo fmt`, and `taplo format`

## Notes

- `Agent` can register tools, memories, and custom LLM providers in JavaScript.
- The default native constructor uses `20` max iterations when none is provided.
- `workspaceHome` lets you control where the runtime creates and resolves workspace state.
