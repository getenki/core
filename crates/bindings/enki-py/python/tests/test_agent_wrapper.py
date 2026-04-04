import json
import types

import enki_py.agent as agent_module


class FakeEnkiAgent:
    last_kwargs = None

    def __init__(self, handler=None, llm_handler=None):
        self.handler = handler
        self.llm_handler = llm_handler

    @classmethod
    def with_tools(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(kwargs["handler"])

    @classmethod
    def with_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(llm_handler=kwargs["llm_handler"])

    @classmethod
    def with_tools_and_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(kwargs["handler"], kwargs["llm_handler"])

    @classmethod
    def with_memory_and_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(llm_handler=kwargs["llm_handler"])

    @classmethod
    def with_tools_memory_and_llm(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(kwargs["tool_handler"], kwargs["llm_handler"])

    async def run(self, session_id: str, user_message: str) -> str:
        if self.llm_handler is not None:
            raw = self.llm_handler.complete(
                FakeEnkiAgent.last_kwargs["model"],
                json.dumps([{"role": "user", "content": user_message}]),
                json.dumps([]),
            )
            payload = json.loads(raw) if raw.startswith("{") else raw
            if isinstance(payload, dict):
                return payload["content"]
            return payload

        tool_names = {tool.name for tool in self.last_kwargs["tools"]}
        if {"get_player_name", "roll_dice"}.issubset(tool_names):
            guess = "".join(ch for ch in user_message if ch.isdigit())
            player_name = self.handler.execute("get_player_name", "{}", "", "", "")
            dice_roll = self.handler.execute("roll_dice", "{}", "", "", "")
            if dice_roll == guess:
                return f"Congratulations {player_name}, you guessed correctly! You're a winner!"
            return f"Sorry {player_name}, you guessed {guess} but rolled {dice_roll}."

        if {"get_player_name", "format_score"}.issubset(tool_names):
            player_name = self.handler.execute("get_player_name", "{}", "", "", "")
            return f"Sorry {player_name}, schema test."

        return "No-op"


class FakeLiteLlmProvider(agent_module.LlmProviderBackend):
    def complete(self, model: str, messages, tools):
        return {"content": f"default llm:{model}:{messages[-1]['content']}"}


def test_wrapper_supports_pydantic_ai_style_usage(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent(
        "gateway/gemini:gemini-3-flash-preview",
        deps_type=str,
        instructions=(
            "You're a dice game, you should roll the die and see if the number "
            "you get back matches the user's guess. If so, tell them they're a winner. "
            "Use the player's name in the response."
        ),
    )

    @agent.tool_plain
    def roll_dice() -> str:
        """Roll a six-sided die and return the result."""
        return "4"

    @agent.tool
    def get_player_name(ctx: agent_module.RunContext[str]) -> str:
        """Get the player's name."""
        return ctx.deps

    dice_result = agent.run_sync("My guess is 4", deps="Anne")

    assert (
        dice_result.output
        == "default llm:gateway/gemini:gemini-3-flash-preview:My guess is 4"
    )


def test_wrapper_builds_tool_schemas_and_passes_runtime_deps(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent("test-model", deps_type=str)

    @agent.tool_plain
    def format_score(total: int, lucky: bool = False) -> str:
        """Format a score summary."""
        return f"{total}:{lucky}"

    @agent.tool
    def get_player_name(ctx: agent_module.RunContext[str]) -> str:
        """Get the player's name."""
        return ctx.deps

    result = agent.run_sync("My guess is 1", deps="Anne")
    assert result.output == "default llm:test-model:My guess is 1"

    tools = FakeEnkiAgent.last_kwargs["tools"]
    score_spec = next(tool for tool in tools if tool.name == "format_score")
    schema = json.loads(score_spec.parameters_json)

    assert schema == {
        "type": "object",
        "properties": {
            "total": {"type": "integer"},
            "lucky": {"type": "boolean"},
        },
        "additionalProperties": False,
        "required": ["total"],
    }

    handler = FakeEnkiAgent.last_kwargs["handler"]
    handler.set_deps("Anne")
    try:
        assert handler.execute(
            "format_score",
            json.dumps({"total": 7, "lucky": True}),
            "",
            "",
            "",
        ) == "7:True"
        assert handler.execute("get_player_name", "{}", "", "", "") == "Anne"
    finally:
        handler.clear_deps()

    assert "include_builtin_tools" not in FakeEnkiAgent.last_kwargs


def test_wrapper_registers_concrete_tool_objects(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent("test-model")

    def format_score(total: int) -> str:
        """Format a score summary."""
        return f"score:{total}"

    tool = agent_module.Tool.from_function(format_score, uses_context=False)
    agent.register_tool(tool)

    result = agent.run_sync("My guess is 1")
    assert result.output == "default llm:test-model:My guess is 1"

    tools = FakeEnkiAgent.last_kwargs["tools"]
    score_spec = next(tool for tool in tools if tool.name == "format_score")
    assert json.loads(score_spec.parameters_json) == {
        "type": "object",
        "properties": {
            "total": {"type": "integer"},
        },
        "additionalProperties": False,
        "required": ["total"],
    }

    handler = FakeEnkiAgent.last_kwargs["handler"]
    assert handler.execute("format_score", json.dumps({"total": 7}), "", "", "") == "score:7"


def test_tool_handler_returns_error_string_for_invalid_args(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent("test-model")

    @agent.tool_plain
    def classify_severity(summary: str) -> str:
        return summary

    agent.run_sync("hello")

    handler = FakeEnkiAgent.last_kwargs["handler"]
    assert (
        handler.execute("classify_severity", "{}", "", "", "")
        == "Error: Missing required argument 'summary' for tool 'classify_severity'"
    )


def test_wrapper_supports_custom_llm_provider(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)

    class DemoProvider(agent_module.LlmProviderBackend):
        def complete(self, model: str, messages, tools):
            assert model == "demo-model"
            assert messages[-1]["content"] == "hello"
            assert tools == []
            return {"content": "provider response"}

    agent = agent_module.Agent("demo-model", llm=DemoProvider())
    result = agent.run_sync("hello")

    assert result.output == "provider response"


def test_wrapper_uses_litellm_provider_by_default(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent("default-model")
    result = agent.run_sync("hello")

    assert result.output == "default llm:default-model:hello"


def test_wrapper_embeds_custom_agentic_loop_in_prompt(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)
    monkeypatch.setattr(agent_module, "LiteLlmProvider", FakeLiteLlmProvider)

    agent = agent_module.Agent(
        "default-model",
        instructions="Keep replies short.",
        agentic_loop="1. Think.\n2. Use tools sparingly.\n3. Answer.",
    )
    agent.run_sync("hello")

    assert FakeEnkiAgent.last_kwargs["system_prompt_preamble"] == (
        "Keep replies short.\n"
        "<enki:agentic-loop>\n"
        "1. Think.\n2. Use tools sparingly.\n3. Answer.\n"
        "</enki:agentic-loop>"
    )


def test_litellm_provider_normalizes_completion(monkeypatch):
    calls = {}

    class FakeResponse:
        def model_dump(self):
            return {
                "model": "openai/gpt-4o-mini",
                "choices": [
                    {
                        "message": {
                            "content": "litellm response",
                            "tool_calls": [
                                {
                                    "id": "call-1",
                                    "function": {
                                        "name": "echo",
                                        "arguments": "{\"value\":\"hello\"}",
                                    },
                                    "type": "function",
                                }
                            ],
                        },
                        "finish_reason": "tool_calls",
                    }
                ],
            }

    fake_module = types.SimpleNamespace()

    def fake_completion(**kwargs):
        calls.update(kwargs)
        return FakeResponse()

    fake_module.completion = fake_completion
    monkeypatch.setattr(agent_module.importlib, "import_module", lambda name: fake_module)
    monkeypatch.delenv("OLLAMA_URL", raising=False)
    monkeypatch.delenv("ENKI_LITELLM_TIMEOUT", raising=False)
    monkeypatch.delenv("ENKI_OLLAMA_TOOLS", raising=False)

    provider = agent_module.LiteLlmProvider(temperature=0.2)
    result = provider.complete(
        "ollama::qwen3.5:latest",
        [{"role": "user", "content": "hello"}],
        [{"name": "echo", "description": "Echo a value"}],
    )

    assert calls["model"] == "ollama/qwen3.5:latest"
    assert calls["messages"] == [{"role": "user", "content": "hello"}]
    assert "tools" not in calls
    assert calls["api_base"] == "http://127.0.0.1:11434"
    assert calls["timeout"] == 60.0
    assert calls["temperature"] == 0.2
    assert result == {
        "content": "litellm response",
        "tool_calls": [
            json.dumps(
                {
                    "id": "call-1",
                    "function": {
                        "name": "echo",
                        "arguments": "{\"value\":\"hello\"}",
                    },
                    "type": "function",
                }
            )
        ],
        "model": "openai/gpt-4o-mini",
        "finish_reason": "tool_calls",
    }


def test_litellm_provider_can_send_ollama_tools_when_enabled(monkeypatch):
    calls = {}

    class FakeResponse:
        def model_dump(self):
            return {
                "model": "ollama/qwen3.5:latest",
                "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            }

    fake_module = types.SimpleNamespace()

    def fake_completion(**kwargs):
        calls.update(kwargs)
        return FakeResponse()

    fake_module.completion = fake_completion
    monkeypatch.setattr(agent_module.importlib, "import_module", lambda name: fake_module)
    monkeypatch.setenv("ENKI_OLLAMA_TOOLS", "true")

    provider = agent_module.LiteLlmProvider()
    provider.complete(
        "ollama::qwen3.5:latest",
        [{"role": "user", "content": "hello"}],
        [{"name": "echo", "description": "Echo a value"}],
    )

    assert calls["tools"] == [
        {
            "type": "function",
            "function": {
                "name": "echo",
                "description": "Echo a value",
                "parameters": {"type": "object", "properties": {}},
            },
        }
    ]


def test_litellm_provider_retries_empty_tool_response_without_tools(monkeypatch):
    calls = []

    class FakeResponse:
        def __init__(self, content: str):
            self._content = content

        def model_dump(self):
            return {
                "model": "openai/gpt-4o-mini",
                "choices": [
                    {
                        "message": {"content": self._content},
                        "finish_reason": "stop",
                    }
                ],
            }

    fake_module = types.SimpleNamespace()

    def fake_completion(**kwargs):
        calls.append(kwargs)
        if "tools" in kwargs:
            return FakeResponse("")
        return FakeResponse("fallback answer")

    fake_module.completion = fake_completion
    monkeypatch.setattr(agent_module.importlib, "import_module", lambda name: fake_module)

    provider = agent_module.LiteLlmProvider()
    result = provider.complete(
        "openai::gpt-4o-mini",
        [{"role": "user", "content": "hello"}],
        [{"name": "echo", "description": "Echo a value"}],
    )

    assert len(calls) == 2
    assert "tools" in calls[0]
    assert "tools" not in calls[1]
    assert result["content"] == "fallback answer"


def test_litellm_provider_normalizes_tool_messages_for_anthropic(monkeypatch):
    calls = {}

    class FakeResponse:
        def model_dump(self):
            return {
                "model": "anthropic/claude-sonnet-4-6",
                "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}],
            }

    fake_module = types.SimpleNamespace()

    def fake_completion(**kwargs):
        calls.update(kwargs)
        return FakeResponse()

    fake_module.completion = fake_completion
    monkeypatch.setattr(agent_module.importlib, "import_module", lambda name: fake_module)

    provider = agent_module.LiteLlmProvider()
    provider.complete(
        "anthropic::claude-sonnet-4-6",
        [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": ""},
            {"role": "tool", "content": "42", "tool_call_id": "call-1"},
        ],
        [],
    )

    assert calls["messages"] == [
        {"role": "user", "content": "hello"},
        {"role": "user", "content": "Tool result (tool_call_id=call-1): 42"},
    ]


def test_litellm_provider_missing_dependency_returns_error_message(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)

    def fail_import(name: str):
        raise ModuleNotFoundError("No module named 'litellm'")

    monkeypatch.setattr(agent_module.importlib, "import_module", fail_import)

    agent = agent_module.Agent("ollama::qwen3.5:latest")
    result = agent.run_sync("hello")

    assert result.output == (
        "LLM provider error: LiteLLM is not installed. Install it with `pip install enki-py[litellm]` "
        "or pass a custom `llm=` provider."
    )
