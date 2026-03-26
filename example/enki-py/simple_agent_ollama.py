import uuid

from enki_py import Agent



def main() -> None:
    agent = Agent("ollama::qwen3.5",
                  name="Simple Agent",
                  instructions="Answer clearly and keep responses short.",
                  )

    result = agent.run_sync(
        "Explain what this Enki Python example demonstrates.",
        session_id=f"simple-agent-ollama-{uuid.uuid4()}",
    )
    print(result.output)


if __name__ == "__main__":
    main()
