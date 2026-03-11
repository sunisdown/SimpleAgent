# SimpleAgent (Rust)

A minimal, opinionated coding agent harness rebuilt from the design doc in `docs/pi-agent-design.md`.

## Architecture

Simple layered architecture:

1. **LLM Adapter Layer** (`src/llm.rs`)
   - Unified request/response model.
   - Streaming events (`text delta`, `tool call delta`, `done`).
   - Context serialize/deserialize helpers.

2. **Agent Core Layer** (`src/core.rs`)
   - Deterministic turn loop.
   - Tool-call execution and validation.
   - Structured tape event emission.

3. **CLI Layer** (`src/main.rs`)
   - Session setup and runtime profile selection.
   - Prompt dispatch.

Supporting modules:
- `src/tools.rs`: minimal coding tools (`read`, `write`, `edit`, `bash`) with typed args.
- `src/memory.rs`: append-only JSONL tape store.
- `src/runtime.rs`: runtime profiles (`yolo`, `readonly`, `custom`).
- `src/agent_config.rs`: versioned system prompt + loop limits.
- `src/router.rs`: slash-command and shell-route parsing.
- `src/tool_view.rs`: progressive tool visibility.

## Commands

- `/help`
- `/tools`
- `/trace`
- `/handoff <name>`
- `!<shell command>` (if profile allows)

## Run

```bash
cargo run -- --profile yolo "read README.md"
cargo run -- --profile readonly "/tools"
cargo run -- --profile custom --tools read,edit "read src/main.rs"
```
