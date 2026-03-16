from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
)

result = agent.run_sync("Explain what this project does.")
print(result.output)
