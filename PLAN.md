# Design Doc Execution Plan

This plan breaks `docs/pi-agent-design.md` into implementation phases and defines concrete deliverables for each.

## Phase 1 — Clarity and Control (execute now)

### Deliverables
- Freeze a short system prompt and version it (e.g., `v1`) in code.
- Add typed tool argument schemas to tool definitions.
- Split tool execution outputs into:
  - model-facing output (`llm_output`),
  - UI-facing structured details (`ui_details`).
- Emit structured tape events with per-turn metadata:
  - prompt version,
  - visible toolset,
  - round and stop reason,
  - tool-call status and args.

### Validation
- `cargo fmt -- --check`
- `cargo test`
- Manual run demonstrates event metadata in tape output.

## Phase 2 — Provider Robustness

### Deliverables
- Extract provider adapters for different provider API shapes.
- Normalize provider-specific request/response quirks.
- Add streaming interface and cancellation propagation.
- Add token/cost accounting (best effort) into event stream.
- Add context serialize/deserialize fixtures for replay.

## Phase 3 — Workflow Ergonomics

### Deliverables
- Add explicit runtime profiles (`yolo`, `readonly`, `custom`).
- Document file-first planning workflow (`PLAN.md`, `TODO.md`).
- Improve trace/handoff UX commands and output readability.
- Add tmux workflow documentation for long-running processes.

## Phase 4 — Optional Advanced

### Deliverables
- Stream tool results in chunks.
- Emit partial tool-argument parse events.
- Add optional MCP adapter (off by default) when demand is proven.
