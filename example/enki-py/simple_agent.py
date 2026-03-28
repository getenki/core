import uuid

from enki_py import Agent


def main() -> None:
    agent = Agent(
        "anthropic::claude-sonnet-4-6",
        name="Simple Agent",
        instructions="Answer clearly and keep responses short.",
    )

    result = agent.run_sync(
        "Explain what this Enki Python example demonstrates.",
        session_id=f"simple-agent-anthropic-{uuid.uuid4()}",
    )
    print("Execution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()
    print(result.output)


if __name__ == "__main__":
    main()
