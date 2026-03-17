---
sidebar_position: 1
slug: /javascript
---

# JavaScript

`enki-js` is the browser-oriented JavaScript binding for Enki, built with `wasm-bindgen`.

Use it when you want to run the Rust agent loop through WebAssembly and keep LLM and tool execution in JavaScript callbacks.

## What it exposes

- `EnkiJsTool`: tool metadata registered from JavaScript
- `EnkiJsAgent`: a WASM-backed agent instance with JavaScript-provided LLM and tool handlers

## Runtime model

The current JavaScript binding is intentionally browser-safe:

- Session state is stored in memory
- Filesystem-backed persistence is not used
- LLM execution is delegated to a JavaScript callback
- Tool execution is delegated to an optional JavaScript callback

## Install

Install the published package from npm:

```bash
npm install enki-js
```

Then import it directly from your application:

```js
import init, { EnkiJsAgent } from "enki-js";
```

## How agent creation works

Creating a JavaScript agent has three layers in this repository:

1. `crates/bindings/enki-js/src/lib.rs` is the crate entrypoint. It enables the `wasm` module on `wasm32` and re-exports the public bindings.
2. The actual JavaScript-facing API is implemented in `crates/bindings/enki-js/src/wasm.rs`, which exposes `EnkiJsAgent`.
3. You consume that API from JavaScript, as shown in `crates/bindings/enki-js/examples/basic.mjs` and the browser example `example/basic-js/src/App.js`.

## Create an agent step by step

The current examples both follow the same sequence.

### 1. Import and initialize the WASM package

```js
import init, { EnkiJsAgent } from "enki-js";

await init();
```

`init()` loads the generated WebAssembly module. You must do this before running the agent.

### 2. Choose the model identifier you want the agent to carry

```js
const model = "google::gemini-3.1-pro-preview";
```

Enki stores this string on the agent and passes it into your LLM callback. The JavaScript binding does not call the model provider for you.

### 3. Write the LLM callback

Your callback receives the agent config plus the current conversation messages. It is responsible for calling your model API and returning the final assistant text or a tool-call payload.

The examples use Google AI Studio:

```js
const llmHandler = async ({ agent, messages }) => {
  const systemText = messages
    .filter((message) => message.role === "system")
    .map((message) => message.content)
    .join("\n\n")
    .trim();

  const contents = messages
    .filter((message) => message.role !== "system")
    .map((message) => ({
      role: message.role === "assistant" ? "model" : "user",
      parts: [{ text: message.content }]
    }));

  const response = await fetch(/* provider request */);
  const body = await response.json();

  return body.candidates?.[0]?.content?.parts
    ?.map((part) => part.text)
    .filter(Boolean)
    .join("\n")
    .trim() || "No response returned.";
};
```

### 4. Construct `EnkiJsAgent`

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

The arguments are:

1. Agent name
2. System prompt preamble
3. Model label
4. Maximum loop iterations
5. Async LLM callback
6. Optional async tool callback, or `null`
7. Tool definitions array

### 5. Wait for readiness in browser code

The React example creates the agent, then waits for the WASM wrapper to finish booting:

```js
await agent.ready();
```

This pattern is used in `example/basic-js/src/App.js` before enabling the UI.

### 6. Run the agent with a session id

```js
const result = await agent.run(
  "example-session",
  "Explain in two sentences what EnkiJS agents are."
);
```

Messages are kept in memory per `sessionId` for the lifetime of that `EnkiJsAgent` instance.

### 7. Free the agent when your app disposes it

In React, release the WASM-backed object during cleanup:

```js
agent.free();
```

That is what `example/basic-js/src/App.js` does inside the `useEffect` cleanup.

## Tool calling example

JavaScript tool calling is supported by passing both a `tool_handler` callback and a `tools` array into `EnkiJsAgent`.

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

const toolHandler = async ({ tool, args }) => {
  if (tool === "echo") {
    return `echo:${args.value}`;
  }

  return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
  "Tool Example Agent",
  "Use the available tools when needed.",
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

The tool call is generated by the LLM callback, executed by the JavaScript `toolHandler`, then fed back into the conversation by the Enki loop.

## Memory in JavaScript

There is currently no JavaScript equivalent of Python's `MemoryBackend` or `MemoryModule`.

What the JS binding supports today:

- Per-session conversation history stored in memory inside `EnkiJsAgent`
- App-managed memory that you keep in JavaScript and inject into prompts or expose through tools

The internal session memory is automatic. Reusing the same `sessionId` keeps the message history for that agent instance.

## App-managed memory example

If you need memory beyond the built-in session transcript, keep it in JavaScript and surface it through your callback logic.

```js
import init, { EnkiJsAgent } from "enki-js";

await init();

const memories = new Map();

const remember = (sessionId, role, content) => {
  const entries = memories.get(sessionId) ?? [];
  entries.push({ role, content });
  memories.set(sessionId, entries.slice(-6));
};

const llmHandler = async ({ messages }) => {
  const lastUser = [...messages].reverse().find((message) => message.role === "user");
  return `I remember your earlier context and your latest message was: ${lastUser?.content ?? ""}`;
};

const runWithMemory = async (agent, sessionId, userMessage) => {
  const recalled = memories.get(sessionId) ?? [];
  const memoryPrefix = recalled.length
    ? `Previous notes:\n${recalled.map((item) => `${item.role}: ${item.content}`).join("\n")}\n\n`
    : "";

  const output = await agent.run(sessionId, `${memoryPrefix}${userMessage}`);
  remember(sessionId, "user", userMessage);
  remember(sessionId, "assistant", output);
  return output;
};

const agent = new EnkiJsAgent(
  "Memory Example Agent",
  "Use the provided notes when answering.",
  "js::memory-demo",
  1,
  llmHandler,
  null,
  []
);
```

This is application-level memory, not a first-class memory module. It is the correct pattern until the JS binding exposes a dedicated memory API.

## Build from source

From `crates/bindings/enki-js`:

```bash
wasm-pack build --target bundler --out-dir pkg
```

If you need the `web` target instead:

```bash
wasm-pack build --target web --out-dir pkg-web
```

## JavaScript docs

- [WASM Usage](/docs/wasm-usage)
