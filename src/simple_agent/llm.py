from __future__ import annotations

from dataclasses import dataclass
import itertools
import re
from typing import Any, Protocol


Message = dict[str, Any]


@dataclass(slots=True)
class ToolSpec:
    name: str
    description: str
    input_schema: dict[str, Any]


class ModelProvider(Protocol):
    def generate(self, messages: list[Message], tools: list[ToolSpec]) -> Message:
        """
        Returns an assistant message shaped like pi-mono AssistantMessage:
        {
          "role": "assistant",
          "content": [{"type":"text","text":"..."}, {"type":"toolCall",...}],
          "stopReason": "complete" | "error" | "aborted"
        }
        """


class MockProvider:
    """Deterministic local provider that emits pi-mono-like assistant messages."""

    def __init__(self) -> None:
        self._counter = itertools.count(1)

    def generate(self, messages: list[Message], tools: list[ToolSpec]) -> Message:
        latest = messages[-1] if messages else {"role": "user", "content": ""}
        latest_role = str(latest.get("role", "user"))
        latest_text = _extract_text(latest).strip().lower()

        if latest_role == "toolResult":
            return _assistant_text(f"Tool result:\n{_extract_text(latest)}")

        read_match = re.search(r"read(?: file)?\s+(.+)$", latest_text)
        if read_match and _has_tool(tools, "read"):
            return _assistant_with_tool_call(
                text="I'll read that file.",
                name="read",
                arguments={"path": read_match.group(1).strip().strip("'\"")},
                call_id=f"call_{next(self._counter)}",
            )

        ls_match = re.search(r"(?:^|\s)(ls|list files|show files)(?:$|\s)", latest_text)
        if ls_match and _has_tool(tools, "ls"):
            return _assistant_with_tool_call(
                text="I'll list the directory.",
                name="ls",
                arguments={"path": "."},
                call_id=f"call_{next(self._counter)}",
            )

        bash_match = re.search(r"(?:run command|bash)\s+(.+)$", latest_text)
        if bash_match and _has_tool(tools, "bash"):
            return _assistant_with_tool_call(
                text="I'll run that command.",
                name="bash",
                arguments={"command": bash_match.group(1).strip().strip("'\"")},
                call_id=f"call_{next(self._counter)}",
            )

        names = ", ".join(spec.name for spec in tools)
        return _assistant_text(
            "Mock provider active. Try: 'ls', 'read <path>', or 'bash <command>'. "
            f"Available tools: {names}."
        )


def _assistant_text(text: str) -> Message:
    return {
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "stopReason": "complete",
        "timestamp": _now_ms(),
    }


def _assistant_with_tool_call(text: str, name: str, arguments: dict[str, Any], call_id: str) -> Message:
    return {
        "role": "assistant",
        "content": [
            {"type": "text", "text": text},
            {"type": "toolCall", "id": call_id, "name": name, "arguments": arguments},
        ],
        "stopReason": "complete",
        "timestamp": _now_ms(),
    }


def _extract_text(message: Message) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        texts: list[str] = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                texts.append(str(item.get("text", "")))
        return "\n".join(t for t in texts if t)
    return str(content or "")


def _has_tool(tools: list[ToolSpec], name: str) -> bool:
    return any(tool.name == name for tool in tools)


def _now_ms() -> int:
    import time

    return int(time.time() * 1000)

