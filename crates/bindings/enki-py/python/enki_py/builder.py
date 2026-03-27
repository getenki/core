"""
Enki Python Builder CLI Runner

This module is invoked by the Rust Enki Builder CLI to instantiate
and run the configured agents in a Python environment.
"""

import importlib.util
import inspect
import sys
from pathlib import Path
from typing import Any, Dict

from .agent import Agent, MultiAgentMember, MultiAgentRuntime

if hasattr(sys.stdout, "reconfigure"):
    getattr(sys.stdout, "reconfigure")(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    getattr(sys.stderr, "reconfigure")(encoding="utf-8", errors="replace")


def load_python_module(project_dir: str, relative_path: str) -> Any:
    entry = Path(project_dir) / relative_path
    if not entry.exists():
        raise RuntimeError(f"Configured Python tool file was not found: {entry}")

    module_name = "enki_tool_" + str(abs(hash(str(entry))))
    spec = importlib.util.spec_from_file_location(module_name, entry)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load Python tool module: {entry}")

    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module) # type: ignore
    return module


def invoke_python_tool(module: Any, symbol: str, agent: Agent, agent_config: Dict[str, Any]) -> None:
    hook = getattr(module, symbol, None)
    if hook is None:
        raise RuntimeError(f"Python tool symbol '{symbol}' was not found in configured module: {module.__name__}")
    if not callable(hook):
        raise RuntimeError(f"Python tool symbol '{symbol}' is not callable in module: {module.__name__}")

    parameters = list(inspect.signature(hook).parameters.values())

    # If the first parameter is 'agent' or annotated as Agent, it's a registration hook.
    if parameters and (parameters[0].name == "agent" or getattr(parameters[0].annotation, "__name__", "") == "Agent" or parameters[0].annotation is Agent):
        if len(parameters) == 1:
            hook(agent)
        else:
            hook(agent, agent_config)
    else:
        # It's a direct tool function. We wrap it and register it automatically.
        import functools
        
        target_func = hook
        config_param = next((p for p in parameters if p.name in ("config", "agent_config", "tool_config")), None)
        
        if config_param is not None:
            # Bind the config param directly
            config_name = config_param.name
            
            def wrapper(*args: Any, **kwargs: Any) -> Any:
                kwargs[config_name] = agent_config
                return hook(*args, **kwargs) # type: ignore
                
            functools.update_wrapper(wrapper, hook)
            
            # Clean up the signature so the LLM doesn't see the config param
            sig = inspect.signature(hook)
            new_params = [p for p in sig.parameters.values() if p.name != config_name]
            wrapper.__signature__ = sig.replace(parameters=new_params) # type: ignore
            
            if hasattr(wrapper, "__annotations__"):
                wrapper.__annotations__ = {k: v for k, v in getattr(hook, "__annotations__", {}).items() if k != config_name}
                
            target_func = wrapper
                
        # Register the tool
        # Check if it uses Context by checking the remaining parameters
        final_sig = getattr(target_func, "__signature__", inspect.signature(target_func))
        uses_context = any(
            p.name in ("ctx", "context") or getattr(p.annotation, "__name__", "") == "RunContext" 
            for p in final_sig.parameters.values()
        )
        
        if uses_context:
            agent.tool(target_func)
        else:
            agent.tool_plain(target_func)


def main() -> None:
    if len(sys.argv) < 7:
        print("Usage: python -m enki_py.builder <project_dir> <workspace_home> <agent_id> <session_id> <message> <agent_count> ...", file=sys.stderr)
        sys.exit(1)

    project_dir = sys.argv[1]
    workspace_home = sys.argv[2]
    agent_id = sys.argv[3]
    session_id = sys.argv[4]
    message = sys.argv[5]
    try:
        agent_count = int(sys.argv[6])
    except ValueError:
        raise RuntimeError(f"Invalid agent count: {sys.argv[6]}")

    members = []
    index = 7
    capability_separator = chr(31)
    tool_separator = chr(30)
    module_cache = {}

    for _ in range(agent_count):
        if index + 7 >= len(sys.argv):
            raise RuntimeError("Missing arguments for configured agent")

        member_id = sys.argv[index]
        name = sys.argv[index + 1]
        model = sys.argv[index + 2]
        instructions = sys.argv[index + 3]
        max_iterations = int(sys.argv[index + 4])
        capabilities = [value for value in sys.argv[index + 5].split(capability_separator) if value]
        serialized_tools = [value for value in sys.argv[index + 6].split(capability_separator) if value]
        script_path = sys.argv[index + 7]
        index += 8

        if script_path:
            module = load_python_module(project_dir, script_path)
            agent = getattr(module, "agent", None)
            if agent is None:
                for obj in module.__dict__.values():
                    if isinstance(obj, Agent):
                        agent = obj
                        break
            if agent is None:
                raise RuntimeError(f"Could not find an Agent instance in {script_path}")
            
            if getattr(agent, "workspace_home", None) is None:
                agent.workspace_home = workspace_home # type: ignore

            script_deps = getattr(module, "deps", None)
            if script_deps is not None:
                original_run = agent.run
                original_run_sync = agent.run_sync
                
                async def patched_run(user_message: str, deps: Any = script_deps, **kwargs: Any) -> Any:
                    return await original_run(user_message, deps=deps, **kwargs)
                    
                def patched_run_sync(user_message: str, deps: Any = script_deps, **kwargs: Any) -> Any:
                    return original_run_sync(user_message, deps=deps, **kwargs)
                    
                agent.run = patched_run # type: ignore
                agent.run_sync = patched_run_sync # type: ignore
        else:
            agent = Agent(
                model,
                name=name,
                instructions=instructions,
                max_iterations=max_iterations,
                workspace_home=workspace_home,
            )
        agent_config = {
            "id": member_id,
            "name": name,
            "model": model,
            "system_prompt": instructions,
            "max_iterations": max_iterations,
            "capabilities": capabilities,
            "tools": serialized_tools,
        }

        for serialized_tool in serialized_tools:
            parts = serialized_tool.split(tool_separator)
            if len(parts) != 3:
                raise RuntimeError(f"Invalid tool configuration format: {serialized_tool}")
            
            kind, relative_path, symbol = parts
            if kind.lower() != "python":
                raise RuntimeError(f"Unsupported tool kind '{kind}' for Python runtime")
            
            module = module_cache.get(relative_path)
            if module is None:
                module = load_python_module(project_dir, relative_path)
                module_cache[relative_path] = module
            
            invoke_python_tool(module, symbol, agent, agent_config)

        members.append(
            MultiAgentMember(
                agent_id=member_id,
                agent=agent,
                capabilities=capabilities,
                description=instructions,
            )
        )

    runtime = MultiAgentRuntime(members)
    result = runtime.process_sync(agent_id, message, session_id=session_id)
    print(result.output)


if __name__ == "__main__":
    main()
