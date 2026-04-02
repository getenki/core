import uuid

from enki_py import Agent


def main() -> None:
    agent = Agent(
        "anthropic::claude-sonnet-4-6",
        name="Custom Agentic Loop",
        instructions="Answer clearly and keep responses short.",
        agentic_loop=(
            "1. Understand the user request.\n"
            "2. Decide whether a tool is necessary before acting.\n"
            "3. If you use a tool, summarize what you learned.\n"
            "4. Verify that the answer is complete.\n"
            "5. Return the final response."
        ),
    )

    result = agent.run_sync(
        "Explain how this example customizes the agentic loop.",
        session_id=f"custom-agentic-loop-{uuid.uuid4()}",
    )
    print("Execution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()
    print(result.output)


if __name__ == "__main__":
    main()
