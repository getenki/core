from enki_py import Agent

agent = Agent(
    "ollama::qwen3.5:latest",
    instructions="Answer clearly and keep responses short.",
)


async def hello_enki():
    result = await agent.run("Explain what this project does.")
    print(result.output)


if __name__ == "__main__":
    import asyncio

    asyncio.run(hello_enki())
