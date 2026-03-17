# basic-js

This example shows how to create an `EnkiJsAgent` in a React app and call a JavaScript-managed LLM from the browser.

The relevant files are:

- [`src/App.js`](/I:/projects/enki/core-next/example/basic-js/src/App.js): React example that creates, runs, and frees the agent
- [`../../crates/bindings/enki-js/examples/basic.mjs`](/I:/projects/enki/core-next/crates/bindings/enki-js/examples/basic.mjs): minimal ESM example without React
- [`../../crates/bindings/enki-js/src/lib.rs`](/I:/projects/enki/core-next/crates/bindings/enki-js/src/lib.rs): Rust crate entrypoint for the WASM binding

## Run the example

1. Install dependencies:

```bash
npm install
```

2. Set your API key in `.env.local`:

```bash
REACT_APP_GOOGLE_API_KEY=your-key-here
```

3. Start the dev server:

```bash
npm start
```

## Agent creation flow

[`src/App.js`](/I:/projects/enki/core-next/example/basic-js/src/App.js) follows this sequence:

1. Import `EnkiJsAgent` from `enki-js`.
2. Define a model string such as `google::gemini-3.1-pro-preview`.
3. Convert Enki messages into the target provider payload format.
4. Implement `llmHandler` to call Google AI Studio with `fetch`.
5. Create `new EnkiJsAgent(...)` with no tools: `toolHandler = null`, `tools = []`.
6. Call `await agent.ready()` after construction.
7. Run prompts with `await agent.run(sessionId, prompt)`.
8. Call `agent.free()` when the component unmounts.

## Notes

- Session history is stored in memory for the lifetime of the agent instance.
- The browser host is responsible for authentication and network requests.
- [`lib.rs`](/I:/projects/enki/core-next/crates/bindings/enki-js/src/lib.rs) only re-exports the WASM module. The binding logic itself lives in `crates/bindings/enki-js/src/wasm.rs`.

## Tool calling pattern

To add tool calling in the browser example:

1. Pass a `toolHandler` function as the sixth `EnkiJsAgent` constructor argument.
2. Pass tool definitions as the seventh argument.
3. Make your `llmHandler` return `{ content, tool_calls }` when it wants the runtime to execute a tool.
4. Handle the tool name and arguments inside `toolHandler`, then return a string result.

Minimal shape:

```js
const toolHandler = async ({ tool, args }) => {
  if (tool === "echo") {
    return `echo:${args.value}`;
  }
  return `Unknown tool: ${tool}`;
};
```

## Memory pattern

There is no first-class JS memory module yet. For browser apps, keep memory in your app state, `localStorage`, IndexedDB, or your backend, then prepend recalled notes before calling `agent.run()`.
