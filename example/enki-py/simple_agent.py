import os

from enki_py import Agent

print(os.environ["ANTHROPIC_API_KEY"])
def main() -> None:
    agent = Agent(
        "anthropic::claude-sonnet-4-6",
        name="Simple Agent",
        instructions="Answer clearly and keep responses short.",
    )

    result = agent.run_sync(
        "Explain what this Enki Python example demonstrates.",
        session_id="simple-agent-example",
    )
    print(result.output)


if __name__ == "__main__":
    main()
