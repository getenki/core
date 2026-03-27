from dataclasses import dataclass
from enki_py import Agent, RunContext

@dataclass
class AgentDeps:
    api_key: str = "dummy_key"

class HrManagerAstAgent(Agent[AgentDeps]):
    def __init__(self, model: str, name: str):
        super().__init__(model, name=name, deps_type=AgentDeps)
        self.tool(self.dummy_tool)

    def dummy_tool(self, ctx: RunContext[AgentDeps], query: str) -> str:
        """A dummy tool to demonstrate tool calling and dependencies."""
        return f"Dummy result for '{query}' using key '{ctx.deps.api_key}'"

deps = AgentDeps()
agent = HrManagerAstAgent("ollama::qwen3.5", name="hr_manager_ast")
