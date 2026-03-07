from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import subprocess
import tempfile
from typing import Any, Callable

from .llm import ToolSpec


DEFAULT_MAX_LINES = 2000
DEFAULT_MAX_BYTES = 50 * 1024
DEFAULT_LS_LIMIT = 500


TextContent = dict[str, Any]
AgentToolUpdateCallback = Callable[[dict[str, Any]], None]


@dataclass(slots=True)
class TruncationResult:
    content: str
    truncated: bool
    truncated_by: str | None
    total_lines: int
    total_bytes: int
    output_lines: int
    output_bytes: int
    last_line_partial: bool
    first_line_exceeds_limit: bool
    max_lines: int
    max_bytes: int

    def as_dict(self) -> dict[str, Any]:
        return {
            "content": self.content,
            "truncated": self.truncated,
            "truncatedBy": self.truncated_by,
            "totalLines": self.total_lines,
            "totalBytes": self.total_bytes,
            "outputLines": self.output_lines,
            "outputBytes": self.output_bytes,
            "lastLinePartial": self.last_line_partial,
            "firstLineExceedsLimit": self.first_line_exceeds_limit,
            "maxLines": self.max_lines,
            "maxBytes": self.max_bytes,
        }


@dataclass(slots=True)
class AgentToolResult:
    content: list[TextContent]
    details: dict[str, Any] | None = None


ToolExecutor = Callable[[str, dict[str, Any], AgentToolUpdateCallback | None], AgentToolResult]


@dataclass(slots=True)
class AgentTool:
    spec: ToolSpec
    label: str
    execute: ToolExecutor


class ToolRegistry:
    def __init__(self, tools: list[AgentTool]):
        self._tools = {tool.spec.name: tool for tool in tools}

    def specs(self) -> list[ToolSpec]:
        return [tool.spec for tool in self._tools.values()]

    def find(self, name: str) -> AgentTool | None:
        return self._tools.get(name)

    def validate_tool_arguments(self, spec: ToolSpec, arguments: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(arguments, dict):
            raise ValueError("Invalid tool arguments: expected object")

        schema = spec.input_schema
        required = schema.get("required", [])
        for key in required:
            if key not in arguments:
                raise ValueError(f"Missing required argument: {key}")

        properties = schema.get("properties", {})
        for key, value in arguments.items():
            prop = properties.get(key)
            if not isinstance(prop, dict):
                continue
            expected = prop.get("type")
            if expected == "string" and not isinstance(value, str):
                raise ValueError(f"Invalid type for '{key}': expected string")
            if expected == "number" and not isinstance(value, (int, float)):
                raise ValueError(f"Invalid type for '{key}': expected number")
            if expected == "integer" and not isinstance(value, int):
                raise ValueError(f"Invalid type for '{key}': expected integer")
        return arguments


def create_default_tools(cwd: Path) -> list[AgentTool]:
    return [
        create_ls_tool(cwd),
        create_read_tool(cwd),
        create_bash_tool(cwd),
    ]


def create_ls_tool(cwd: Path) -> AgentTool:
    spec = ToolSpec(
        name="ls",
        description=(
            "List directory contents. Returns entries sorted alphabetically, with '/' suffix "
            f"for directories. Includes dotfiles. Output is truncated to {DEFAULT_LS_LIMIT} entries "
            f"or {DEFAULT_MAX_BYTES // 1024}KB."
        ),
        input_schema={
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "limit": {"type": "number"},
            },
        },
    )

    def execute(_tool_call_id: str, params: dict[str, Any], _on_update: AgentToolUpdateCallback | None) -> AgentToolResult:
        path = _resolve_to_cwd(str(params.get("path", ".")), cwd)
        limit = int(params.get("limit", DEFAULT_LS_LIMIT))

        if not path.exists():
            raise FileNotFoundError(f"Path not found: {path}")
        if not path.is_dir():
            raise NotADirectoryError(f"Not a directory: {path}")

        entries = sorted(path.iterdir(), key=lambda p: p.name.lower())
        results: list[str] = []
        entry_limit_reached = False
        for entry in entries:
            if len(results) >= limit:
                entry_limit_reached = True
                break
            results.append(f"{entry.name}/" if entry.is_dir() else entry.name)

        if not results:
            return AgentToolResult(content=[{"type": "text", "text": "(empty directory)"}], details=None)

        raw_output = "\n".join(results)
        truncation = truncate_head(raw_output, max_lines=10**9)
        output = truncation.content
        details: dict[str, Any] = {}
        notices: list[str] = []

        if entry_limit_reached:
            notices.append(f"{limit} entries limit reached. Use limit={limit * 2} for more")
            details["entryLimitReached"] = limit
        if truncation.truncated:
            notices.append(f"{format_size(DEFAULT_MAX_BYTES)} limit reached")
            details["truncation"] = truncation.as_dict()
        if notices:
            output += f"\n\n[{' '.join(notices)}]"

        return AgentToolResult(content=[{"type": "text", "text": output}], details=details or None)

    return AgentTool(spec=spec, label="ls", execute=execute)


def create_read_tool(cwd: Path) -> AgentTool:
    spec = ToolSpec(
        name="read",
        description=(
            "Read file content. Text output is truncated to "
            f"{DEFAULT_MAX_LINES} lines or {DEFAULT_MAX_BYTES // 1024}KB."
        ),
        input_schema={
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "offset": {"type": "number"},
                "limit": {"type": "number"},
            },
            "required": ["path"],
        },
    )

    def execute(_tool_call_id: str, params: dict[str, Any], _on_update: AgentToolUpdateCallback | None) -> AgentToolResult:
        path = _resolve_read_path(str(params["path"]), cwd)
        offset = int(params["offset"]) if "offset" in params else None
        limit = int(params["limit"]) if "limit" in params else None

        if not path.exists():
            raise FileNotFoundError(f"Path not found: {path}")
        if not path.is_file():
            raise IsADirectoryError(f"Not a file: {path}")

        text = path.read_text(encoding="utf-8", errors="replace")
        all_lines = text.split("\n")
        total_lines = len(all_lines)

        start_line = max(0, (offset or 1) - 1)
        if start_line >= total_lines:
            raise ValueError(f"Offset {offset} is beyond end of file ({total_lines} lines total)")

        if limit is not None:
            end_line = min(start_line + limit, total_lines)
            selected = "\n".join(all_lines[start_line:end_line])
            user_limited_lines = end_line - start_line
        else:
            selected = "\n".join(all_lines[start_line:])
            user_limited_lines = None

        truncation = truncate_head(selected)
        output_text = truncation.content
        details: dict[str, Any] | None = None
        start_display = start_line + 1

        if truncation.first_line_exceeds_limit:
            line_size = format_size(len(all_lines[start_line].encode("utf-8")))
            output_text = (
                f"[Line {start_display} is {line_size}, exceeds {format_size(DEFAULT_MAX_BYTES)} limit. "
                f"Use bash to inspect this section.]"
            )
            details = {"truncation": truncation.as_dict()}
        elif truncation.truncated:
            end_display = start_display + truncation.output_lines - 1
            next_offset = end_display + 1
            output_text += (
                f"\n\n[Showing lines {start_display}-{end_display} of {total_lines}. "
                f"Use offset={next_offset} to continue.]"
            )
            details = {"truncation": truncation.as_dict()}
        elif user_limited_lines is not None and start_line + user_limited_lines < total_lines:
            next_offset = start_line + user_limited_lines + 1
            remaining = total_lines - (start_line + user_limited_lines)
            output_text += f"\n\n[{remaining} more lines in file. Use offset={next_offset} to continue.]"

        return AgentToolResult(content=[{"type": "text", "text": output_text}], details=details)

    return AgentTool(spec=spec, label="read", execute=execute)


def create_bash_tool(cwd: Path) -> AgentTool:
    spec = ToolSpec(
        name="bash",
        description=(
            "Execute a bash command in cwd. Returns stdout/stderr. Output is truncated to "
            f"last {DEFAULT_MAX_LINES} lines or {DEFAULT_MAX_BYTES // 1024}KB."
        ),
        input_schema={
            "type": "object",
            "properties": {
                "command": {"type": "string"},
                "timeout": {"type": "number"},
            },
            "required": ["command"],
        },
    )

    def execute(
        _tool_call_id: str,
        params: dict[str, Any],
        on_update: AgentToolUpdateCallback | None,
    ) -> AgentToolResult:
        command = str(params["command"])
        timeout = float(params.get("timeout", 0)) or None

        proc = subprocess.run(
            command,
            cwd=str(cwd),
            shell=True,
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
        output = "\n".join(part for part in [proc.stdout.strip(), proc.stderr.strip()] if part).strip()
        if not output:
            output = "(no output)"

        truncation = truncate_tail(output)
        final_text = truncation.content or "(no output)"
        details: dict[str, Any] = {"exitCode": proc.returncode}

        if on_update:
            on_update({"content": [{"type": "text", "text": final_text}], "details": details.copy()})

        if truncation.truncated:
            fd, full_path = tempfile.mkstemp(prefix="simple-agent-bash-", suffix=".log")
            os.close(fd)
            Path(full_path).write_text(output, encoding="utf-8")
            start_line = truncation.total_lines - truncation.output_lines + 1
            end_line = truncation.total_lines
            final_text += (
                f"\n\n[Showing lines {start_line}-{end_line} of {truncation.total_lines}. "
                f"Full output: {full_path}]"
            )
            details["truncation"] = truncation.as_dict()
            details["fullOutputPath"] = full_path

        return AgentToolResult(content=[{"type": "text", "text": final_text}], details=details)

    return AgentTool(spec=spec, label="bash", execute=execute)


def _resolve_to_cwd(file_path: str, cwd: Path) -> Path:
    expanded = _expand_path(file_path)
    p = Path(expanded)
    return p.resolve() if p.is_absolute() else (cwd / p).resolve()


def _resolve_read_path(file_path: str, cwd: Path) -> Path:
    return _resolve_to_cwd(file_path, cwd)


def _expand_path(file_path: str) -> str:
    p = file_path[1:] if file_path.startswith("@") else file_path
    return str(Path(p).expanduser())


def format_size(num_bytes: int) -> str:
    if num_bytes < 1024:
        return f"{num_bytes}B"
    if num_bytes < 1024 * 1024:
        return f"{num_bytes / 1024:.1f}KB"
    return f"{num_bytes / (1024 * 1024):.1f}MB"


def truncate_head(content: str, max_lines: int = DEFAULT_MAX_LINES, max_bytes: int = DEFAULT_MAX_BYTES) -> TruncationResult:
    total_bytes = len(content.encode("utf-8"))
    lines = content.split("\n")
    total_lines = len(lines)

    if total_lines <= max_lines and total_bytes <= max_bytes:
        return TruncationResult(
            content=content,
            truncated=False,
            truncated_by=None,
            total_lines=total_lines,
            total_bytes=total_bytes,
            output_lines=total_lines,
            output_bytes=total_bytes,
            last_line_partial=False,
            first_line_exceeds_limit=False,
            max_lines=max_lines,
            max_bytes=max_bytes,
        )

    first_line_bytes = len(lines[0].encode("utf-8")) if lines else 0
    if first_line_bytes > max_bytes:
        return TruncationResult(
            content="",
            truncated=True,
            truncated_by="bytes",
            total_lines=total_lines,
            total_bytes=total_bytes,
            output_lines=0,
            output_bytes=0,
            last_line_partial=False,
            first_line_exceeds_limit=True,
            max_lines=max_lines,
            max_bytes=max_bytes,
        )

    out: list[str] = []
    used_bytes = 0
    truncated_by = "lines"
    for i, line in enumerate(lines[:max_lines]):
        line_bytes = len(line.encode("utf-8")) + (1 if i > 0 else 0)
        if used_bytes + line_bytes > max_bytes:
            truncated_by = "bytes"
            break
        out.append(line)
        used_bytes += line_bytes

    output = "\n".join(out)
    output_bytes = len(output.encode("utf-8"))
    return TruncationResult(
        content=output,
        truncated=True,
        truncated_by=truncated_by,
        total_lines=total_lines,
        total_bytes=total_bytes,
        output_lines=len(out),
        output_bytes=output_bytes,
        last_line_partial=False,
        first_line_exceeds_limit=False,
        max_lines=max_lines,
        max_bytes=max_bytes,
    )


def truncate_tail(content: str, max_lines: int = DEFAULT_MAX_LINES, max_bytes: int = DEFAULT_MAX_BYTES) -> TruncationResult:
    total_bytes = len(content.encode("utf-8"))
    lines = content.split("\n")
    total_lines = len(lines)

    if total_lines <= max_lines and total_bytes <= max_bytes:
        return TruncationResult(
            content=content,
            truncated=False,
            truncated_by=None,
            total_lines=total_lines,
            total_bytes=total_bytes,
            output_lines=total_lines,
            output_bytes=total_bytes,
            last_line_partial=False,
            first_line_exceeds_limit=False,
            max_lines=max_lines,
            max_bytes=max_bytes,
        )

    out: list[str] = []
    used_bytes = 0
    truncated_by = "lines"
    for idx, line in enumerate(reversed(lines)):
        if idx >= max_lines:
            break
        line_bytes = len(line.encode("utf-8")) + (1 if out else 0)
        if used_bytes + line_bytes > max_bytes:
            truncated_by = "bytes"
            break
        out.insert(0, line)
        used_bytes += line_bytes

    output = "\n".join(out)
    output_bytes = len(output.encode("utf-8"))
    return TruncationResult(
        content=output,
        truncated=True,
        truncated_by=truncated_by,
        total_lines=total_lines,
        total_bytes=total_bytes,
        output_lines=len(out),
        output_bytes=output_bytes,
        last_line_partial=False,
        first_line_exceeds_limit=False,
        max_lines=max_lines,
        max_bytes=max_bytes,
    )
