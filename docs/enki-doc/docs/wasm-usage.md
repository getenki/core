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

## Step-by-step agent setup

This is the shortest accurate path to creating a JS agent in this repo. It matches `crates/bindings/enki-js/examples/basic.mjs` and `example/basic-js/src/App.js`.

### 1. Understand where the binding comes from

`crates/bindings/enki-js/src/lib.rs` only wires the crate up for `wasm32` and re-exports the `wasm` module:

- on `wasm32`, `mod wasm;`
- on `wasm32`, `pub use wasm::*;`

That means the JavaScript API you use is defined by the WASM binding layer, while `lib.rs` is just the Rust entrypoint.

### 2. Initialize the generated package

```js
import init, { EnkiJsAgent } from "enki-js";

await init();
```

If you are working from source instead of npm, the local example imports from `../pkg/enki_js.js` after building the package.

### 3. Define the model string

```js
const model = "google::gemini-3.1-pro-preview";
```

This string is passed back into your callback as `agent.model`. It is metadata plus a routing hint for your JavaScript LLM code.

### 4. Translate Enki messages into your provider format

The examples convert Enki messages into Gemini request payloads:

```js
const toGeminiModel = (value) =>
  value.startsWith("google::") ? value.slice("google::".length) : value;

const toGeminiContent = (message) => ({
  role: message.role === "assistant" ? "model" : "user",
  parts: [{ text: message.content }]
});
```

### 5. Implement the LLM callback

```js
const apiKey =
  process.env.GOOGLE_AI_STUDIO_API_KEY ??
  process.env.GEMINI_API_KEY;

const llmHandler = async ({ agent, messages }) => {
  const systemText = messages
    .filter((message) => message.role === "system")
    .map((message) => message.content)
    .join("\n\n")
    .trim();

  const contents = messages
    .filter((message) => message.role !== "system")
    .map(toGeminiContent);

  const response = await fetch(
    `https://generativelanguage.googleapis.com/v1beta/models/${toGeminiModel(agent.model)}:generateContent`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-goog-api-key": apiKey
      },
      body: JSON.stringify({
        contents,
        systemInstruction: systemText
          ? { parts: [{ text: systemText }] }
          : undefined
      })
    }
  );

  const body = await response.json();
  const parts = body.candidates?.[0]?.content?.parts ?? [];
  return parts.map((part) => part.text).filter(Boolean).join("\n").trim();
};
```

The important point is that Enki owns the agent loop, but your callback owns the provider call.

### 6. Create the agent instance

```js
const agent = new EnkiJsAgent(
  "Simple Example Agent",
  "Answer clearly and concisely.",
  model,
  1,
  llmHandler,
  null,
  []
);
```

This example passes `null` for `tool_handler` and `[]` for tools, so it is a pure prompt/response agent with no tool calling.

### 7. Wait for readiness in browser apps

The React example calls:

```js
await agent.ready();
```

Do that before enabling UI actions that call `run()`.

### 8. Run a session

```js
const result = await agent.run(
  `example-${Date.now()}`,
  "Explain in two sentences what EnkiJS agents are."
);
```

Reusing the same session id continues the conversation. Changing it starts a new in-memory session.

### 9. Release resources in long-lived apps

When the browser example unmounts, it calls:

```js
agent.free();
```

That is the right cleanup path for a WASM-backed object in UI code.

## Build from source

From `crates/bindings/enki-js`:

```bash
wasm-pack build --target bundler --out-dir pkg
```

If you need the `web` target instead:

```bash
wasm-pack build --target web --out-dir pkg-web
```

## Minimal example from this repo

```js
import init, { EnkiJsAgent } from "enki-js";

await init();

const model = "google::gemini-3.1-pro-preview";
const apiKey =
  process.env.GOOGLE_AI_STUDIO_API_KEY ??
  process.env.GEMINI_API_KEY;

const toGeminiModel = (value) =>
  value.startsWith("google::") ? value.slice("google::".length) : value;

const toGeminiContent = (message) => ({
  role: message.role === "assistant" ? "model" : "user",
  parts: [{ text: message.content }]
});

const llmHandler = async ({ agent, messages }) => {
  const systemText = messages
    .filter((message) => message.role === "system")
    .map((message) => message.content)
    .join("\n\n")
    .trim();

  const contents = messages
    .filter((message) => message.role !== "system")
    .map(toGeminiContent);

  const response = await fetch(
    `https://generativelanguage.googleapis.com/v1beta/models/${toGeminiModel(agent.model)}:generateContent`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-goog-api-key": apiKey
      },
      body: JSON.stringify({
        contents,
        systemInstruction: systemText
          ? { parts: [{ text: systemText }] }
          : undefined
      })
    }
  );

  if (!response.ok) {
    throw new Error(
      `Google AI Studio request failed: ${response.status} ${await response.text()}`
    );
  }

  const body = await response.json();
  const parts = body.candidates?.[0]?.content?.parts ?? [];
  const text = parts
    .map((part) => part.text)
    .filter(Boolean)
    .join("\n")
    .trim();

  return text || "No response returned.";
};

const agent = new EnkiJsAgent(
  "Simple Example Agent",
  "Answer clearly and concisely.",
  model,
  1,
  llmHandler,
  null,
  []
);

const result = await agent.run(
  "demo-session",
  "Explain in two sentences what EnkiJS agents are."
);
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

## Tool calling example

This example shows the full JS tool-calling path: the LLM callback asks for a tool call, the tool handler executes it, and the runtime feeds the result back into the conversation.

```js
import init, { EnkiJsAgent } from "enki-js";

await init();

const llmHandler = async ({ messages }) => {
  const last = messages[messages.length - 1];

  if (last.role === "user") {
    return {
      content: "Calling the echo tool.",
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
    return `Tool result: ${last.content}`;
  }

  return "No action taken.";
};

const toolHandler = async ({ tool, args, context }) => {
  if (tool === "echo") {
    return `echo:${args.value} from ${context.workspace_dir}`;
  }

  return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
  "Tool Example Agent",
  "Use the echo tool before answering.",
  "js::tool-demo",
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

const result = await agent.run("tool-session", "hello from javascript");
console.log(result);
```

## Memory support in JavaScript

The JS binding does not currently expose a dedicated `MemoryBackend` or `MemoryModule` API.

What you do get today:

- In-memory session history inside each `EnkiJsAgent`
- The ability to keep your own memory store in JavaScript and inject recalled context into prompts
- The ability to expose memory read/write operations through JS tools

The internal session history comes from the binding itself. In `crates/bindings/enki-js/src/wasm.rs`, each agent stores a `sessions` map keyed by `sessionId`, and `run()` reuses those messages when you call the same session again.

## App-managed memory example

This pattern gives you memory behavior in JavaScript without a formal memory module API.

```js
import init, { EnkiJsAgent } from "enki-js";

await init();

const memoryStore = new Map();

const recallMemory = (sessionId) => memoryStore.get(sessionId) ?? [];

const storeExchange = (sessionId, userMessage, assistantMessage) => {
  const entries = recallMemory(sessionId);
  entries.push({ role: "user", content: userMessage });
  entries.push({ role: "assistant", content: assistantMessage });
  memoryStore.set(sessionId, entries.slice(-8));
};

const llmHandler = async ({ messages }) => {
  const lastUser = [...messages].reverse().find((message) => message.role === "user");
  return `Using the supplied memory, I think you are asking about: ${lastUser?.content ?? ""}`;
};

const agent = new EnkiJsAgent(
  "Memory Example Agent",
  "Use any recalled notes that appear in the user message.",
  "js::memory-demo",
  1,
  llmHandler,
  null,
  []
);

const runWithMemory = async (sessionId, userMessage) => {
  const recalled = recallMemory(sessionId);
  const memoryBlock = recalled.length
    ? `Remember these notes:\n${recalled.map((item) => `${item.role}: ${item.content}`).join("\n")}\n\n`
    : "";

  const output = await agent.run(sessionId, `${memoryBlock}${userMessage}`);
  storeExchange(sessionId, userMessage, output);
  return output;
};
```

If you need richer memory semantics, implement them in your host app or expose them as tools. The Python-style backend contract is not available in JS yet.

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
- There is no first-class JavaScript memory module API yet
- Native process execution is not available from WASM
- Filesystem tools should be implemented in JavaScript if you need them
- The browser host is responsible for authentication, networking, and model API calls

For Rust-side implementation details, see `crates/bindings/enki-js/src/wasm.rs`.
