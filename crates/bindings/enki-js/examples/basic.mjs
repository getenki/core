import init, { EnkiJsAgent } from "../pkg/enki_js.js";

await init();

const apiKey =
    process.env.GOOGLE_AI_STUDIO_API_KEY ?? process.env.GEMINI_API_KEY ?? "AIzaSyAZEXZmE9THksdS5_8qtx2QhswgGiu6CxM";

if (!apiKey) {
    throw new Error(
        "Set GOOGLE_AI_STUDIO_API_KEY or GEMINI_API_KEY before running this example."
    );
}

const model = "google::gemini-3.1-pro-preview";

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

const agent = new EnkiJsAgent(
    "Simple Example Agent",
    "Answer clearly and concisely.",
    model,
    1,
    llmHandler,
    null,
    []
);

const sessionId = `example-${Date.now()}`;
const result = await agent.run(
    sessionId,
    "Explain in two sentences what EnkiJS agents are."
);

console.log(result);
