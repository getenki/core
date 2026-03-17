import json
import os
import urllib.request

from enki_py import Agent, LlmProviderBackend


class OllamaProvider(LlmProviderBackend):
    def __init__(self, base_url: str | None = None) -> None:
        self.base_url = (base_url or os.getenv("OLLAMA_URL") or "http://127.0.0.1:11434").rstrip("/")

    def complete(self, model: str, messages: list[dict], tools: list[dict]) -> dict:
        payload = {
            "model": model,
            "messages": [self._to_ollama_message(message) for message in messages],
            "stream": False,
        }

        if tools:
            payload["tools"] = tools

        request = urllib.request.Request(
            f"{self.base_url}/api/chat",
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )

        with urllib.request.urlopen(request) as response:
            body = json.loads(response.read().decode("utf-8"))

        message = body.get("message", {})
        return {
            "content": message.get("content", ""),
            "tool_calls": message.get("tool_calls", []),
        }

    @staticmethod
    def _to_ollama_message(message: dict) -> dict:
        role = str(message.get("role", "user")).lower()
        role = {
            "system": "system",
            "user": "user",
            "assistant": "assistant",
            "tool": "tool",
        }.get(role, role)

        normalized = {
            "role": role,
            "content": message.get("content", ""),
        }

        tool_call_id = message.get("tool_call_id")
        if tool_call_id:
            normalized["tool_call_id"] = tool_call_id

        return normalized


agent = Agent(
    "qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
    llm=OllamaProvider(),
)

result = agent.run_sync("Explain what this project does.")
print(result.output)
