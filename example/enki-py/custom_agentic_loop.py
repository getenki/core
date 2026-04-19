import uuid

from enki_py import Agent, AgentLoopRequest, AgentLoopResult, ExecutionStep


def custom_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    return AgentLoopResult(
        output=(
            "This response was produced by a Python-defined agent loop override. "
            f"The original user request was: {request.user_message}"
        ),
        steps=[
            ExecutionStep(
                index=1,
                phase="Custom",
                kind="inspect_request",
                detail=(
                    f"Read {len(request.messages)} message(s) and "
                    f"{len(request.tools)} available tool definition(s)"
                ),
            ),
            ExecutionStep(
                index=2,
                phase="Custom",
                kind="final",
                detail="Returned a final response directly from Python",
            ),
        ],
    )


def main() -> None:
    agent = Agent(
        "anthropic::claude-sonnet-4-6",
        name="Custom Agentic Loop",
        instructions="Answer clearly and keep responses short.",
        agent_loop_handler=custom_loop,
    )

    result = agent.run_sync(
        "Explain how this example overrides the default agentic loop.",
        session_id=f"custom-agentic-loop-{uuid.uuid4()}",
    )
    print("Execution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()
    print(result.output)


if __name__ == "__main__":
    main()
