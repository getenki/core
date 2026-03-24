from enki_py import Agent, MultiAgentMember, MultiAgentRuntime


def main() -> None:
    model = "ollama::qwen3.5:latest"

    coordinator = Agent(
        model,
        name="Coordinator",
        instructions=(
            "You are a coordinator agent. "
            "Use discover_agents first. "
            "Delegate research work to the researcher with delegate_task. "
            "Return the delegated answer and mention which agent handled it."
        ),
    )
    researcher = Agent(
        model,
        name="Researcher",
        instructions=(
            "You are a research agent. "
            "Answer delegated questions clearly and briefly."
        ),
    )

    runtime = MultiAgentRuntime(
        [
            MultiAgentMember(
                agent_id="coordinator",
                agent=coordinator,
                capabilities=["planning", "orchestration"],
            ),
            MultiAgentMember(
                agent_id="researcher",
                agent=researcher,
                capabilities=["research"],
            ),
        ]
    )

    result = runtime.process_sync(
        "coordinator",
        (
            "Use discover_agents first. "
            "Then delegate_task to the researcher to answer: "
            "what is the purpose of this simple multi-agent example?"
        ),
        session_id="simple-multi-agent-example",
    )
    print(result.output)


if __name__ == "__main__":
    main()
