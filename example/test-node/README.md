# test-node

A small Next.js app wired to the local `@getenki/ai` package from this repository.

## Getting Started

Install dependencies and run the development server:

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) with your browser to see the result.

## Custom Agentic Loops

This app depends on the local JavaScript bindings:

```json
"@getenki/ai": "file:../../crates/bindings/enki-js"
```

That means you can use the same custom loop APIs shown in the checked-in Node examples:

- prompt-level customization with the `agenticLoop` constructor argument
- reusable tool registries with `NativeToolRegistry` and `agent.connectToolRegistry(...)`
- full JavaScript loop overrides with `agent.setAgentLoopHandler(...)`

Reference examples in this repository:

- [`example/basic-js/tool-registry.js`](/I:/projects/enki/core-next/example/basic-js/tool-registry.js)
- [`example/basic-js/custom-agent-loop.js`](/I:/projects/enki/core-next/example/basic-js/custom-agent-loop.js)
- [`example/basic-js/react-custom-agent-loop.js`](/I:/projects/enki/core-next/example/basic-js/react-custom-agent-loop.js)

If you update the native bindings, rebuild them from [`crates/bindings/enki-js`](/I:/projects/enki/core-next/crates/bindings/enki-js) before testing the app:

```bash
npm install
npm run build
```

## Useful Commands

```bash
npm run dev
npm run build
npm run lint
```

## Learn More

- [Next.js Documentation](https://nextjs.org/docs)
- [Enki JavaScript package README](/I:/projects/enki/core-next/crates/bindings/enki-js/README.md)
- [Repository README](/I:/projects/enki/core-next/README.md)
