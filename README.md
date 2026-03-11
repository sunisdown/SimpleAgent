# SimpleAgent (Rust)

Rust implementation of a deterministic agent pipeline inspired by Bub/OpenClaw:

`Route -> Record(Tape) -> Tools(view) -> Context -> Model -> Process`

## Architecture

- `Router` (`src/router.rs`): routes `/` commands and `!` shell invocations directly (bypass model).
- `TapeStore` (`src/memory.rs`): append-only JSONL memory with `handoff`, `trace`, and search.
- `ProgressiveToolView` (`src/tool_view.rs`): lightweight tool exposure, expands on hint/use.
- `AgentLoop` (`src/core.rs`): unified loop and tool-calling orchestration (max 15 rounds).
- `MockProvider` + provider adapter API (`src/llm.rs`): normalized provider request/response shape with usage accounting, streaming events, and abort signal support.
- Tools (`src/tools.rs`): built-in `ls`, `read`, `bash`.
- Runtime profiles (`src/runtime.rs`): `yolo`, `readonly`, and `custom`.

## Commands

- `/help`
- `/tools`
- `/trace [turn]`
- `/tape.search <query>`
- `/handoff [name]`
- `/handoff.list`
- `!<shell command>`

## Runtime profiles

- `yolo` (default): full toolset (`ls`, `read`, `bash`) and shell route enabled.
- `readonly`: read-only exploration toolset (`ls`, `read`) and shell route disabled.
- `custom`: user-provided allowlist via `--tools`.

Examples:

```bash
cargo run -- --profile yolo "ls"
cargo run -- --profile readonly "/tools"
cargo run -- --profile custom --tools ls,read "read README.md"
```

## File-first planning workflow

Use repo files as explicit planning state:

- `PLAN.md`: long-running design and phase execution plans.
- `TODO.md`: short checklists and current execution queue.

Recommended loop:

1. update `PLAN.md` with phase scope,
2. maintain actionable items in `TODO.md`,
3. implement and validate,
4. checkpoint with `/handoff <name>`.

## Long-running process workflow (tmux)

See `docs/tmux-workflow.md` for a terminal-first workflow for servers, watchers, and debugging sessions.

## Notes

- Session memory is stored at `.simple_agent/<session>.tape`.
- Shell command route uses `/bin/sh -c` with a 30s timeout.

## Design

- Detailed Pi-inspired design doc: `docs/pi-agent-design.md`
