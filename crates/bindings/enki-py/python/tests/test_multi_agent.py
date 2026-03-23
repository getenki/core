import json
import os
import urllib.request

import enki_py.agent as agent_module
import pytest


class FakeEnkiAgent:
    last_kwargs = None

    def __init__(self, handler=None, llm_handler=None):
        self.handler = handler
        self.llm_handler = llm_handler

    @classmethod
    def with_tools_and_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(kwargs["handler"], kwargs["llm_handler"])

    @classmethod
    def with_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(llm_handler=kwargs["llm_handler"])

    async def run(self, session_id: str, user_message: str) -> str:
        tools = [
            {
                "name": tool.name,
                "description": tool.description,
                "parameters_json": tool.parameters_json,
            }
            for tool in FakeEnkiAgent.last_kwargs.get("tools", [])
        ]
        first = self.llm_handler.complete(
            FakeEnkiAgent.last_kwargs["model"],
            json.dumps([{"role": "user", "content": user_message}]),
            json.dumps(tools),
        )
        payload = json.loads(first) if first.startswith("{") else first
        if isinstance(payload, dict) and payload.get("tool_calls"):
            tool_call = payload["tool_calls"][0]
            arguments = tool_call["function"]["arguments"]
            tool_result = self.handler.execute(
                tool_call["function"]["name"],
                json.dumps(arguments),
                "",
                "",
                "",
            )
            second = self.llm_handler.complete(
                FakeEnkiAgent.last_kwargs["model"],
                json.dumps(
                    [
                        {"role": "user", "content": user_message},
                        {"role": "tool", "content": tool_result},
                    ]
                ),
                json.dumps(tools),
            )
            second_payload = json.loads(second) if second.startswith("{") else second
            if isinstance(second_payload, dict):
                return second_payload["content"]
            return second_payload

        if isinstance(payload, dict):
            return payload["content"]
        return payload


class RecordingProvider(agent_module.LlmProviderBackend):
    def __init__(self, responses):
        self._responses = list(responses)
        self.calls = []

    def complete(self, model: str, messages, tools):
        self.calls.append(
            {
                "model": model,
                "messages": messages,
                "tools": tools,
            }
        )
        if not self._responses:
            raise AssertionError("missing response")
        return self._responses.pop(0)


class OllamaProvider(agent_module.LlmProviderBackend):
    def __init__(self, base_url: str | None = None) -> None:
        self.base_url = (
            base_url or os.getenv("OLLAMA_URL") or "http://127.0.0.1:11434"
        ).rstrip("/")

    def complete(self, model: str, messages, tools):
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


def test_multi_agent_runtime_discovers_members():
    coordinator = agent_module.Agent("coordinator-model", name="Coordinator")
    researcher = agent_module.Agent("researcher-model", name="Researcher")

    runtime = agent_module.MultiAgentRuntime(
        [
            agent_module.MultiAgentMember(
                agent_id="coordinator",
                agent=coordinator,
                capabilities=["planning", "orchestration"],
            ),
            agent_module.MultiAgentMember(
                agent_id="researcher",
                agent=researcher,
                capabilities=["research"],
                status="busy",
            ),
        ]
    )

    all_cards = runtime.registry()
    assert [card.agent_id for card in all_cards] == ["coordinator", "researcher"]

    research_cards = runtime.discover(capability="research")
    assert [card.agent_id for card in research_cards] == ["researcher"]

    busy_cards = runtime.discover(status="busy")
    assert [card.agent_id for card in busy_cards] == ["researcher"]


def test_multi_agent_runtime_delegates_between_python_agents(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)

    coordinator_provider = RecordingProvider(
        [
            {
                "content": "",
                "tool_calls": [
                    {
                        "id": "call-1",
                        "function": {
                            "name": "delegate_task",
                            "arguments": {
                                "agent_id": "researcher",
                                "task": "Summarize the docs",
                            },
                        },
                    }
                ],
            },
            {
                "content": "Coordinator saw: delegated answer",
            },
        ]
    )
    researcher_provider = RecordingProvider(
        [
            {
                "content": "delegated answer",
            }
        ]
    )

    coordinator = agent_module.Agent(
        "coordinator-model",
        name="Coordinator",
        instructions=(
            "Delegate research work to the researcher agent with delegate_task."
        ),
        llm=coordinator_provider,
    )
    researcher = agent_module.Agent(
        "researcher-model",
        name="Researcher",
        instructions="Answer research questions directly.",
        llm=researcher_provider,
    )

    runtime = agent_module.MultiAgentRuntime(
        [
            agent_module.MultiAgentMember(
                agent_id="coordinator",
                agent=coordinator,
                capabilities=["orchestration"],
            ),
            agent_module.MultiAgentMember(
                agent_id="researcher",
                agent=researcher,
                capabilities=["research"],
            ),
        ]
    )

    result = runtime.process_sync(
        "coordinator",
        "Find the answer by delegating to the researcher.",
        session_id="session-1",
    )

    assert result.output == "Coordinator saw: delegated answer"
    assert any(
        tool["name"] == "delegate_task"
        for tool in coordinator_provider.calls[0]["tools"]
    )
    assert researcher_provider.calls[0]["messages"][-1]["content"] == "Summarize the docs"


def test_multi_agent_runtime_unknown_agent_raises():
    runtime = agent_module.MultiAgentRuntime(
        [
            agent_module.MultiAgentMember(
                agent_id="only",
                agent=agent_module.Agent("only-model", name="Only"),
                capabilities=[],
            )
        ]
    )

    try:
        runtime.process_sync("missing", "hello")
    except ValueError as error:
        assert "not found" in str(error)
    else:  # pragma: no cover
        raise AssertionError("expected ValueError")


@pytest.mark.skipif(
    os.getenv("ENKI_RUN_OLLAMA_TESTS") != "1",
    reason="Set ENKI_RUN_OLLAMA_TESTS=1 to run Ollama integration tests.",
)
@pytest.mark.skipif(
    not hasattr(agent_module._LowLevelEnkiAgent, "with_llm"),
    reason="Native enki-py extension is required for Ollama integration tests.",
)
def test_multi_agent_runtime_with_ollama():
    model = os.getenv("OLLAMA_MODEL", "qwen3.5:latest")
    provider = OllamaProvider()

    coordinator = agent_module.Agent(
        model,
        name="Coordinator",
        instructions=(
            "You are a coordinator. Use delegate_task when the user explicitly asks "
            "you to delegate to the researcher. Return the delegated answer verbatim."
        ),
        llm=provider,
    )
    researcher = agent_module.Agent(
        model,
        name="Researcher",
        instructions=(
            "You are a research agent. If the task asks for an exact response, "
            "return exactly that response and nothing else."
        ),
        llm=provider,
    )

    runtime = agent_module.MultiAgentRuntime(
        [
            agent_module.MultiAgentMember(
                agent_id="coordinator",
                agent=coordinator,
                capabilities=["orchestration"],
            ),
            agent_module.MultiAgentMember(
                agent_id="researcher",
                agent=researcher,
                capabilities=["research"],
            ),
        ]
    )

    result = runtime.process_sync(
        "coordinator",
        (
            "Delegate to the researcher using delegate_task. "
            "Ask it to reply exactly with RESEARCHER_OK. "
            "Return only the delegated response."
        ),
        session_id="ollama-multi-agent",
    )

    assert "RESEARCHER_OK" in result.output
