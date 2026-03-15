from __future__ import annotations

import asyncio
import inspect
import json
import threading
import uuid
from dataclasses import dataclass
from typing import Any, Callable, Generic, Optional, TypeVar, Union, get_args, get_origin

from .enki_py import EnkiAgent as _LowLevelEnkiAgent
from .enki_py import EnkiToolHandler, EnkiToolSpec


DepsT = TypeVar("DepsT")


@dataclass(frozen=True)
class RunContext(Generic[DepsT]):
    deps: DepsT


@dataclass(frozen=True)
class AgentRunResult:
    output: str


@dataclass(frozen=True)
class _RegisteredTool:
    name: str
    func: Callable[..., Any]
    uses_context: bool
    parameters_json: str


class _PythonToolHandler(EnkiToolHandler):
    def __init__(self, tools: dict[str, _RegisteredTool]) -> None:
        self._tools = tools
        self._deps_lock = threading.Lock()
        self._current_deps: Any = None

    def set_deps(self, deps: Any) -> None:
        with self._deps_lock:
            self._current_deps = deps

    def clear_deps(self) -> None:
        with self._deps_lock:
            self._current_deps = None

    def execute(
        self,
        tool_name: str,
        args_json: str,
        agent_dir: str,
        workspace_dir: str,
        sessions_dir: str,
    ) -> str:
        tool = self._tools[tool_name]
        parsed_args = json.loads(args_json) if args_json else {}
        if parsed_args is None:
            parsed_args = {}
        if not isinstance(parsed_args, dict):
            raise TypeError(f"Tool '{tool_name}' expected JSON object args")

        bound_args = []
        if tool.uses_context:
            with self._deps_lock:
                deps = self._current_deps
            bound_args.append(RunContext(deps=deps))

        signature = inspect.signature(tool.func)
        parameters = list(signature.parameters.values())
        if tool.uses_context and parameters:
            parameters = parameters[1:]

        for parameter in parameters:
            if parameter.kind not in (
                inspect.Parameter.POSITIONAL_OR_KEYWORD,
                inspect.Parameter.KEYWORD_ONLY,
            ):
                raise TypeError(
                    f"Tool '{tool_name}' uses unsupported parameter kind: {parameter.kind}"
                )

            if parameter.name in parsed_args:
                bound_args.append(parsed_args[parameter.name])
            elif parameter.default is not inspect._empty:
                bound_args.append(parameter.default)
            else:
                raise TypeError(
                    f"Missing required argument '{parameter.name}' for tool '{tool_name}'"
                )

        result = tool.func(*bound_args)
        return _stringify_tool_result(result)


def _stringify_tool_result(value: Any) -> str:
    if isinstance(value, str):
        return value
    if value is None:
        return ""
    if isinstance(value, (int, float, bool)):
        return str(value)
    return json.dumps(value)


def _is_optional(annotation: Any) -> tuple[bool, Any]:
    origin = get_origin(annotation)
    if origin not in (Union, getattr(__import__("types"), "UnionType", Union)):
        return False, annotation

    args = [arg for arg in get_args(annotation) if arg is not type(None)]
    if len(args) != 1:
        return False, annotation
    return True, args[0]


def _json_schema_for_annotation(annotation: Any) -> dict[str, Any]:
    optional, inner = _is_optional(annotation)
    annotation = inner if optional else annotation

    if annotation in (inspect._empty, Any):
        schema: dict[str, Any] = {}
    elif annotation is str:
        schema = {"type": "string"}
    elif annotation is int:
        schema = {"type": "integer"}
    elif annotation is float:
        schema = {"type": "number"}
    elif annotation is bool:
        schema = {"type": "boolean"}
    else:
        origin = get_origin(annotation)
        args = get_args(annotation)

        if origin in (list, tuple):
            item_annotation = args[0] if args else Any
            schema = {
                "type": "array",
                "items": _json_schema_for_annotation(item_annotation),
            }
        elif origin is dict:
            value_annotation = args[1] if len(args) > 1 else Any
            schema = {
                "type": "object",
                "additionalProperties": _json_schema_for_annotation(value_annotation),
            }
        else:
            schema = {}

    if optional:
        if "type" in schema:
            schema["type"] = [schema["type"], "null"]
        elif schema:
            schema = {"anyOf": [schema, {"type": "null"}]}
        else:
            schema = {"type": ["string", "number", "integer", "boolean", "object", "array", "null"]}

    return schema


def _build_parameters_json(func: Callable[..., Any], uses_context: bool) -> str:
    signature = inspect.signature(func)
    parameters = list(signature.parameters.values())
    if uses_context and parameters:
        parameters = parameters[1:]

    properties: dict[str, Any] = {}
    required: list[str] = []

    for parameter in parameters:
        if parameter.kind not in (
            inspect.Parameter.POSITIONAL_OR_KEYWORD,
            inspect.Parameter.KEYWORD_ONLY,
        ):
            raise TypeError(
                f"Tool '{func.__name__}' uses unsupported parameter kind: {parameter.kind}"
            )

        properties[parameter.name] = _json_schema_for_annotation(parameter.annotation)
        if parameter.default is inspect._empty:
            required.append(parameter.name)

    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required

    return json.dumps(schema)


class Agent(Generic[DepsT]):
    def __init__(
        self,
        model: str,
        *,
        deps_type: type[DepsT] | None = None,
        instructions: str = "",
        name: str = "Agent",
        max_iterations: int = 20,
        workspace_home: str | None = None,
    ) -> None:
        self.model = model
        self.deps_type = deps_type
        self.instructions = instructions
        self.name = name
        self.max_iterations = max_iterations
        self.workspace_home = workspace_home
        self._tools: dict[str, _RegisteredTool] = {}
        self._handler = _PythonToolHandler(self._tools)
        self._backend: Any = None
        self._dirty = True

    def tool_plain(self, func: Callable[..., Any]) -> Callable[..., Any]:
        self._register_tool(func, uses_context=False)
        return func

    def tool(self, func: Callable[..., Any]) -> Callable[..., Any]:
        signature = inspect.signature(func)
        parameters = list(signature.parameters.values())
        if not parameters:
            raise TypeError(f"Tool '{func.__name__}' must accept a RunContext argument")
        self._register_tool(func, uses_context=True)
        return func

    def _register_tool(self, func: Callable[..., Any], *, uses_context: bool) -> None:
        description = inspect.getdoc(func) or ""
        self._tools[func.__name__] = _RegisteredTool(
            name=func.__name__,
            func=func,
            uses_context=uses_context,
            parameters_json=_build_parameters_json(func, uses_context),
        )
        self._dirty = True

    def _tool_specs(self) -> list[EnkiToolSpec]:
        return [
            EnkiToolSpec(
                name=tool.name,
                description=inspect.getdoc(tool.func) or "",
                parameters_json=tool.parameters_json,
            )
            for tool in self._tools.values()
        ]

    def _ensure_backend(self) -> Any:
        if self._backend is not None and not self._dirty:
            return self._backend

        self._backend = _LowLevelEnkiAgent.with_tools(
            name=self.name,
            system_prompt_preamble=self.instructions,
            model=self.model,
            max_iterations=self.max_iterations,
            workspace_home=self.workspace_home,
            tools=self._tool_specs(),
            handler=self._handler,
        )
        self._dirty = False
        return self._backend

    async def run(
        self,
        user_message: str,
        *,
        deps: DepsT | None = None,
        session_id: str | None = None,
    ) -> AgentRunResult:
        backend = self._ensure_backend()
        session_id = session_id or f"session-{uuid.uuid4()}"
        self._handler.set_deps(deps)
        try:
            output = await backend.run(session_id, user_message)
        finally:
            self._handler.clear_deps()
        return AgentRunResult(output=output)

    def run_sync(
        self,
        user_message: str,
        *,
        deps: DepsT | None = None,
        session_id: str | None = None,
    ) -> AgentRunResult:
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            return asyncio.run(self.run(user_message, deps=deps, session_id=session_id))

        result_box: dict[str, AgentRunResult] = {}
        error_box: dict[str, BaseException] = {}

        def runner() -> None:
            try:
                result_box["result"] = asyncio.run(
                    self.run(user_message, deps=deps, session_id=session_id)
                )
            except BaseException as error:  # pragma: no cover
                error_box["error"] = error

        thread = threading.Thread(target=runner, daemon=True)
        thread.start()
        thread.join()

        if "error" in error_box:
            raise error_box["error"]
        return result_box["result"]


__all__ = ["Agent", "AgentRunResult", "RunContext"]
