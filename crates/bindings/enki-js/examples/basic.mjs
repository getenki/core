import init, { EnkiJsAgent } from "../pkg/enki_js.js";

await init();

const tools = [
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
];

const llmHandler = async ({ messages }) => {
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
};

const toolHandler = async ({ tool, args }) => {
  if (tool === "echo") {
    return `echo:${args.value}`;
  }

  return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
  "Example Agent",
  "Use the echo tool before answering.",
  4,
  llmHandler,
  toolHandler,
  tools
);

const result = await agent.run("demo-session", "hello from javascript");
console.log(result);
