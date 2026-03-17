# enki-js

Browser-oriented JavaScript bindings for Enki built with `wasm-bindgen`.

## What it exposes

- `EnkiJsTool`: tool metadata registered from JavaScript
- `EnkiJsAgent`: a `core_next::agent::Agent` running inside WASM with JavaScript callbacks for LLM requests and tool execution

The current WASM binding is intentionally browser-safe:

- the agent loop uses `crates/core` as the runtime
- session state is kept in memory by the core agent on `wasm32`
- filesystem-backed persistence is not used
- tools execute through a JavaScript callback instead of native process execution

## Build

```powershell
wasm-pack build --target web --out-dir pkg
```

## Examples

Build the package first, then run these files in a JS environment that supports ESM:

- `examples/basic.mjs`: minimal LLM-backed agent
- `examples/tool-calling.mjs`: tool registration and tool callback flow
- `examples/memory-module.mjs`: host-side JavaScript memory module pattern built on tools

The minimal example is:

```javascript
import init, { EnkiJsAgent } from "../pkg/enki_js.js";

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

## Create an agent step by step

1. Build the package so `pkg/enki_js.js` and the WASM artifact exist.
2. Import `init` and `EnkiJsAgent`, then call `await init()`.
3. Decide which model label your JavaScript callback should use, for example `google::gemini-3.1-pro-preview`.
4. Implement an async LLM callback that receives `{ agent, messages, tools }`, calls your provider, and returns either a string or `{ content, tool_calls }`.
5. Create `new EnkiJsAgent(name, systemPrompt, model, maxIterations, llmHandler, toolHandler, tools)`.
6. Call `await agent.run(sessionId, userMessage)` to execute or continue a conversation.

[`src/lib.rs`](/I:/projects/enki/core-next/crates/bindings/enki-js/src/lib.rs) is only the crate entrypoint that re-exports the `wasm` module on `wasm32`. The JavaScript-facing behavior is implemented in `src/wasm.rs`.

## Tool calling example

```javascript
import init, { EnkiJsAgent } from "../pkg/enki_js.js";

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
```

## Memory in JS

There is no dedicated JavaScript memory module API yet.

- Reusing the same `sessionId` keeps the conversation transcript in memory.
- If you need long-term or structured memory, keep it in JavaScript and inject recalled notes into prompts.
- Another workable pattern is exposing memory operations through tools such as `save_memory` and `search_memory`.

See `examples/tool-calling.mjs` for a runnable tool example and `examples/memory-module.mjs` for a host-managed memory pattern.

## LLM callback contract

The constructor accepts an async JavaScript function that receives:

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
    { "name": "echo", "description": "Echo a value" }
  ]
}
```

It must return either:

- a string final response, or
- an object shaped like `{ content, tool_calls? }`

`tool_calls` should contain objects shaped like:

```json
{
  "id": "call-1",
  "function": {
    "name": "echo",
    "arguments": { "value": "hello" }
  }
}
```
