---
sidebar_position: 6
slug: /wasm-usage
---

# WASM Usage

Use the `enki-js` binding when you want to run Enki in a browser or another JavaScript runtime that can load WebAssembly.

The current binding is built with `wasm-bindgen` and is intentionally browser-safe:

- Session state is stored in memory inside the WASM agent
- Filesystem-backed persistence is not used
- LLM execution is delegated to a JavaScript callback
- Tool execution is delegated to an optional JavaScript callback

## Install from npm

Install the published package:

```bash
npm install enki-js
```

Then import it directly:

```js
import init, { EnkiJsAgent } from "enki-js";
```

## Create an agent

The published package exports `init()` plus `EnkiJsAgent`.

```js
import init, { EnkiJsAgent } from "enki-js";

await init();
```

## Build from source

From `crates/bindings/enki-js`:

```bash
wasm-pack build --target bundler --out-dir pkg
```

If you need the `web` target instead:

```bash
wasm-pack build --target web --out-dir pkg-web
```

## Full example

```js
import init, { EnkiJsAgent } from "enki-js";

await init();

const llmHandler = async ({ agent, messages, tools }) => {
  const last = messages[messages.length - 1];

  if (last.role === "user") {
    return {
      content: "",
      tool_calls: [
        {
          id: "call-1",
          function: {
            name: "echo",
            arguments: { value: last.content }
          }
        }
      ]
    };
  }

  if (last.role === "tool") {
    return `Tool said: ${last.content}`;
  }

  return `No action taken for model ${agent.model}. Available tools: ${tools.length}`;
};

const toolHandler = async ({ tool, args, context }) => {
  if (tool === "echo") {
    return `echo:${args.value} from ${context.workspace_dir}`;
  }

  return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
  "Example Agent",
  "Use the echo tool before answering.",
  "js::demo",
  4,
  llmHandler,
  toolHandler,
  [
    {
      name: "echo",
      description: "Echo a value back to the agent",
      parameters_json: JSON.stringify({
        type: "object",
        properties: {
          value: { type: "string" }
        },
        required: ["value"]
      })
    }
  ]
);

const result = await agent.run("demo-session", "hello from javascript");
console.log(result);
```

## Constructor

`EnkiJsAgent` takes these arguments in order:

1. `name?: string`
2. `system_prompt_preamble?: string`
3. `model?: string`
4. `max_iterations?: number`
5. `llm_handler: Function`
6. `tool_handler?: Function | null`
7. `tools: EnkiJsTool[]`

`EnkiJsTool` has:

- `name`
- `description`
- `parameters_json`

`parameters_json` should be a JSON schema string for the tool arguments.

## LLM callback contract

The LLM callback receives an object shaped like:

```json
{
  "agent": {
    "name": "Personal Assistant",
    "system_prompt_preamble": "...",
    "model": "js::callback",
    "max_iterations": 20
  },
  "messages": [
    { "role": "system", "content": "..." }
  ],
  "tools": [
    {
      "name": "echo",
      "description": "Echo a value",
      "parameters": {
        "type": "object"
      }
    }
  ]
}
```

It must return either:

- A final string response
- An object with `content` and optional `tool_calls`

Each `tool_calls` entry should look like:

```json
{
  "id": "call-1",
  "function": {
    "name": "echo",
    "arguments": { "value": "hello" }
  }
}
```

If your model does not support native tool calling, the runtime also accepts a fallback text payload shaped like:

```json
{
  "tool": "echo",
  "args": { "value": "hello" }
}
```

## Tool callback contract

The tool callback is optional. When present, it receives:

```json
{
  "tool": "echo",
  "args": { "value": "hello" },
  "context": {
    "agent_dir": "agent",
    "workspace_dir": "workspace",
    "sessions_dir": "sessions"
  }
}
```

Return a string. That string is added back into the conversation as the tool result.

If no tool callback is configured, tool calls fail with an error message returned to the agent loop.

## Running sessions

Use `run(sessionId, userMessage)` to continue or start a session:

```js
const first = await agent.run("session-1", "Say hello");
const second = await agent.run("session-1", "Now summarize the previous answer");
```

Messages for the same `sessionId` stay in memory for the lifetime of that `EnkiJsAgent` instance.

## Introspection

Use `toolCatalogJson()` if you want the current tool registry as JSON:

```js
console.log(agent.toolCatalogJson());
```

## Current limitations

- Persistence is in-memory only
- Native process execution is not available from WASM
- Filesystem tools should be implemented in JavaScript if you need them
- The browser host is responsible for authentication, networking, and model API calls

For Rust-side implementation details, see `crates/bindings/enki-js/src/wasm.rs`.
