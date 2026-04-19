from __future__ import annotations

import json
import os
import uuid

from enki_py import (
    Agent,
    AgentLoopRequest,
    AgentLoopResult,
    ExecutionStep,
    LiteLlmProvider,
    Tool,
)


QUESTION = (
    "Use any helpful tools to explain how Enki tools, memory, and the agent loop "
    "fit together."
)

COMMON_INSTRUCTIONS = (
    "Answer clearly and keep responses short. "
    "Use the lookup_example_topic tool when you need facts about memory, tools, or agent-loop."
)

llm = LiteLlmProvider()


def lookup_example_topic(topic: str) -> str:
    """Return a canned fact about an Enki example topic."""
    facts = {
        "memory": "Memory lets Enki retain useful context across turns and sessions.",
        "tools": "Tools let Enki agents call Python or JavaScript functions for structured results.",
        "agent-loop": "The agent loop controls how the model reasons, acts, observes, retries, and finalizes.",
    }
    return facts.get(
        topic.lower(),
        f"No prepared fact exists for '{topic}'. Try memory, tools, or agent-loop.",
    )


TOOL_IMPLS = {
    "lookup_example_topic": lookup_example_topic,
}


def build_lookup_tool() -> Tool:
    return Tool.from_function(lookup_example_topic, uses_context=False)


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


def summarize_tool_catalog(request: AgentLoopRequest[None]) -> list[dict]:
    return [
        {
            "name": name,
            "description": spec.get("description", ""),
            "parameters": spec.get("parameters", {}),
        }
        for name, spec in request.tools.items()
    ]


def planner_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    planner_messages = [
        {
            "role": "system",
            "content": (
                "You are operating a planner-executor loop for an Enki agent.\n"
                "Return JSON only with this shape:\n"
                '{"thought":"...","tool_calls":[{"name":"tool_name","args":{...}}]}\n'
                "Use no more than three tool calls.\n"
                f"Available tools: {json.dumps(summarize_tool_catalog(request))}"
            ),
        },
        {
            "role": "system",
            "content": f"Original agent instructions:\n{request.system_prompt}",
        },
        *request.messages,
    ]
    plan = extract_json(str(llm.complete(request.model, planner_messages, []).get("content", "")))

    steps = [
        ExecutionStep(
            index=1,
            phase="Planner",
            kind="plan",
            detail=str(plan.get("thought", "Planned the next actions.")),
        )
    ]

    observations: list[str] = []
    for tool_call in list(plan.get("tool_calls") or [])[:3]:
        tool_name = str(tool_call.get("name", "")).strip()
        tool_args = tool_call.get("args") or {}
        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="Planner",
                kind="action",
                detail=f"Calling {tool_name} with {json.dumps(tool_args)}",
            )
        )
        tool = TOOL_IMPLS.get(tool_name)
        if tool is None:
            observation = f"Unknown tool '{tool_name}'."
        else:
            try:
                observation = str(tool(**tool_args))
            except Exception as error:  # pragma: no cover - example defensive path
                observation = f"Tool error: {error}"
        observations.append(f"{tool_name}: {observation}")
        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="Planner",
                kind="observation",
                detail=observation,
            )
        )

    final_messages = [
        {"role": "system", "content": request.system_prompt},
        {
            "role": "user",
            "content": (
                f"Question: {request.user_message}\n\n"
                f"Tool observations:\n- " + "\n- ".join(observations)
                if observations
                else f"Question: {request.user_message}\n\nNo tool observations were collected."
            ),
        },
    ]
    final_output = str(llm.complete(request.model, final_messages, []).get("content", "")).strip()
    steps.append(
        ExecutionStep(
            index=len(steps) + 1,
            phase="Planner",
            kind="final",
            detail="Synthesized the final answer after the planning phase",
        )
    )
    return AgentLoopResult(output=final_output, steps=steps)


def react_loop(request: AgentLoopRequest[None]) -> AgentLoopResult:
    working_messages = [
        {
            "role": "system",
            "content": (
                "You are operating a ReAct loop for an Enki agent.\n"
                "You must respond with JSON only.\n"
                'If you need a tool, reply with {"thought":"...","action":{"name":"tool_name","args":{...}}}.\n'
                'If you are ready to finish, reply with {"thought":"...","final":"..."}.\n'
                f"Available tools: {json.dumps(summarize_tool_catalog(request))}"
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
        content = str(llm.complete(request.model, working_messages, []).get("content", "")).strip()
        decision = extract_json(content)

        steps.append(
            ExecutionStep(
                index=len(steps) + 1,
                phase="ReAct",
                kind="thought",
                detail=f"Turn {turn}: {decision.get('thought', 'No thought provided.')}",
            )
        )

        final = decision.get("final")
        if isinstance(final, str) and final.strip():
            steps.append(
                ExecutionStep(
                    index=len(steps) + 1,
                    phase="ReAct",
                    kind="final",
                    detail="Returned a final answer from the ReAct loop",
                )
            )
            return AgentLoopResult(output=final.strip(), steps=steps)

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

        tool = TOOL_IMPLS.get(tool_name)
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
        working_messages.append({"role": "user", "content": f"Observation: {observation}"})

    steps.append(
        ExecutionStep(
            index=len(steps) + 1,
            phase="ReAct",
            kind="stop",
            detail="Reached the example turn limit without a final answer",
        )
    )
    return AgentLoopResult(
        output="Max ReAct turns reached without producing a final answer.",
        steps=steps,
    )


def build_default_agent(model: str) -> Agent:
    agent = Agent(
        model,
        name="Default Loop",
        instructions=COMMON_INSTRUCTIONS,
        max_iterations=8,
    )
    agent.register_tool(build_lookup_tool())
    return agent


def build_prompt_loop_agent(model: str) -> Agent:
    agent = Agent(
        model,
        name="Prompt Custom Loop",
        instructions=COMMON_INSTRUCTIONS,
        agentic_loop=(
            "1. Understand the user's request.\n"
            "2. Decide which facts are missing.\n"
            "3. Use lookup_example_topic before answering when memory, tools, or agent-loop facts are needed.\n"
            "4. Summarize the observations.\n"
            "5. Return the final answer."
        ),
        max_iterations=8,
    )
    agent.register_tool(build_lookup_tool())
    return agent


def build_planner_agent(model: str) -> Agent:
    agent = Agent(
        model,
        name="Planner Custom Loop",
        instructions=COMMON_INSTRUCTIONS,
        agent_loop_handler=planner_loop,
        max_iterations=8,
    )
    agent.register_tool(build_lookup_tool())
    return agent


def build_react_agent(model: str) -> Agent:
    agent = Agent(
        model,
        name="ReAct Custom Loop",
        instructions=COMMON_INSTRUCTIONS,
        agent_loop_handler=react_loop,
        max_iterations=8,
    )
    agent.register_tool(build_lookup_tool())
    return agent


def print_result(label: str, output: str, steps: list[ExecutionStep]) -> None:
    print("=" * 88)
    print(label)
    print("-" * 88)
    print("Output:")
    print(output)
    print("\nExecution steps:")
    for step in steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
    print()


def main() -> None:
    model = os.getenv("ENKI_MODEL", "ollama::qwen3.5:latest")
    agents = [
        ("Default runtime loop", build_default_agent(model)),
        ("Prompt-customized loop", build_prompt_loop_agent(model)),
        ("Planner custom loop", build_planner_agent(model)),
        ("ReAct custom loop", build_react_agent(model)),
    ]

    print(f"Model: {model}")
    print(f"Question: {QUESTION}\n")

    for label, agent in agents:
        session_id = f"compare-loops-{label.lower().replace(' ', '-')}-{uuid.uuid4()}"
        result = agent.run_sync(QUESTION, session_id=session_id)
        print_result(label, result.output, result.steps)


if __name__ == "__main__":
    main()
