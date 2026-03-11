# SimpleAgent (Rust)

Rust implementation of a deterministic agent pipeline inspired by Bub/OpenClaw:

`Route -> Record(Tape) -> Tools(view) -> Context -> Model -> Process`

## Architecture

- `Router` (`src/router.rs`): routes `/` commands and `!` shell invocations directly (bypass model).
- `TapeStore` (`src/memory.rs`): append-only JSONL memory with `handoff` and search.
- `ProgressiveToolView` (`src/tool_view.rs`): lightweight tool exposure, expands on hint/use.
- `AgentLoop` (`src/core.rs`): unified loop and tool-calling orchestration (max 15 rounds).
- `MockProvider` + provider adapter API (`src/llm.rs`): normalized provider request/response shape with usage accounting, streaming events, and abort signal support.
- Tools (`src/tools.rs`): built-in `ls`, `read`, `bash`.

## Commands

- `/help`
- `/tools`
- `/tape.search <query>`
- `/handoff [name]`
- `!<shell command>`

## Quickstart

```bash
cargo run -- "ls"
cargo run -- "/tools"
cargo run -- "/handoff phase-2"
```

## Design

- Detailed Pi-inspired design doc: `docs/pi-agent-design.md`

## Notes

- Session memory is stored at `.simple_agent/<session>.tape`.
- Shell command route uses `/bin/sh -c` with a 30s timeout.


## Phase status

- Phase 1 (clarity/control): complete.
- Phase 2 (provider robustness): complete (adapter normalization, usage accounting, stream+abort interfaces, context fixtures).
