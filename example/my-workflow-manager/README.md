# my-workflow-manager

An [Enki](https://docs.getenki.com) multi-agent project.

## Getting Started

```bash
# Install dependencies
enki build

# Run agents
enki run --message "Hello, agents!"

# Interactive mode
enki join
```

## Configuration

Edit `enki.toml` to add agents, change models, or update capabilities.
Python projects can define reusable `[[tool]]` entries with `path` and `symbol`.

## Custom Agentic Loops

Workflow manifests still use the configured agent prompt and workflow definitions by default.

If you want a workflow agent with a custom loop:

- keep the workflow definitions in `workflows/`
- build the agent in Python with `enki_py.Agent(..., agent_loop_handler=...)`
- export it as a workflow agent with `as_workflow_agent(...)`

Reference examples in this repository:

- [`example/enki-py/custom_agentic_loop.py`](/I:/projects/enki/core-next/example/enki-py/custom_agentic_loop.py)
- [`example/enki-py/react_custom_agentic_loop.py`](/I:/projects/enki/core-next/example/enki-py/react_custom_agentic_loop.py)
- [`example/enki-py/agent_workflow.py`](/I:/projects/enki/core-next/example/enki-py/agent_workflow.py)
