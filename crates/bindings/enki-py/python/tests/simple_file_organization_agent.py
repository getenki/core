from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path

import enki_py.agent as agent_module

DEFAULT_MODEL = "ollama::qwen3.5:latest"
DEFAULT_MAX_ITERATIONS = 1000
DEFAULT_PROMPT = (
    "Review this folder's organization. Explain what it appears to contain, "
    "identify clutter or structural issues, and suggest a better layout."
)


@dataclass(frozen=True)
class FolderReviewDeps:
    root: Path


def _resolve_path(root: Path, relative_path: str) -> Path:
    candidate = (root / relative_path).resolve()
    root = root.resolve()
    if candidate != root and root not in candidate.parents:
        raise ValueError(f"Path escapes the review root: {relative_path}")
    return candidate


agent = agent_module.Agent(
    DEFAULT_MODEL,
    deps_type=FolderReviewDeps,
    name="Folder Review Agent",
    max_iterations=DEFAULT_MAX_ITERATIONS,
    workspace_home=None,
    instructions=(
        "You review and analyze folders using only the provided Python tools. "
        "Do not assume access to built-in filesystem tools. "
        "Start by inspecting the tree, then read only the most relevant files. "
        "Return a concise review covering structure, likely purpose, risks, and "
        "specific cleanup or organization recommendations."
    ),
)


@agent.tool
def list_directory(
        ctx: agent_module.RunContext[FolderReviewDeps],
        relative_path: str = ".",
        max_entries: int = 200,
) -> str:
    """List directory entries relative to the review root."""
    directory = _resolve_path(ctx.deps.root, relative_path)
    if not directory.exists():
        raise FileNotFoundError(f"Directory not found: {relative_path}")
    if not directory.is_dir():
        raise NotADirectoryError(f"Not a directory: {relative_path}")

    entries = []
    for entry in sorted(directory.iterdir(), key=lambda item: (not item.is_dir(), item.name.lower()))[
        : max_entries
    ]:
        entries.append(
            {
                "name": entry.name,
                "path": entry.relative_to(ctx.deps.root).as_posix(),
                "kind": "dir" if entry.is_dir() else "file",
                "size": entry.stat().st_size if entry.is_file() else None,
            }
        )
    return json.dumps(entries, indent=2)


@agent.tool
def read_text_file(
        ctx: agent_module.RunContext[FolderReviewDeps],
        relative_path: str,
        max_chars: int = 6000,
) -> str:
    """Read a UTF-8 text file from the review root."""
    file_path = _resolve_path(ctx.deps.root, relative_path)
    if not file_path.exists():
        raise FileNotFoundError(f"File not found: {relative_path}")
    if not file_path.is_file():
        raise IsADirectoryError(f"Expected a file: {relative_path}")

    text = file_path.read_text(encoding="utf-8", errors="replace")
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + "\n\n[truncated]"


@agent.tool
def find_files(
        ctx: agent_module.RunContext[FolderReviewDeps],
        glob_pattern: str,
        max_results: int = 200,
) -> str:
    """Find files under the review root using a glob pattern."""
    root = ctx.deps.root.resolve()
    matches = []
    for path in sorted(root.glob(glob_pattern)):
        if path.is_file():
            matches.append(path.relative_to(root).as_posix())
        if len(matches) >= max_results:
            break
    return json.dumps(matches, indent=2)


@agent.tool
def folder_summary(
        ctx: agent_module.RunContext[FolderReviewDeps],
        relative_path: str = ".",
        max_depth: int = 2,
) -> str:
    """Summarize file counts by extension within a folder."""
    root = _resolve_path(ctx.deps.root, relative_path)
    if not root.is_dir():
        raise NotADirectoryError(f"Not a directory: {relative_path}")

    extension_counts: dict[str, int] = {}
    file_count = 0
    dir_count = 0
    base_depth = len(root.relative_to(ctx.deps.root).parts)

    for path in root.rglob("*"):
        depth = len(path.relative_to(ctx.deps.root).parts) - base_depth
        if depth > max_depth:
            continue
        if path.is_dir():
            dir_count += 1
            continue
        file_count += 1
        extension = path.suffix.lower() or "<no_ext>"
        extension_counts[extension] = extension_counts.get(extension, 0) + 1

    summary = {
        "path": root.relative_to(ctx.deps.root).as_posix() or ".",
        "directories": dir_count,
        "files": file_count,
        "extensions": dict(sorted(extension_counts.items(), key=lambda item: (-item[1], item[0]))),
    }
    return json.dumps(summary, indent=2)


def review_folder(folder: str, prompt: str) -> str:
    root = Path(folder).resolve()
    result = agent.run_sync(prompt, deps=FolderReviewDeps(root=root))
    return result.output


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: simple_file_organization_agent.py <folder>")

    folder = sys.argv[1]
    prompt = DEFAULT_PROMPT
    review = review_folder(folder, prompt)
    print(review)

if __name__ == "__main__":
    main()
