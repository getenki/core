# my-app

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

This scaffold uses manifest-driven agents in `enki.toml`, so the simplest customization path here is still prompt-level:

- adjust `system_prompt` for the agent in `enki.toml`
- if you need a fully custom loop, move that agent into Python code and construct an `enki_py.Agent(..., agent_loop_handler=...)`

Reference examples in this repository:

- [`example/enki-py/custom_agentic_loop.py`](/I:/projects/enki/core-next/example/enki-py/custom_agentic_loop.py)
- [`example/enki-py/react_custom_agentic_loop.py`](/I:/projects/enki/core-next/example/enki-py/react_custom_agentic_loop.py)
- [`example/enki-py/compare_agent_loops.py`](/I:/projects/enki/core-next/example/enki-py/compare_agent_loops.py)
