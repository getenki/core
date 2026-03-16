from __future__ import annotations

import asyncio
import inspect
import json
import threading
import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any, Callable, Generic, Optional, TypeVar, Union, get_args, get_origin

from .enki_py import EnkiAgent as _LowLevelEnkiAgent
from .enki_py import EnkiMemoryEntry as _LowLevelMemoryEntry
from .enki_py import EnkiMemoryHandler
from .enki_py import EnkiMemoryKind as _LowLevelMemoryKind
from .enki_py import EnkiMemoryModule as _LowLevelMemoryModule
from .enki_py import EnkiToolHandler
try:
    from .enki_py import EnkiTool as _LowLevelTool
except ImportError:  # pragma: no cover
    from .enki_py import EnkiToolSpec as _LowLevelTool
try:
    from .enki_py.enki import uniffi_set_event_loop as _uniffi_set_event_loop
except ImportError:  # pragma: no cover
    _uniffi_set_event_loop = None


DepsT = TypeVar("DepsT")
_CALLBACK_EVENT_LOOP: asyncio.AbstractEventLoop | None = None


@dataclass(frozen=True)
class RunContext(Generic[DepsT]):
    deps: DepsT


@dataclass(frozen=True)
class AgentRunResult:
    output: str


class MemoryKind(str, Enum):
    RECENT_MESSAGE = "RecentMessage"
    SUMMARY = "Summary"
    ENTITY = "Entity"
    PREFERENCE = "Preference"


@dataclass(frozen=True)
class MemoryEntry:
    key: str
    content: str
    kind: MemoryKind
    relevance: float
    timestamp_ns: int

    def as_low_level_entry(self) -> _LowLevelMemoryEntry:
        return _LowLevelMemoryEntry(
            key=self.key,
            content=self.content,
            kind=_LowLevelMemoryKind[self.kind.name],
            relevance=self.relevance,
            timestamp_ns=self.timestamp_ns,
        )

    @classmethod
    def from_low_level(cls, entry: _LowLevelMemoryEntry) -> "MemoryEntry":
        return cls(
            key=entry.key,
            content=entry.content,
            kind=MemoryKind[entry.kind.name],
            relevance=entry.relevance,
            timestamp_ns=entry.timestamp_ns,
        )


@dataclass(frozen=True)
class Tool:
    name: str
    description: str
    parameters_json: str
    func: Callable[..., Any]
    uses_context: bool

    @classmethod
    def from_function(
        cls,
        func: Callable[..., Any],
        *,
        uses_context: bool,
        name: str | None = None,
        description: str | None = None,
        parameters_json: str | None = None,
    ) -> Tool:
        tool_name = name or func.__name__
        tool_description = description or inspect.getdoc(func) or ""
        tool_parameters_json = parameters_json or _build_parameters_json(func, uses_context)
        return cls(
            name=tool_name,
            description=tool_description,
            parameters_json=tool_parameters_json,
            func=func,
            uses_context=uses_context,
        )

    def as_low_level_tool(self) -> _LowLevelTool:
        return _LowLevelTool(
            name=self.name,
            description=self.description,
            parameters_json=self.parameters_json,
        )


@dataclass(frozen=True)
class MemoryModule:
    """Python memory callbacks.

    Each callback may be a normal function or an ``async def`` coroutine
    function. Async callbacks are executed on the active event loop when one
    is available.
    """

    name: str
    record: Callable[[str, str, str], Any]
    recall: Callable[[str, str, int], Any]
    flush: Callable[[str], Any] | None = None
    consolidate: Callable[[str], Any] | None = None

    def as_low_level_memory(self) -> _LowLevelMemoryModule:
        return _LowLevelMemoryModule(name=self.name)


class MemoryBackend(ABC):
    """Base class for custom Python memory backends.

    Implementations may provide either synchronous methods or ``async def``
    methods for ``record``, ``recall``, ``flush``, and ``consolidate``.
    """

    name: str = "memory"

    @abstractmethod
    def record(self, session_id: str, user_msg: str, assistant_msg: str) -> Any:
        """Store a user and assistant exchange for a session.

        May return ``None`` directly or an awaitable resolving to ``None``.
        """

    @abstractmethod
    def recall(
        self,
        session_id: str,
        query: str,
        max_entries: int,
    ) -> Any:
        """Return entries relevant to the current query.

        May return ``list[MemoryEntry]`` directly or an awaitable resolving to
        that list.
        """

    @abstractmethod
    def flush(self, session_id: str) -> Any:
        """Persist or clear buffered session state.

        May return ``None`` directly or an awaitable resolving to ``None``.
        """

    def consolidate(self, session_id: str) -> Any:
        """Optional hook for summarization or compaction.

        May return ``None`` directly or an awaitable resolving to ``None``.
        """
        return None

    def as_memory_module(self) -> MemoryModule:
        return MemoryModule(
            name=self.name,
            record=self.record,
            recall=self.recall,
            flush=self.flush,
            consolidate=self.consolidate,
        )


class _PythonToolHandler(EnkiToolHandler):
    def __init__(self, tools: dict[str, Tool]) -> None:
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

        result = _resolve_callback_result(tool.func(*bound_args))
        return _stringify_tool_result(result)


class _PythonMemoryHandler(EnkiMemoryHandler):
    def __init__(self, memories: dict[str, MemoryModule]) -> None:
        self._memories = memories

    def record(
        self,
        memory_name: str,
        session_id: str,
        user_msg: str,
        assistant_msg: str,
    ) -> None:
        memory = self._memories[memory_name]
        _resolve_callback_result(memory.record(session_id, user_msg, assistant_msg))

    def recall(
        self,
        memory_name: str,
        session_id: str,
        query: str,
        max_entries: int,
    ) -> list[_LowLevelMemoryEntry]:
        memory = self._memories[memory_name]
        entries = _resolve_callback_result(memory.recall(session_id, query, max_entries))
        entries = entries or []
        return [entry.as_low_level_entry() for entry in entries]

    def flush(self, memory_name: str, session_id: str) -> None:
        memory = self._memories[memory_name]
        if memory.flush is not None:
            _resolve_callback_result(memory.flush(session_id))

    def consolidate(self, memory_name: str, session_id: str) -> None:
        memory = self._memories[memory_name]
        if memory.consolidate is not None:
            _resolve_callback_result(memory.consolidate(session_id))


def _resolve_callback_result(value: Any) -> Any:
    if not inspect.isawaitable(value):
        return value

    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        loop = _CALLBACK_EVENT_LOOP

    if loop is not None and loop.is_running():
        future = asyncio.run_coroutine_threadsafe(value, loop)
        return future.result()

    return asyncio.run(value)


def _try_set_uniffi_event_loop() -> None:
    global _CALLBACK_EVENT_LOOP
    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        return
    _CALLBACK_EVENT_LOOP = loop
    if _uniffi_set_event_loop is not None:
        _uniffi_set_event_loop(loop)


async def _maybe_await(value: Any) -> Any:
    if inspect.isawaitable(value):
        return await value
    return value


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
        tools: list[Tool] | None = None,
        memories: list[MemoryModule] | None = None,
    ) -> None:
        self.model = model
        self.deps_type = deps_type
        self.instructions = instructions
        self.name = name
        self.max_iterations = max_iterations
        self.workspace_home = workspace_home
        self._tools: dict[str, Tool] = {}
        self._memories: dict[str, MemoryModule] = {}
        self._handler = _PythonToolHandler(self._tools)
        self._memory_handler = _PythonMemoryHandler(self._memories)
        self._backend: Any = None
        self._dirty = True
        if tools:
            for tool in tools:
                self.register_tool(tool)
        if memories:
            for memory in memories:
                self.register_memory(memory)

    def tool_plain(self, func: Callable[..., Any]) -> Callable[..., Any]:
        self.register_tool(Tool.from_function(func, uses_context=False))
        return func

    def tool(self, func: Callable[..., Any]) -> Callable[..., Any]:
        signature = inspect.signature(func)
        parameters = list(signature.parameters.values())
        if not parameters:
            raise TypeError(f"Tool '{func.__name__}' must accept a RunContext argument")
        self.register_tool(Tool.from_function(func, uses_context=True))
        return func

    def register_tool(self, tool: Tool) -> Tool:
        self._tools[tool.name] = tool
        self._dirty = True
        return tool

    def register_memory(self, memory: MemoryModule) -> MemoryModule:
        self._memories[memory.name] = memory
        self._dirty = True
        return memory

    def _tool_specs(self) -> list[_LowLevelTool]:
        return [tool.as_low_level_tool() for tool in self._tools.values()]

    def _memory_specs(self) -> list[_LowLevelMemoryModule]:
        return [memory.as_low_level_memory() for memory in self._memories.values()]

    def _ensure_backend(self) -> Any:
        if self._backend is not None and not self._dirty:
            return self._backend

        tool_specs = self._tool_specs()
        memory_specs = self._memory_specs()

        if tool_specs and memory_specs:
            self._backend = _LowLevelEnkiAgent.with_tools_and_memory(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                tools=tool_specs,
                tool_handler=self._handler,
                memories=memory_specs,
                memory_handler=self._memory_handler,
            )
        elif tool_specs:
            self._backend = _LowLevelEnkiAgent.with_tools(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                tools=tool_specs,
                handler=self._handler,
            )
        elif memory_specs:
            self._backend = _LowLevelEnkiAgent.with_memory(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                memories=memory_specs,
                handler=self._memory_handler,
            )
        else:
            self._backend = _LowLevelEnkiAgent(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
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
        _try_set_uniffi_event_loop()
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


__all__ = [
    "Agent",
    "AgentRunResult",
    "MemoryBackend",
    "MemoryEntry",
    "MemoryKind",
    "MemoryModule",
    "RunContext",
    "Tool",
]
