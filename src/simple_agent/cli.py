from __future__ import annotations

import argparse
from pathlib import Path

from .agent import AgentContext, AgentLoopConfig, agent_loop
from .llm import MockProvider
from .tools import ToolRegistry, create_default_tools


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="SimpleAgent CLI")
    parser.add_argument("prompt", nargs="*", help="Prompt to run")
    parser.add_argument("--cwd", default=".", help="Workspace directory for tools")
    parser.add_argument("--system-prompt", default="You are SimpleAgent.", help="System prompt")
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    prompt = " ".join(args.prompt).strip()
    if not prompt:
        parser.error("prompt is required")

    cwd = Path(args.cwd).resolve()
    registry = ToolRegistry(create_default_tools(cwd))
    context = AgentContext(system_prompt=args.system_prompt, messages=[], tools=registry)
    config = AgentLoopConfig(provider=MockProvider())

    user_message = {"role": "user", "content": prompt, "timestamp": _now_ms()}
    for event in agent_loop([user_message], context, config):
        _print_event(event.type, event.payload)


def _print_event(event_type: str, payload: dict) -> None:
    if event_type == "agent_start":
        print("[agent] start")
    elif event_type == "agent_end":
        print("[agent] end")
    elif event_type == "turn_start":
        print("[turn] start")
    elif event_type == "turn_end":
        print("[turn] end")
    elif event_type == "message_start":
        msg = payload.get("message", {})
        role = msg.get("role", "?")
        text = _message_text(msg)
        if text:
            print(f"[message:start] {role}: {text}")
        else:
            print(f"[message:start] {role}")
    elif event_type == "message_end":
        msg = payload.get("message", {})
        role = msg.get("role", "?")
        print(f"[message:end] {role}")
    elif event_type == "tool_execution_start":
        print(
            f"[tool:start] {payload.get('toolName')} "
            f"id={payload.get('toolCallId')} args={payload.get('args')}"
        )
    elif event_type == "tool_execution_update":
        print(f"[tool:update] {payload.get('toolName')} id={payload.get('toolCallId')}")
    elif event_type == "tool_execution_end":
        status = "error" if payload.get("isError") else "ok"
        result = payload.get("result", {})
        text = _message_text({"content": result.get("content", [])})
        print(f"[tool:end:{status}] {payload.get('toolName')} id={payload.get('toolCallId')}")
        if text:
            print(text)


def _message_text(message: dict) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        chunks: list[str] = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                chunks.append(str(item.get("text", "")))
        return "\n".join(c for c in chunks if c)
    return ""


def _now_ms() -> int:
    import time

    return int(time.time() * 1000)


if __name__ == "__main__":
    main()

