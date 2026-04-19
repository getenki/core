from __future__ import annotations

import json
import uuid

from enki_py import (
    Agent,
    AgentLoopRequest,
    AgentLoopResult,
    ExecutionStep,
    LiteLlmProvider,
)


def extract_json(content: str) -> dict:
    raw = content.strip()
    if raw.startswith("```"):
        parts = raw.split("```")
        for part in parts:
            candidate = part.strip()
            if not candidate or candidate.lower() == "json":
                continue
            if "\n" in candidate:
                candidate = candidate.split("\n", 1)[1].strip()
            try:
                return json.loads(candidate)
            except json.JSONDecodeError:
                continue

    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        pass

    start = raw.find("{")
    end = raw.rfind("}")
    if start == -1 or end == -1 or end <= start:
        raise ValueError(f"Expected JSON object, got: {content}")
    return json.loads(raw[start:end + 1])


def lookup_example_topic(topic: str) -> str:
    facts = {
        "memory": "Memory lets Enki retain useful context across turns and sessions.",
        "tools": "Tools let Enki agents call Python or JavaScript functions for structured results.",
        "agent-loop": "The agent loop controls how the model reasons, acts, observes, retries, and finalizes.",
    }
    return facts.get(
        topic.lower(),
        f"No prepared fact exists for '{topic}'. Try memory, tools, or agent-loop.",
    )


tool_impls = {
    "lookup_example_topic": lookup_example_topic,
}
llm = LiteLlmProvider()


def react_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    tool_catalog = [
        {
            "name": name,
            "description": spec.get("description", ""),
            "parameters": spec.get("parameters", {}),
        }
        for name, spec in request.tools.items()
    ]

    working_messages = [
        {
            "role": "system",
            "content": (
                "You are operating a ReAct loop for an Enki agent.\n"
                "You must respond with JSON only.\n"
                "If you need a tool, reply with "
                '{"thought":"...","action":{"name":"tool_name","args":{...}}}.\n'
                "If you are ready to finish, reply with "
                '{"thought":"...","final":"..."}.\n'
                f"Available tools: {json.dumps(tool_catalog)}"
            ),
        },
        {
            "role": "system",
            "content": f"Original agent instructions:\n{request.system_prompt}",
        },
        *request.messages,
    ]

    steps: list[ExecutionStep] = []
    max_turns = max(1, min(request.max_iterations, 6))

    for turn in range(1, max_turns + 1):
        response = llm.complete(request.model, working_messages, [])
        content = str(response.get("content", "")).strip()
        decision = extract_json(content)

        thought = str(decision.get("thought", "No thought provided."))
        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="ReAct",
                kind="thought",
                detail=f"Turn {turn}: {thought}",
            )
        )

        final = decision.get("final")
        if isinstance(final, str) and final.strip():
            final_text = final.strip()
            steps.append(
                ExecutionStep(
                    index=len(steps) + 1,
                    phase="ReAct",
                    kind="final",
                    detail="Returned a final answer from the Python ReAct loop",
                )
            )
            return AgentLoopResult(output=final_text, steps=steps)

        action = decision.get("action") or {}
        tool_name = str(action.get("name", "")).strip()
        tool_args = action.get("args") or {}
        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="ReAct",
                kind="action",
                detail=f"Calling {tool_name} with {json.dumps(tool_args)}",
            )
        )

        tool = tool_impls.get(tool_name)
        if tool is None:
            observation = f"Unknown tool '{tool_name}'."
        else:
            try:
                observation = str(tool(**tool_args))
            except Exception as error:  # pragma: no cover - example defensive path
                observation = f"Tool error: {error}"

        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="ReAct",
                kind="observation",
                detail=observation,
            )
        )

        working_messages.append({"role": "assistant", "content": content})
        working_messages.append(
            {"role": "user", "content": f"Observation: {observation}"}
        )

    return AgentLoopResult(
        output="Max ReAct turns reached without producing a final answer.",
        steps=steps
        + [
            ExecutionStep(
                index=len(steps) + 1,
                phase="ReAct",
                kind="stop",
                detail="Reached the example loop turn limit",
            )
        ],
    )


def main() -> None:
    model = "ollama::qwen3.5:latest"
    agent = Agent(
        model,
        name="Python ReAct Loop Agent",
        instructions=(
            "Answer clearly. Use the lookup_example_topic tool when you need facts "
            "about memory, tools, or the agent loop."
        ),
        agent_loop_handler=react_loop,
    )

    @agent.tool_plain
    def lookup_example_topic(topic: str) -> str:
        """Return a canned fact about an Enki example topic."""
        return tool_impls["lookup_example_topic"](topic)

    result = agent.run_sync(
        "Use ReAct to explain how Enki tools and the agent loop fit together.",
        session_id=f"react-custom-agent-loop-{uuid.uuid4()}",
    )
    print("Execution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()
    print(result.output)


if __name__ == "__main__":
    main()
