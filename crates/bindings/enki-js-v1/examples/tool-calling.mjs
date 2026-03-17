import init, { EnkiJsAgent } from "../pkg/enki_js.js";

await init();

const tools = [
    {
        name: "echo",
        description: "Echo a string back to the agent",
        parameters_json: JSON.stringify({
            type: "object",
            properties: {
                value: { type: "string" }
            },
            required: ["value"]
        })
    },
    {
        name: "add_numbers",
        description: "Add two numbers together",
        parameters_json: JSON.stringify({
            type: "object",
            properties: {
                left: { type: "number" },
                right: { type: "number" }
            },
            required: ["left", "right"]
        })
    }
];

const llmHandler = async ({ messages }) => {
    const last = messages[messages.length - 1];

    if (last.role === "user") {
        const text = last.content.trim();
        const addMatch = /^add\s+(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)$/i.exec(text);

        if (text.toLowerCase().startsWith("echo ")) {
            return {
                content: "Calling the echo tool.",
                tool_calls: [
                    {
                        id: "call-echo",
                        function: {
                            name: "echo",
                            arguments: { value: text.slice(5) }
                        }
                    }
                ]
            };
        }

        if (addMatch) {
            return {
                content: "Calling the add_numbers tool.",
                tool_calls: [
                    {
                        id: "call-add",
                        function: {
                            name: "add_numbers",
                            arguments: {
                                left: Number(addMatch[1]),
                                right: Number(addMatch[2])
                            }
                        }
                    }
                ]
            };
        }

        return "Try `echo hello` or `add 2 3`.";
    }

    if (last.role === "tool") {
        return `Tool result: ${last.content}`;
    }

    return "No action taken.";
};

const toolHandler = async ({ tool, args }) => {
    if (tool === "echo") {
        return String(args.value ?? "");
    }

    if (tool === "add_numbers") {
        return String(Number(args.left ?? 0) + Number(args.right ?? 0));
    }

    return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
    "Tool Calling Example",
    "Use tools when the user asks for them.",
    "js::tool-demo",
    4,
    llmHandler,
    toolHandler,
    tools
);

const sessionId = "tool-calling-demo";

for (const prompt of ["echo hello from enki-js", "add 7 35", "what can you do?"]) {
    const result = await agent.run(sessionId, prompt);
    console.log(`prompt: ${prompt}`);
    console.log(`result: ${result}`);
    console.log("---");
}
