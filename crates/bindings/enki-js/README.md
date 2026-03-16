# enki-js

Browser-oriented JavaScript bindings for Enki built with `wasm-bindgen`.

## What it exposes

- `EnkiJsTool`: tool metadata registered from JavaScript
- `EnkiJsAgent`: in-memory agent loop backed by JavaScript callbacks for LLM requests and tool execution

The current WASM binding is intentionally browser-safe:

- session state is kept in memory
- filesystem-backed persistence is not used
- tools execute through a JavaScript callback instead of native process execution

## Build

```powershell
wasm-pack build --target bundler --out-dir pkg
```

## Example

Build the package first, then run [examples/basic.mjs](/I:/projects/enki/core-next/crates/bindings/enki-js/examples/basic.mjs) in a JS environment that supports ESM:

```javascript
import init, { EnkiJsAgent } from "../pkg/enki_js.js";

await init();

const agent = new EnkiJsAgent(
  "Example Agent",
  "Use the echo tool before answering.",
  4,
  async ({ messages }) => {
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

    return "No action taken.";
  },
  async ({ tool, args }) => {
    if (tool === "echo") {
      return `echo:${args.value}`;
    }

    return `Unknown tool: ${tool}`;
  },
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

## LLM callback contract

The constructor accepts an async JavaScript function that receives:

```json
{
  "agent": {
    "name": "Personal Assistant",
    "system_prompt_preamble": "...",
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
