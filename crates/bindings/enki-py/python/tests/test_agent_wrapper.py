import json

import enki_py.agent as agent_module


class FakeEnkiAgent:
    last_kwargs = None

    def __init__(self, handler):
        self.handler = handler

    @classmethod
    def with_tools(cls, **kwargs):
        cls.last_kwargs = kwargs
        return cls(kwargs["handler"])

    async def run(self, session_id: str, user_message: str) -> str:
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


def test_wrapper_supports_pydantic_ai_style_usage(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)

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

    assert dice_result.output == (
        "Congratulations Anne, you guessed correctly! You're a winner!"
    )


def test_wrapper_builds_tool_schemas_and_passes_runtime_deps(monkeypatch):
    monkeypatch.setattr(agent_module, "_LowLevelEnkiAgent", FakeEnkiAgent)

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
    assert result.output.startswith("Sorry Anne")

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

    agent = agent_module.Agent("test-model")

    def format_score(total: int) -> str:
        """Format a score summary."""
        return f"score:{total}"

    tool = agent_module.Tool.from_function(format_score, uses_context=False)
    agent.register_tool(tool)

    result = agent.run_sync("My guess is 1")
    assert result.output == "No-op"

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
