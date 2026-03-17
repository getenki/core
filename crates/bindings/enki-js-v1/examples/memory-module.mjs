import init, { EnkiJsAgent } from "../pkg/enki_js.js";

await init();

class InMemoryNotes {
    constructor() {
        this.sessions = new Map();
    }

    remember(sessionId, note) {
        const notes = this.sessions.get(sessionId) ?? [];
        notes.push(note);
        this.sessions.set(sessionId, notes);
        return `Saved note: ${note}`;
    }

    search(sessionId, query) {
        const notes = this.sessions.get(sessionId) ?? [];
        const matches = notes.filter((note) =>
            note.toLowerCase().includes(query.toLowerCase())
        );

        if (matches.length === 0) {
            return `No notes matched "${query}".`;
        }

        return matches.map((note, index) => `${index + 1}. ${note}`).join("\n");
    }

    clear(sessionId) {
        this.sessions.set(sessionId, []);
        return "Cleared memory for this session.";
    }
}

const memoryModule = new InMemoryNotes();
let activeSessionId = null;

const tools = [
    {
        name: "remember_note",
        description: "Store a note in the host application's memory module",
        parameters_json: JSON.stringify({
            type: "object",
            properties: {
                note: { type: "string" }
            },
            required: ["note"]
        })
    },
    {
        name: "search_memory",
        description: "Search stored notes in the host application's memory module",
        parameters_json: JSON.stringify({
            type: "object",
            properties: {
                query: { type: "string" }
            },
            required: ["query"]
        })
    },
    {
        name: "clear_memory",
        description: "Clear all notes for the current session",
        parameters_json: JSON.stringify({
            type: "object",
            properties: {}
        })
    }
];

const llmHandler = async ({ messages }) => {
    const last = messages[messages.length - 1];

    if (last.role === "user") {
        const text = last.content.trim();

        if (text.toLowerCase().startsWith("remember ")) {
            return {
                content: "Saving that note to memory.",
                tool_calls: [
                    {
                        id: "remember-call",
                        function: {
                            name: "remember_note",
                            arguments: { note: text.slice(9) }
                        }
                    }
                ]
            };
        }

        if (text.toLowerCase().startsWith("recall ")) {
            return {
                content: "Searching memory.",
                tool_calls: [
                    {
                        id: "search-call",
                        function: {
                            name: "search_memory",
                            arguments: { query: text.slice(7) }
                        }
                    }
                ]
            };
        }

        if (text.toLowerCase() === "forget everything") {
            return {
                content: "Clearing memory.",
                tool_calls: [
                    {
                        id: "clear-call",
                        function: {
                            name: "clear_memory",
                            arguments: {}
                        }
                    }
                ]
            };
        }

        return "Try `remember ...`, `recall ...`, or `forget everything`.";
    }

    if (last.role === "tool") {
        return last.content;
    }

    return "No action taken.";
};

const toolHandler = async ({ tool, args }) => {
    if (!activeSessionId) {
        throw new Error("No active session id is set.");
    }

    if (tool === "remember_note") {
        return memoryModule.remember(activeSessionId, String(args.note ?? ""));
    }

    if (tool === "search_memory") {
        return memoryModule.search(activeSessionId, String(args.query ?? ""));
    }

    if (tool === "clear_memory") {
        return memoryModule.clear(activeSessionId);
    }

    return `Unknown tool: ${tool}`;
};

const agent = new EnkiJsAgent(
    "Memory Module Example",
    "Use memory tools when the user wants to save or recall information.",
    "js::memory-demo",
    4,
    llmHandler,
    toolHandler,
    tools
);

const runWithSession = async (sessionId, prompt) => {
    activeSessionId = sessionId;
    const result = await agent.run(sessionId, prompt);
    console.log(`prompt: ${prompt}`);
    console.log(`result: ${result}`);
    console.log("---");
    return result;
};

const sessionId = "memory-module-demo";

await runWithSession(sessionId, "remember Colombo is in Sri Lanka");
await runWithSession(sessionId, "remember Enki JS sessions are stored in memory");
await runWithSession(sessionId, "recall sri");
await runWithSession(sessionId, "forget everything");
await runWithSession(sessionId, "recall sri");
