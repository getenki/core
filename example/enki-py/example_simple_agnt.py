from enki_py import Agent

agent = Agent(
    "ollama::minimax-m2.5:cloud",
    name="Support Triage Agent",
    instructions=(
        "You triage incoming support issues. "
        "Classify severity, identify the likely subsystem, "
        "and recommend the next action for the support team. "
        "Use tools before making operational claims."
    ),
)


@agent.tool_plain
def lookup_runbook(service: str) -> str:
    """Return the internal runbook summary for a service."""
    runbooks = {
        "billing": "Check payment provider status, failed webhook retries, and recent invoice events.",
        "auth": "Check login error spikes, token expiry configuration, and identity provider health.",
        "api": "Check latency, deployment history, and upstream dependency failures.",
    }
    return runbooks.get(service.lower(), "No runbook entry found.")


@agent.tool_plain
def classify_severity(summary: str) -> str:
    """Return a simple severity label for a support issue."""
    text = summary.lower()
    if "all users" in text or "outage" in text:
        return "sev-1"
    if "payments failing" in text or "cannot login" in text:
        return "sev-2"
    return "sev-3"


result = agent.run_sync(
    "Customers report they cannot login after this morning's deployment.",
)

print("Agent output:")
print(repr(result.output))

if result.steps:
    print("\nExecution steps:")
    for step in result.steps:
        print(f"{step.index}. [{step.phase}] {step.kind}: {step.detail}")
