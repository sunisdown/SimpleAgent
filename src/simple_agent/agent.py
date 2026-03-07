from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Iterable

from .events import Event, make_event
from .llm import Message, ModelProvider
from .tools import AgentToolResult, ToolRegistry


GetMessagesFn = Callable[[], list[Message]]
ConvertToLlmFn = Callable[[list[Message]], list[Message]]
TransformContextFn = Callable[[list[Message]], list[Message]]


@dataclass(slots=True)
class AgentContext:
    system_prompt: str
    messages: list[Message]
    tools: ToolRegistry


@dataclass(slots=True)
class AgentLoopConfig:
    provider: ModelProvider
    convert_to_llm: ConvertToLlmFn | None = None
    transform_context: TransformContextFn | None = None
    get_steering_messages: GetMessagesFn | None = None
    get_follow_up_messages: GetMessagesFn | None = None


def agent_loop(prompts: list[Message], context: AgentContext, config: AgentLoopConfig) -> Iterable[Event]:
    new_messages: list[Message] = [*prompts]
    current_context = AgentContext(
        system_prompt=context.system_prompt,
        messages=[*context.messages, *prompts],
        tools=context.tools,
    )

    yield make_event("agent_start")
    yield make_event("turn_start")
    for prompt in prompts:
        yield make_event("message_start", message=prompt)
        yield make_event("message_end", message=prompt)

    yield from _run_loop(current_context, new_messages, config)


def agent_loop_continue(context: AgentContext, config: AgentLoopConfig) -> Iterable[Event]:
    if not context.messages:
        raise ValueError("Cannot continue: no messages in context")
    if context.messages[-1].get("role") == "assistant":
        raise ValueError("Cannot continue from message role: assistant")

    new_messages: list[Message] = []
    current_context = AgentContext(
        system_prompt=context.system_prompt,
        messages=[*context.messages],
        tools=context.tools,
    )

    yield make_event("agent_start")
    yield make_event("turn_start")
    yield from _run_loop(current_context, new_messages, config)


def _run_loop(context: AgentContext, new_messages: list[Message], config: AgentLoopConfig) -> Iterable[Event]:
    first_turn = True
    pending_messages = _get_messages(config.get_steering_messages)

    while True:
        has_more_tool_calls = True
        steering_after_tools: list[Message] | None = None

        while has_more_tool_calls or pending_messages:
            if not first_turn:
                yield make_event("turn_start")
            else:
                first_turn = False

            if pending_messages:
                for message in pending_messages:
                    yield make_event("message_start", message=message)
                    yield make_event("message_end", message=message)
                    context.messages.append(message)
                    new_messages.append(message)
                pending_messages = []

            assistant_message, stream_events = _stream_assistant_response(context, config)
            for ev in stream_events:
                yield ev
            new_messages.append(assistant_message)

            if assistant_message.get("stopReason") in {"error", "aborted"}:
                yield make_event("turn_end", message=assistant_message, toolResults=[])
                yield make_event("agent_end", messages=new_messages)
                return

            tool_calls = [c for c in assistant_message.get("content", []) if c.get("type") == "toolCall"]
            has_more_tool_calls = len(tool_calls) > 0

            tool_results: list[Message] = []
            if has_more_tool_calls:
                tool_results, steering_after_tools, exec_events = _execute_tool_calls(
                    context.tools,
                    assistant_message,
                    config.get_steering_messages,
                )
                for ev in exec_events:
                    yield ev
                for result in tool_results:
                    context.messages.append(result)
                    new_messages.append(result)

            yield make_event("turn_end", message=assistant_message, toolResults=tool_results)

            if steering_after_tools:
                pending_messages = steering_after_tools
            else:
                pending_messages = _get_messages(config.get_steering_messages)

        follow_up_messages = _get_messages(config.get_follow_up_messages)
        if follow_up_messages:
            pending_messages = follow_up_messages
            continue
        break

    yield make_event("agent_end", messages=new_messages)


def _stream_assistant_response(context: AgentContext, config: AgentLoopConfig) -> tuple[Message, list[Event]]:
    messages = context.messages
    if config.transform_context:
        messages = config.transform_context(messages)

    llm_messages = config.convert_to_llm(messages) if config.convert_to_llm else messages
    assistant = config.provider.generate(llm_messages, context.tools.specs())
    context.messages.append(assistant)

    events = [
        make_event("message_start", message=assistant),
        make_event("message_end", message=assistant),
    ]
    return assistant, events


def _execute_tool_calls(
    registry: ToolRegistry,
    assistant_message: Message,
    get_steering_messages: GetMessagesFn | None,
) -> tuple[list[Message], list[Message] | None, list[Event]]:
    tool_calls = [c for c in assistant_message.get("content", []) if c.get("type") == "toolCall"]
    results: list[Message] = []
    events: list[Event] = []
    steering_messages: list[Message] | None = None

    for idx, tool_call in enumerate(tool_calls):
        call_id = str(tool_call.get("id", f"call_{idx + 1}"))
        tool_name = str(tool_call.get("name", "unknown"))
        args = tool_call.get("arguments", {})
        events.append(make_event("tool_execution_start", toolCallId=call_id, toolName=tool_name, args=args))

        is_error = False
        try:
            tool = registry.find(tool_name)
            if tool is None:
                raise ValueError(f"Tool {tool_name} not found")
            validated_args = registry.validate_tool_arguments(tool.spec, args)
            partials: list[dict[str, Any]] = []

            def on_update(partial: dict[str, Any]) -> None:
                partials.append(partial)
                events.append(
                    make_event(
                        "tool_execution_update",
                        toolCallId=call_id,
                        toolName=tool_name,
                        args=args,
                        partialResult=partial,
                    )
                )

            result = tool.execute(call_id, validated_args, on_update)
            _ = partials
        except Exception as exc:
            is_error = True
            result = AgentToolResult(content=[{"type": "text", "text": str(exc)}], details={})

        events.append(
            make_event(
                "tool_execution_end",
                toolCallId=call_id,
                toolName=tool_name,
                result={"content": result.content, "details": result.details},
                isError=is_error,
            )
        )

        tool_result_message = {
            "role": "toolResult",
            "toolCallId": call_id,
            "toolName": tool_name,
            "content": result.content,
            "details": result.details,
            "isError": is_error,
            "timestamp": _now_ms(),
        }
        results.append(tool_result_message)
        events.append(make_event("message_start", message=tool_result_message))
        events.append(make_event("message_end", message=tool_result_message))

        if get_steering_messages:
            steering = get_steering_messages()
            if steering:
                steering_messages = steering
                for skipped in tool_calls[idx + 1 :]:
                    skipped_id = str(skipped.get("id", ""))
                    skipped_name = str(skipped.get("name", "unknown"))
                    skipped_result = {
                        "content": [{"type": "text", "text": "Skipped due to queued user message."}],
                        "details": {},
                    }
                    events.append(
                        make_event(
                            "tool_execution_start",
                            toolCallId=skipped_id,
                            toolName=skipped_name,
                            args=skipped.get("arguments", {}),
                        )
                    )
                    events.append(
                        make_event(
                            "tool_execution_end",
                            toolCallId=skipped_id,
                            toolName=skipped_name,
                            result=skipped_result,
                            isError=True,
                        )
                    )
                    skipped_message = {
                        "role": "toolResult",
                        "toolCallId": skipped_id,
                        "toolName": skipped_name,
                        "content": skipped_result["content"],
                        "details": {},
                        "isError": True,
                        "timestamp": _now_ms(),
                    }
                    results.append(skipped_message)
                    events.append(make_event("message_start", message=skipped_message))
                    events.append(make_event("message_end", message=skipped_message))
                break

    return results, steering_messages, events


def _get_messages(fn: GetMessagesFn | None) -> list[Message]:
    if not fn:
        return []
    messages = fn()
    return messages if isinstance(messages, list) else []


def _now_ms() -> int:
    import time

    return int(time.time() * 1000)

