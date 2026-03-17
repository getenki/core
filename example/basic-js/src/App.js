import { useEffect, useRef, useState } from "react";
import "./App.css";
import { EnkiJsAgent } from "enki-js";

const geminiModel = "google::gemini-3.1-pro-preview";

const toolDefinitions = [
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

const memoryTools = [
  {
    name: "remember_note",
    description: "Store a note in the browser memory module",
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
    description: "Search stored notes in the browser memory module",
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

const allAgentTools = [...toolDefinitions, ...memoryTools];

const modes = {
  agent: {
    label: "Agent",
    description: "Gemini plus Enki tool execution and a separate browser memory module.",
    initialPrompt: "Remember that Colombo is in Sri Lanka, then tell me what you stored.",
    sessionId: "browser-agent-demo"
  },
  tools: {
    label: "Tools",
    description: "Gemini plus calculation and utility tools, without memory tools.",
    initialPrompt: "Add 7 and 35, then explain the result.",
    sessionId: "browser-tools-demo"
  }
};

function createMemoryModule() {
  return {
    sessions: new Map(),
    remember(sessionId, note) {
      if (!note.trim()) {
        return "Refused to save an empty note.";
      }

      const notes = this.sessions.get(sessionId) ?? [];
      notes.push(note);
      this.sessions.set(sessionId, notes);
      return `Saved note: ${note}`;
    },
    search(sessionId, query) {
      const notes = this.sessions.get(sessionId) ?? [];
      const matches = notes.filter((note) =>
        note.toLowerCase().includes(query.toLowerCase())
      );

      if (matches.length === 0) {
        return `No notes matched "${query}".`;
      }

      return matches.map((note, index) => `${index + 1}. ${note}`).join("\n");
    },
    clear(sessionId) {
      this.sessions.set(sessionId, []);
      return "Cleared memory for this session.";
    },
    list(sessionId) {
      return this.sessions.get(sessionId) ?? [];
    }
  };
}

function normalizeCallbackArgs(args) {
  if (typeof args === "string") {
    try {
      return normalizeCallbackArgs(JSON.parse(args));
    } catch {
      return args;
    }
  }

  if (args && typeof args === "object" && !Array.isArray(args)) {
    if (args.arguments && typeof args.arguments === "object") {
      return normalizeCallbackArgs(args.arguments);
    }
    return args;
  }

  return args ?? {};
}

function readNamedArg(args, key) {
  const normalized = normalizeCallbackArgs(args);

  if (normalized && typeof normalized === "object" && key in normalized) {
    return normalized[key];
  }

  return "";
}

function createPendingToolState() {
  return {
    values: new Map(),
    set(tool, payload) {
      this.values.set(tool, payload);
    },
    take(tool) {
      const payload = this.values.get(tool);
      this.values.delete(tool);
      return payload ?? {};
    },
    clear() {
      this.values.clear();
    }
  };
}

function createToolEventStore() {
  return {
    items: [],
    push(entry) {
      this.items = [...this.items, entry].slice(-12);
    },
    list() {
      return this.items;
    },
    clear() {
      this.items = [];
    }
  };
}

function inferIntentToolCall(text) {
  const trimmed = text.trim();
  const lower = trimmed.toLowerCase();
  const addMatch = /^add\s+(-?\d+(?:\.\d+)?)\s+(?:and\s+)?(-?\d+(?:\.\d+)?)$/i.exec(trimmed);

  if (lower.startsWith("echo ")) {
    return {
      content: "Calling the echo tool.",
      tool: "echo",
      args: { value: trimmed.slice(5) }
    };
  }

  if (addMatch) {
    return {
      content: "Calling the add_numbers tool.",
      tool: "add_numbers",
      args: {
        left: Number(addMatch[1]),
        right: Number(addMatch[2])
      }
    };
  }

  if (lower.startsWith("remember ")) {
    return {
      content: "Saving that note to memory.",
      tool: "remember_note",
      args: { note: trimmed.slice(9) }
    };
  }

  if (lower.startsWith("recall ")) {
    return {
      content: "Searching memory.",
      tool: "search_memory",
      args: { query: trimmed.slice(7) }
    };
  }

  if (lower === "forget everything" || lower === "clear memory") {
    return {
      content: "Clearing memory.",
      tool: "clear_memory",
      args: {}
    };
  }

  if (lower.includes("remember that ")) {
    const start = lower.indexOf("remember that ");
    const raw = trimmed.slice(start + "remember that ".length);
    const note = raw.split(/,| then | and then /i)[0].trim();

    if (note) {
      return {
        content: "Saving that note to memory before answering.",
        tool: "remember_note",
        args: { note }
      };
    }
  }

  if (lower.includes("what do you remember")) {
    return {
      content: "Searching memory for relevant notes.",
      tool: "search_memory",
      args: { query: "remember" }
    };
  }

  if (lower.includes("what did you store")) {
    return {
      content: "Searching memory for stored notes.",
      tool: "search_memory",
      args: { query: "" }
    };
  }

  return null;
}

function buildToolPrompt(tools) {
  return [
    "You can call tools when needed.",
    "When you need a tool, respond with JSON only.",
    'Use this exact shape: {"tool":"tool_name","args":{...}}',
    "Do not wrap JSON in markdown fences.",
    "If no tool is needed, answer normally.",
    `Available tools: ${tools
      .map((tool) => `${tool.name}: ${tool.description}`)
      .join(" | ")}`
  ].join("\n");
}

function withToolInstructions(messages, tools) {
  const instruction = buildToolPrompt(tools);
  const cloned = messages.map((message) => ({ ...message }));
  const systemIndex = cloned.findIndex((message) => message.role === "system");

  if (systemIndex >= 0) {
    cloned[systemIndex] = {
      ...cloned[systemIndex],
      content: `${cloned[systemIndex].content}\n\n${instruction}`
    };
    return cloned;
  }

  return [{ role: "system", content: instruction }, ...cloned];
}

const toGeminiModel = (value) =>
  value.startsWith("google::") ? value.slice("google::".length) : value;

const toGeminiContent = (message) => ({
  role: message.role === "assistant" ? "model" : "user",
  parts: [{ text: message.content }]
});

function createGeminiHandler(toolAwareTools = []) {
  return async ({ agent, messages }) => {
    const apiKey = process.env.REACT_APP_GOOGLE_API_KEY;

    if (!apiKey) {
      throw new Error("Set REACT_APP_GOOGLE_API_KEY before running this example.");
    }

    const preparedMessages =
      toolAwareTools.length > 0
        ? withToolInstructions(messages, toolAwareTools)
        : messages;

    const systemText = preparedMessages
      .filter((message) => message.role === "system")
      .map((message) => message.content)
      .join("\n\n")
      .trim();

    const contents = preparedMessages
      .filter((message) => message.role !== "system")
      .map(toGeminiContent);

    const response = await fetch(
      `https://generativelanguage.googleapis.com/v1beta/models/${toGeminiModel(
        agent.model
      )}:generateContent`,
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
}

function createToolOnlyHandlers(pendingTools, toolEventStore) {
  const geminiHandler = createGeminiHandler(toolDefinitions);

  const llmHandler = async ({ agent, messages, tools }) => {
    const last = messages[messages.length - 1];

    if (last?.role === "user") {
      const intent = inferIntentToolCall(last.content);

      if (intent && (intent.tool === "echo" || intent.tool === "add_numbers")) {
        pendingTools.set(intent.tool, intent.args);
        return {
          content: intent.content,
          tool_calls: [
            {
              id: `call-${intent.tool}`,
              function: {
                name: intent.tool,
                arguments: intent.args
              }
            }
          ]
        };
      }
    }

    return geminiHandler({ agent, messages, tools });
  };

  const toolHandler = async ({ tool, args }) => {
    const normalizedArgs = normalizeCallbackArgs(args);
    const fallbackArgs = pendingTools.take(tool);
    const mergedArgs =
      normalizedArgs && typeof normalizedArgs === "object"
        ? { ...fallbackArgs, ...normalizedArgs }
        : fallbackArgs;

    if (tool === "echo") {
      const value = String(readNamedArg(mergedArgs, "value"));
      toolEventStore.push({ kind: "tool", tool, detail: value || "(empty)" });
      return value;
    }

    if (tool === "add_numbers") {
      const left = Number(readNamedArg(mergedArgs, "left") ?? 0);
      const right = Number(readNamedArg(mergedArgs, "right") ?? 0);
      const total = String(
        left + right
      );
      toolEventStore.push({ kind: "tool", tool, detail: `${left} + ${right} = ${total}` });
      return total;
    }

    return `Unknown tool: ${tool}`;
  };

  return { llmHandler, toolHandler, tools: toolDefinitions, model: geminiModel };
}

function createCombinedAgentHandlers(
  memoryModule,
  activeSessionRef,
  pendingTools,
  toolEventStore
) {
  const geminiHandler = createGeminiHandler(allAgentTools);

  const llmHandler = async ({ agent, messages, tools }) => {
    const last = messages[messages.length - 1];

    if (last?.role === "user") {
      const intent = inferIntentToolCall(last.content);

      if (intent) {
        pendingTools.set(intent.tool, intent.args);
        return {
          content: intent.content,
          tool_calls: [
            {
              id: `call-${intent.tool}`,
              function: {
                name: intent.tool,
                arguments: intent.args
              }
            }
          ]
        };
      }
    }

    return geminiHandler({ agent, messages, tools });
  };

  const toolHandler = async ({ tool, args }) => {
    const sessionId = activeSessionRef.current;
    const normalizedArgs = normalizeCallbackArgs(args);
    const fallbackArgs = pendingTools.take(tool);
    const mergedArgs =
      normalizedArgs && typeof normalizedArgs === "object"
        ? { ...fallbackArgs, ...normalizedArgs }
        : fallbackArgs;

    if (!sessionId) {
      throw new Error("No active session id is set.");
    }

    if (tool === "echo") {
      const value = String(readNamedArg(mergedArgs, "value"));
      toolEventStore.push({ kind: "tool", tool, detail: value || "(empty)" });
      return value;
    }

    if (tool === "add_numbers") {
      const left = Number(readNamedArg(mergedArgs, "left") ?? 0);
      const right = Number(readNamedArg(mergedArgs, "right") ?? 0);
      const total = String(left + right);
      toolEventStore.push({ kind: "tool", tool, detail: `${left} + ${right} = ${total}` });
      return total;
    }

    if (tool === "remember_note") {
      const note = String(readNamedArg(mergedArgs, "note"));
      const output = memoryModule.remember(
        sessionId,
        note
      );
      toolEventStore.push({ kind: "memory", tool, detail: note || "(empty)" });
      return output;
    }

    if (tool === "search_memory") {
      const query = String(readNamedArg(mergedArgs, "query"));
      const output = memoryModule.search(
        sessionId,
        query
      );
      toolEventStore.push({ kind: "memory", tool, detail: `query: ${query || "(empty)"}` });
      return output;
    }

    if (tool === "clear_memory") {
      toolEventStore.push({ kind: "memory", tool, detail: "clear current session" });
      return memoryModule.clear(sessionId);
    }

    return `Unknown tool: ${tool}`;
  };

  return {
    llmHandler,
    toolHandler,
    tools: allAgentTools,
    model: geminiModel
  };
}

function App() {
  const agentRef = useRef(null);
  const activeSessionRef = useRef(modes.agent.sessionId);
  const memoryModuleRef = useRef(createMemoryModule());
  const pendingToolsRef = useRef(createPendingToolState());
  const toolEventsRef = useRef(createToolEventStore());
  const [mode, setMode] = useState("agent");
  const [prompt, setPrompt] = useState(modes.agent.initialPrompt);
  const [status, setStatus] = useState("Initializing...");
  const [result, setResult] = useState("No response yet.");
  const [isRunning, setIsRunning] = useState(false);
  const [toolCatalog, setToolCatalog] = useState("{}");
  const [memoryNotes, setMemoryNotes] = useState([]);
  const [toolEvents, setToolEvents] = useState([]);

  useEffect(() => {
    let disposed = false;

    async function boot() {
      setStatus(`Initializing ${modes[mode].label} agent...`);
      setResult("No response yet.");
      setToolCatalog("{}");
      pendingToolsRef.current.clear();
      toolEventsRef.current.clear();
      setToolEvents([]);

      try {
        if (agentRef.current) {
          agentRef.current.free();
          agentRef.current = null;
        }

        const config =
          mode === "agent"
            ? createCombinedAgentHandlers(
                memoryModuleRef.current,
                activeSessionRef,
                pendingToolsRef.current,
                toolEventsRef.current
              )
            : createToolOnlyHandlers(
                pendingToolsRef.current,
                toolEventsRef.current
              );

        const agent = new EnkiJsAgent(
          `${modes[mode].label} Example Agent`,
          modes[mode].description,
          config.model,
          4,
          config.llmHandler,
          config.toolHandler,
          config.tools
        );

        agentRef.current = agent;
        await agent.ready();

        if (!disposed) {
          setToolCatalog(agent.toolCatalogJson());
          setStatus(`Ready: ${modes[mode].label}`);
        }
      } catch (error) {
        if (!disposed) {
          setStatus(`Init failed: ${String(error)}`);
        }
      }
    }

    boot();

    return () => {
      disposed = true;
      if (agentRef.current) {
        agentRef.current.free();
        agentRef.current = null;
      }
    };
  }, [mode]);

  useEffect(() => {
    activeSessionRef.current = modes[mode].sessionId;
    setPrompt(modes[mode].initialPrompt);
    setMemoryNotes(memoryModuleRef.current.list(modes.agent.sessionId));
    setToolEvents(toolEventsRef.current.list());
  }, [mode]);

  async function runAgent() {
    if (!agentRef.current || isRunning) {
      return;
    }

    const sessionId = modes[mode].sessionId;
    activeSessionRef.current = sessionId;
    setIsRunning(true);
    setStatus(`Running ${modes[mode].label} flow...`);
    setResult("Waiting for agent response...");

    try {
      const output = await agentRef.current.run(sessionId, prompt);
      setResult(output);
      setStatus(`Completed: ${modes[mode].label}`);
      setToolEvents(toolEventsRef.current.list());
      if (mode === "agent") {
        setMemoryNotes(memoryModuleRef.current.list(sessionId));
      } else {
        setMemoryNotes([]);
      }
    } catch (error) {
      setResult(String(error));
      setStatus("Failed");
    } finally {
      setIsRunning(false);
    }
  }

  function applyPreset(nextPrompt) {
    setPrompt(nextPrompt);
  }

  const presetPrompts =
    mode === "agent"
      ? [
          "Remember that Colombo is in Sri Lanka, then tell me what you stored.",
          "Add 7 and 35, then remember the total.",
          "What do you remember about Sri Lanka?"
        ]
      : mode === "tools"
        ? [
            "Add 7 and 35, then explain the result.",
            "Echo hello from enki-js and then summarize what you did.",
            "What tools are available for calculations?"
          ]
        : ["Remember that Colombo is in Sri Lanka, then tell me what you stored."];

  return (
    <div className="app-shell">
      <main className="panel">
        <section className="hero">
          <p className="eyebrow">enki-js React example</p>
          <h1>Browser agent, tool calls, and host memory in one UI.</h1>
          <p className="hero-copy">
            Run a full Gemini-backed Enki agent with tool execution and a separate
            browser memory module, or use a Gemini-backed tool workflow without memory.
          </p>
        </section>

        <section className="mode-grid">
          {Object.entries(modes).map(([key, config]) => (
            <button
              key={key}
              className={key === mode ? "mode-card active" : "mode-card"}
              onClick={() => setMode(key)}
              type="button"
            >
              <span className="mode-title">{config.label}</span>
              <span className="mode-copy">{config.description}</span>
            </button>
          ))}
        </section>

        <section className="workspace">
          <div className="composer">
            <div className="section-heading">
              <h2>Prompt</h2>
              <span className="status-pill">{status}</span>
            </div>
            <p className="hint">
              Session: <code>{modes[mode].sessionId}</code>
            </p>
            <textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              rows={7}
            />
            <div className="preset-row">
              {presetPrompts.map((preset) => (
                <button
                  key={preset}
                  className="ghost-button"
                  onClick={() => applyPreset(preset)}
                  type="button"
                >
                  {preset}
                </button>
              ))}
            </div>
            <button
              className="primary-button"
              onClick={runAgent}
              disabled={isRunning || !agentRef.current}
              type="button"
            >
              {isRunning ? "Running..." : "Run agent"}
            </button>
          </div>

          <div className="side-panel">
            <div className="card">
              <div className="section-heading">
                <h2>Result</h2>
              </div>
              <pre>{result}</pre>
            </div>

            <div className="card">
              <div className="section-heading">
                <h2>Tool catalog</h2>
              </div>
              <pre>{toolCatalog}</pre>
            </div>

            <div className="card">
              <div className="section-heading">
                <h2>Tool activity</h2>
              </div>
              {toolEvents.length === 0 ? (
                <p className="empty-state">No tool calls recorded yet.</p>
              ) : (
                <ul className="memory-list">
                  {toolEvents.map((entry, index) => (
                    <li key={`${index}-${entry.kind}-${entry.tool}`}>
                      <strong>{entry.kind}</strong>: {entry.tool} - {entry.detail}
                    </li>
                  ))}
                </ul>
              )}
            </div>

            <div className="card">
              <div className="section-heading">
                <h2>Memory notes</h2>
              </div>
              {memoryNotes.length === 0 ? (
                <p className="empty-state">
                  {mode === "agent"
                    ? "No saved notes for the agent session yet."
                    : "Memory is not enabled in Tools mode."}
                </p>
              ) : (
                <ul className="memory-list">
                  {memoryNotes.map((note, index) => (
                    <li key={`${index}-${note}`}>{note}</li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}

export default App;
