import { useEffect, useRef, useState } from "react";
import "./App.css";
import { EnkiJsAgent } from "enki-js";

const model = "google::gemini-3.1-pro-preview";

const toGeminiModel = (value) =>
  value.startsWith("google::") ? value.slice("google::".length) : value;

const toGeminiContent = (message) => ({
  role: message.role === "assistant" ? "model" : "user",
  parts: [{ text: message.content }]
});

const llmHandler = async ({ agent, messages }) => {
  const apiKey = process.env.REACT_APP_GOOGLE_API_KEY;

  if (!apiKey) {
    throw new Error("Set REACT_APP_GOOGLE_API_KEY before running this example.");
  }

  const systemText = messages
    .filter((message) => message.role === "system")
    .map((message) => message.content)
    .join("\n\n")
    .trim();

  const contents = messages
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

function App() {
  const agentRef = useRef(null);
  const [prompt, setPrompt] = useState("Hello!");
  const [status, setStatus] = useState("Initializing...");
  const [result, setResult] = useState("No response yet.");
  const [isRunning, setIsRunning] = useState(false);

  useEffect(() => {
    let disposed = false;

    async function boot() {
      try {
        const agent = new EnkiJsAgent(
          "Browser Test Agent",
          "Answer clearly and concisely.",
          model,
          1,
          llmHandler,
          null,
          []
        );

        agentRef.current = agent;
        await agent.ready();

        if (!disposed) {
          setStatus("Ready");
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
  }, []);

  async function runAgent() {
    if (!agentRef.current || isRunning) {
      return;
    }

    setIsRunning(true);
    setStatus("Running...");
    setResult("Waiting for model response...");

    try {
      const output = await agentRef.current.run(
        `browser-test-${Date.now()}`,
        prompt
      );
      setResult(output);
      setStatus("Completed");
    } catch (error) {
      setResult(String(error));
      setStatus("Failed");
    } finally {
      setIsRunning(false);
    }
  }

  return (
    <div className="App">
      <header className="App-header">
        <h1>enki-js basic example</h1>
        <p>Status: {status}</p>
        <textarea
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          rows={6}
          cols={60}
        />
        <button onClick={runAgent} disabled={isRunning || !agentRef.current}>
          {isRunning ? "Running..." : "Run agent"}
        </button>
        <pre>{result}</pre>
      </header>
    </div>
  );
}

export default App;
