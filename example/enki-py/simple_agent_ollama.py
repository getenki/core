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
    print("Execution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()
    print(result.output)


if __name__ == "__main__":
    main()
