from __future__ import annotations

import asyncio
import importlib
import inspect
import json
import os
import threading
import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any, Callable, Generic, Optional, TypeVar, Union, get_args, get_origin

try:
    from .enki_py import EnkiAgent as _LowLevelEnkiAgent
    from .enki_py import EnkiMemoryEntry as _LowLevelMemoryEntry
    from .enki_py import EnkiMemoryHandler
    from .enki_py import EnkiMemoryKind as _LowLevelMemoryKind
    from .enki_py import EnkiMemoryModule as _LowLevelMemoryModule
    from .enki_py import EnkiToolHandler
except ImportError:  # pragma: no cover
    class _LowLevelEnkiAgent:  # type: ignore[override]
        pass


    class _LowLevelMemoryKind(Enum):  # type: ignore[override]
        RecentMessage = "RecentMessage"
        Summary = "Summary"
        Entity = "Entity"
        Preference = "Preference"


    @dataclass(frozen=True)
    class _LowLevelMemoryEntry:  # type: ignore[override]
        key: str
        content: str
        kind: _LowLevelMemoryKind
        relevance: float
        timestamp_ns: int


    @dataclass(frozen=True)
    class _LowLevelMemoryModule:  # type: ignore[override]
        name: str


    class EnkiToolHandler:  # type: ignore[override]
        pass


    class EnkiMemoryHandler:  # type: ignore[override]
        pass
try:
    from .enki_py import EnkiLlmHandler
except ImportError:  # pragma: no cover
    class EnkiLlmHandler:  # type: ignore[override]
        pass
try:
    from .enki_py import EnkiTool as _LowLevelTool
except ImportError:  # pragma: no cover
    try:
        from .enki_py import EnkiToolSpec as _LowLevelTool
    except ImportError:  # pragma: no cover
        @dataclass(frozen=True)
        class _LowLevelTool:  # type: ignore[override]
            name: str
            description: str
            parameters_json: str
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


@dataclass(frozen=True)
class AgentCard:
    agent_id: str
    name: str
    description: str
    capabilities: list[str]
    status: str = "online"


@dataclass(frozen=True)
class MultiAgentMember(Generic[DepsT]):
    agent_id: str
    agent: "Agent[DepsT]"
    capabilities: list[str]
    description: str | None = None
    status: str = "online"


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


class LlmProviderBackend(ABC):
    """Base class for custom Python LLM providers."""

    @abstractmethod
    def complete(
            self,
            model: str,
            messages: list[dict[str, Any]],
            tools: list[dict[str, Any]],
    ) -> Any:
        """Return either a plain string or a response object/dict."""


class LiteLlmProvider(LlmProviderBackend):
    """Python-side LiteLLM adapter.

    This keeps model/provider selection outside Rust and routes requests through
    ``litellm.completion``.
    """

    def __init__(self, **default_kwargs: Any) -> None:
        self._default_kwargs = default_kwargs

    def complete(
            self,
            model: str,
            messages: list[dict[str, Any]],
            tools: list[dict[str, Any]],
    ) -> dict[str, Any]:
        try:
            litellm = importlib.import_module("litellm")
        except ModuleNotFoundError as error:
            raise RuntimeError(
                "LiteLLM is not installed. Install it with `pip install litellm` "
                "or pass a custom `llm=` provider."
            ) from error

        provider_name, normalized_model = self._normalize_model(model)
        payload: dict[str, Any] = {
            "model": normalized_model,
            "messages": [
                normalized_message
                for message in messages
                if (
                    normalized_message := self._normalize_message(
                        message, provider_name=provider_name
                    )
                )
                is not None
            ],
        }
        if tools and self._should_send_tools(provider_name):
            payload["tools"] = [self._normalize_tool(tool) for tool in tools]
        payload.update(self._provider_defaults(provider_name))
        payload.update(self._default_kwargs)

        response = litellm.completion(**payload)
        return self._normalize_response(response, normalized_model)

    @staticmethod
    def _normalize_model(model: str) -> tuple[str | None, str]:
        if "::" not in model:
            if "/" in model:
                provider, _backend_model = model.split("/", 1)
                return provider.strip().lower(), model
            return None, model

        provider, backend_model = model.split("::", 1)
        provider = provider.strip().lower()
        backend_model = backend_model.strip()

        if not provider or not backend_model:
            return None, model

        aliases = {
            "azure-openai": "azure",
            "azure_openai": "azure",
            "azureopenai": "azure",
            "x.ai": "xai",
        }
        provider = aliases.get(provider, provider)
        return provider, f"{provider}/{backend_model}"

    @staticmethod
    def _provider_defaults(provider_name: str | None) -> dict[str, Any]:
        timeout = float(os.getenv("ENKI_LITELLM_TIMEOUT", "60"))
        if provider_name == "ollama":
            return {
                "api_base": (
                    os.getenv("OLLAMA_URL") or "http://127.0.0.1:11434"
                ).rstrip("/"),
                "timeout": timeout,
            }
        return {"timeout": timeout}

    @staticmethod
    def _should_send_tools(provider_name: str | None) -> bool:
        if provider_name == "ollama":
            return os.getenv("ENKI_OLLAMA_TOOLS", "").strip().lower() in {
                "1",
                "true",
                "yes",
                "on",
            }
        return True

    @staticmethod
    def _normalize_message(
        message: dict[str, Any],
        *,
        provider_name: str | None,
    ) -> dict[str, Any] | None:
        role = str(message.get("role", "user")).lower()
        content = message.get("content", "")
        tool_call_id = message.get("tool_call_id")

        if role == "tool":
            if tool_call_id:
                content = f"Tool result (tool_call_id={tool_call_id}): {content}"
            else:
                content = f"Tool result: {content}"

            return {
                "role": "user",
                "content": content,
            }

        if role == "assistant" and not content:
            return None

        normalized = {
            "role": role,
            "content": content,
        }

        if provider_name == "openai" and tool_call_id:
            normalized["tool_call_id"] = tool_call_id

        return normalized

    @staticmethod
    def _normalize_tool(tool: dict[str, Any]) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {
                "name": tool.get("name", ""),
                "description": tool.get("description"),
                "parameters": tool.get("parameters", {"type": "object", "properties": {}}),
            },
        }

    @classmethod
    def _normalize_response(cls, response: Any, model: str) -> dict[str, Any]:
        if hasattr(response, "model_dump"):
            payload = response.model_dump()
        elif isinstance(response, dict):
            payload = response
        else:
            payload = dict(response)

        choices = payload.get("choices") or []
        if not choices:
            return {
                "content": "",
                "tool_calls": [],
                "model": payload.get("model", model),
                "finish_reason": payload.get("finish_reason", "stop"),
            }

        message = choices[0].get("message", {}) or {}
        return {
            "content": message.get("content") or "",
            "tool_calls": [
                json.dumps(tool_call)
                for tool_call in (message.get("tool_calls") or [])
            ],
            "model": payload.get("model", model),
            "finish_reason": choices[0].get("finish_reason", "stop"),
        }


class _PythonLlmHandler(EnkiLlmHandler):
    def __init__(self,
                 provider: "LlmProviderBackend | Callable[[str, list[dict[str, Any]], list[dict[str, Any]]], Any]") -> None:
        self._provider = provider

    def complete(self, model: str, messages_json: str, tools_json: str) -> str:
        messages = json.loads(messages_json) if messages_json else []
        tools = json.loads(tools_json) if tools_json else []

        try:
            if isinstance(self._provider, LlmProviderBackend):
                result = _resolve_callback_result(self._provider.complete(model, messages, tools))
            else:
                result = _resolve_callback_result(self._provider(model, messages, tools))
        except Exception as error:
            return json.dumps(
                {
                    "content": f"LLM provider error: {error}",
                    "tool_calls": [],
                    "model": model,
                    "finish_reason": "error",
                }
            )

        if isinstance(result, str):
            return result
        return json.dumps(result)


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
            llm: LlmProviderBackend | Callable[[str, list[dict[str, Any]], list[dict[str, Any]]], Any] | None = None,
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
        provider = llm if llm is not None else LiteLlmProvider()
        self._llm_handler = _PythonLlmHandler(provider)
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

        if self._llm_handler is not None and tool_specs and memory_specs:
            self._backend = _LowLevelEnkiAgent.with_tools_memory_and_llm(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                tools=tool_specs,
                tool_handler=self._handler,
                memories=memory_specs,
                memory_handler=self._memory_handler,
                llm_handler=self._llm_handler,
            )
        elif self._llm_handler is not None and tool_specs:
            self._backend = _LowLevelEnkiAgent.with_tools_and_llm(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                tools=tool_specs,
                handler=self._handler,
                llm_handler=self._llm_handler,
            )
        elif self._llm_handler is not None and memory_specs:
            self._backend = _LowLevelEnkiAgent.with_memory_and_llm(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                memories=memory_specs,
                handler=self._memory_handler,
                llm_handler=self._llm_handler,
            )
        elif self._llm_handler is not None:
            self._backend = _LowLevelEnkiAgent.with_llm(
                name=self.name,
                system_prompt_preamble=self.instructions,
                model=self.model,
                max_iterations=self.max_iterations,
                workspace_home=self.workspace_home,
                llm_handler=self._llm_handler,
            )
        elif tool_specs and memory_specs:
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


class MultiAgentRuntime:
    def __init__(self, members: list[MultiAgentMember[Any]]) -> None:
        if not members:
            raise ValueError("MultiAgentRuntime requires at least one agent")

        self._members: dict[str, MultiAgentMember[Any]] = {}
        for member in members:
            if member.agent_id in self._members:
                raise ValueError(f"Duplicate agent_id '{member.agent_id}'")
            self._members[member.agent_id] = member

        for member in members:
            self._install_runtime_tools(member)

    def _install_runtime_tools(self, member: MultiAgentMember[Any]) -> None:
        for reserved_name in ("discover_agents", "delegate_task"):
            if reserved_name in member.agent._tools:
                raise ValueError(
                    f"Agent '{member.agent_id}' already defines reserved tool '{reserved_name}'"
                )

        def discover_agents(
                capability: str | None = None,
                status: str | None = None,
        ) -> list[dict[str, Any]]:
            cards = self.discover(capability=capability, status=status)
            return [card.__dict__ for card in cards if card.agent_id != member.agent_id]

        def delegate_task(agent_id: str, task: str) -> str:
            if agent_id == member.agent_id:
                return "Error: cannot delegate a task to yourself."

            target = self._members.get(agent_id)
            if target is None:
                return f"Error: agent '{agent_id}' not found in registry."

            result = target.agent.run_sync(
                task,
                session_id=f"delegation-{agent_id}-{uuid.uuid4()}",
            )
            return result.output

        member.agent.register_tool(
            Tool.from_function(
                discover_agents,
                uses_context=False,
                description=(
                    "Discover peer agents registered in the runtime. "
                    "Returns a JSON array of agent cards matching the query."
                ),
            )
        )
        member.agent.register_tool(
            Tool.from_function(
                delegate_task,
                uses_context=False,
                description=(
                    "Delegate a task to another agent by its agent_id. "
                    "Returns the peer agent's response."
                ),
            )
        )

    def registry(self) -> list[AgentCard]:
        return [
            AgentCard(
                agent_id=member.agent_id,
                name=member.agent.name,
                description=member.description or member.agent.instructions,
                capabilities=list(member.capabilities),
                status=member.status,
            )
            for member in self._members.values()
        ]

    def discover(
            self,
            *,
            capability: str | None = None,
            status: str | None = None,
    ) -> list[AgentCard]:
        cards = self.registry()
        if capability is not None:
            cards = [
                card
                for card in cards
                if any(candidate.lower() == capability.lower() for candidate in card.capabilities)
            ]
        if status is not None:
            cards = [card for card in cards if card.status.lower() == status.lower()]
        return cards

    async def process(
            self,
            agent_id: str,
            user_message: str,
            *,
            session_id: str | None = None,
    ) -> AgentRunResult:
        member = self._members.get(agent_id)
        if member is None:
            raise ValueError(f"Agent '{agent_id}' not found in runtime.")
        return await member.agent.run(user_message, session_id=session_id)

    def process_sync(
            self,
            agent_id: str,
            user_message: str,
            *,
            session_id: str | None = None,
    ) -> AgentRunResult:
        member = self._members.get(agent_id)
        if member is None:
            raise ValueError(f"Agent '{agent_id}' not found in runtime.")
        return member.agent.run_sync(user_message, session_id=session_id)


__all__ = [
    "AgentCard",
    "Agent",
    "AgentRunResult",
    "LlmProviderBackend",
    "MemoryBackend",
    "MemoryEntry",
    "MemoryKind",
    "MemoryModule",
    "MultiAgentMember",
    "MultiAgentRuntime",
    "RunContext",
    "Tool",
]
