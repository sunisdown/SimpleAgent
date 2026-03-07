# SimpleAgent

Python scaffold inspired by the architecture of `badlogic/pi-mono`:

- `llm` layer: provider abstraction and model response contract.
- `agent` layer: event-driven loop with tool-calling.
- `tools` layer: registry + executable tools.
- `cli` layer: minimal terminal UX.

## Quickstart

```bash
uv venv .venv
uv pip install -e .
uv run simple-agent "ls"
uv run simple-agent "read README.md"
uv run simple-agent "bash ls -la"
```

## Project Layout

```text
src/simple_agent/
  agent.py      # agent state loop
  events.py     # event model
  llm.py        # model provider interfaces + mock provider
  tools.py      # tool registry and built-in tools
  cli.py        # command-line entrypoint
```

## Notes

- Default provider is `MockProvider` to keep local setup dependency-free.
- Add real model providers by implementing `ModelProvider` in `llm.py`.
- Loop behavior follows `pi-mono` structure with `agent_start`, `turn_start`, `message_*`, `tool_execution_*`, `turn_end`, `agent_end`.
- No fixed `max_turns` limit in the loop; continuation is driven by tool calls, steering, and follow-up messages.
- Built-in tools are `ls`, `read`, and `bash`, with `pi-mono`-style argument validation and truncation metadata.
