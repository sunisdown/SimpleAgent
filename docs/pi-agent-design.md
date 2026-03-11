# SimpleAgent Design Doc (Rewritten from “What I learned building an opinionated and minimal coding agent”)

## 1. Why this design exists

This design adapts the concrete lessons from the Pi coding agent blog into an implementation plan for **SimpleAgent**.

The core thesis is:

- minimal harnesses can perform competitively,
- context engineering and observability matter more than feature count,
- simple, explicit primitives beat hidden orchestration.

For this repo, that means we should evolve SimpleAgent without turning it into a “spaceship harness” with opaque behavior.

---

## 2. Product principles (non-negotiable)

### P1) Minimal and opinionated by default

Prefer a small, coherent feature set over many optional subsystems. If a feature is not clearly needed for daily coding tasks, do not build it.

### P2) Full observability

Every meaningful action should be visible and inspectable:

- what entered model context,
- which tools were available,
- which tools were called with what args,
- what outputs were returned,
- why the loop stopped.

### P3) Context engineering as a first-class concern

SimpleAgent should make context control explicit and user-steerable, rather than silently injecting large hidden prompts or tool payloads.

### P4) Stable behavior across releases

System prompt, tool definitions, and routing behavior must be versioned and predictable so workflows don’t break unexpectedly.

### P5) Leverage general primitives

Where possible, use `bash + files + docs` instead of heavyweight protocol-level integrations.

---

## 3. Scope and non-goals

### In scope

- deterministic loop orchestration,
- minimal toolset for coding tasks,
- inspectable tape/session format,
- model/provider abstraction,
- progressive context loading.

### Out of scope (for now)

- built-in TODO subsystem,
- built-in plan mode,
- built-in MCP integration,
- built-in background process manager,
- hidden sub-agent orchestration.

These are excluded by default to keep the core simple and observable.

---

## 4. Target architecture

SimpleAgent should remain a thin, layered system:

1. **LLM Adapter Layer**
   - Unified provider API with streaming and abort support.
   - Best-effort token/cost accounting.

2. **Agent Core Layer**
   - Turn loop (message -> model -> tools -> model ...).
   - Tool validation and execution.
   - Event stream emission.

3. **CLI/TUI Layer**
   - Session lifecycle (new/continue/branch).
   - Rendering and interaction.
   - Human-facing commands.

This aligns with Pi’s “separate provider abstraction, core loop, and UI/runtime wiring” approach.

---

## 5. Model/provider abstraction requirements

A robust coding harness must accept a multi-model reality. SimpleAgent’s provider API should support:

1. Multiple API families (OpenAI-style completions/responses, Anthropic-style messages, Google-style generative APIs).
2. Per-provider quirks normalization (roles, reasoning fields, max token fields, unsupported request keys).
3. Streaming across text and tool-call events.
4. Abort propagation end-to-end.
5. Best-effort token/cost tracking and cache-read/write accounting.
6. Context serialization/deserialization for session persistence.

### Design constraint

Treat “unified API” as intentionally leaky. Preserve raw provider metadata where useful for debugging and replay.

---

## 6. Context handoff and portability

SimpleAgent should support mid-session provider/model switching with best-effort context handoff.

### Rules

- Keep canonical session context in SimpleAgent-native schema.
- Convert provider-specific artifacts (e.g., reasoning traces) into explicit portable content blocks.
- Preserve source-provider metadata when transforming context.
- Persist enough information to replay/continue with a different provider later.

### Expected behavior

Switching model/provider may reduce fidelity for provider-specific semantics, but conversation continuity must remain functional and inspectable.

---

## 7. System prompt strategy

Use a **short, stable system prompt** plus project instructions (e.g., AGENTS.md).

### Prompt policy

- Keep core prompt under strict size budget (target: <1k tokens including tool defs where possible).
- Prefer behavior expressed through tools and process constraints over verbose prose rules.
- Version prompt text; emit prompt version into tape metadata per turn.

### Why

Modern coding-capable models already understand coding-agent patterns; oversized prompts add cost and hidden variability.

---

## 8. Tool philosophy and minimum viable toolset

Default coding mode should expose only a compact set of high-value tools:

1. `read`
2. `write`
3. `edit`
4. `bash`

Optional read-only profiles can swap/limit tools (`ls`, `find`, `grep`, `read`) for restricted exploration.

### Tool contracts

- Typed argument validation (schema-driven).
- Clear errors on validation failure.
- Structured outputs split into:
  - **LLM-facing output** (concise text/JSON for reasoning),
  - **UI-facing details** (rich structured payload, optional attachments).

This split avoids forcing UI parsing from brittle textual tool output.

---

## 9. Tool-call UX: streaming and partial parsing

To improve usability during long tool argument generation:

- stream tool-call deltas when provider supports it,
- progressively parse partial JSON arguments,
- surface partial intent in UI (e.g., a diff being assembled).

This is especially useful for edit/write-heavy coding tasks.

---

## 10. Agent loop design

Keep the loop simple:

1. Accept user message.
2. Generate assistant output.
3. If no tool calls -> finalize.
4. Validate and execute tool calls.
5. Append results/events.
6. Repeat.

### Additions to current loop

- explicit abort support (user cancellation),
- optional message queue injection between loop iterations,
- richer event taxonomy for UI/reactive integrations.

### On step limits

Pi argues hard limits are often less valuable than expected. For SimpleAgent, keep pragmatic safeguards but make them explicit, configurable, and observable.

---

## 11. Safety model: “YOLO reality” with explicit operational modes

The blog’s key security argument: once an agent can read files, execute commands, and access network, hard guarantees are weak.

SimpleAgent should acknowledge this reality while offering explicit modes:

1. **YOLO mode (default for local power users)**
   - no per-action prompts,
   - full user-level privileges in workspace context.

2. **Constrained mode (optional)**
   - restricted tool allowlist,
   - optional path/network/time limits.

Important: market constrained mode as risk reduction, not full security.

---

## 12. Planning and task tracking philosophy

Do not add built-in plan/TODO engines.

Use file-based state instead:

- `TODO.md` for checklists,
- `PLAN.md` for long-running design/execution plans.

Advantages:

- explicit and versionable,
- cross-session continuity,
- human-editable outside agent runtime,
- zero hidden planner state.

---

## 13. No built-in MCP assumption

Default position: do not make MCP a core dependency.

Rationale:

- large always-in-context tool manifests are token-expensive,
- many tasks can be handled by CLI tools + READMEs loaded on demand.

Design pattern:

- treat external capabilities as executable CLIs with discoverable docs,
- let agent read docs only when needed (progressive disclosure),
- invoke through `bash`.

MCP can remain an optional adapter layer later, not foundational.

---

## 14. Background processes and long-running tasks

No dedicated background process subsystem initially.

Preferred strategy:

- synchronous `bash` in core,
- rely on `tmux` for long-lived processes and interactive debugging,
- keep observability in normal terminal workflows.

If future background support is added, it must include process listing, output retrieval, input injection, and cleanup guarantees.

---

## 15. Sub-agent stance

No first-class hidden sub-agent primitive in core.

If needed, allow explicit self-invocation via CLI command (`bash` spawning a separate SimpleAgent session), so behavior remains auditable and user-controlled.

Guideline:

- use separate sessions for heavy context-gathering,
- materialize findings into artifacts/files,
- start implementation in fresh context with those artifacts.

---

## 16. Observability and session format

### Session artifact requirements

Store append-only structured events/messages with:

- timestamp,
- turn id,
- prompt version,
- visible toolset for that turn,
- model/provider id,
- tool call + args + result status,
- stop reason,
- token/cost counters (best effort).

### Must-have commands

- `/trace [turn]`: inspect turn-level event flow,
- `/tools`: show currently exposed tools,
- `/handoff [name]`: materialize checkpoint,
- `/tape.search <query>`: fast retrieval.

---

## 17. UX: terminal-first, low-flicker, high signal

SimpleAgent should remain terminal-first and linear (chat + tool logs), favoring scrollback-native workflows over full-screen complexity by default.

If/when richer TUI is built:

- prefer differential updates and cached rendering,
- optimize for readability and minimal flicker,
- never hide core execution details from user.

---

## 18. Benchmarking and evaluation framework

Measure whether minimalism works rather than assuming it.

### Quality metrics

- task success rate (e.g., benchmark suites, real tasks),
- regression rate after changes,
- median turns per successful task,
- tool-call error frequency,
- context/token usage per solved task.

### Stability metrics

- behavior drift across releases (prompt/tool changes),
- provider parity for core flows,
- abort reliability,
- replay fidelity from saved sessions.

---

## 19. Implementation roadmap for SimpleAgent

### Phase 1 — Rewrite for clarity and control

1. Freeze and version a minimal system prompt.
2. Expand tool model to typed schemas and structured split outputs.
3. Emit richer structured tape events.
4. Add explicit prompt/toolset/version metadata per turn.

### Phase 2 — Provider robustness

1. Introduce provider adapters and quirks normalization.
2. Add streaming + abort support in provider interface.
3. Add best-effort token/cost accounting.
4. Add context serialize/deserialize test fixtures.

### Phase 3 — Workflow ergonomics

1. Add explicit mode profiles (`yolo`, `readonly`, `custom`).
2. Add file-first planning workflow docs (`PLAN.md`, `TODO.md`).
3. Improve `/trace` and handoff UX.
4. Optional tmux helper docs/commands.

### Phase 4 — Advanced (only if needed)

1. Tool result streaming.
2. Partial tool-argument parsing events.
3. Optional MCP adapter (off by default) if concrete demand exists.

---

## 20. Acceptance criteria

A release aligned with this design should satisfy:

1. Users can understand every action taken by the agent from session artifacts.
2. Prompt/tool/context inputs are explicit and inspectable.
3. Core coding tasks are solvable with minimal default tools.
4. Provider/model switching works with best-effort continuity.
5. Abort and replay are reliable enough for real daily use.
6. Feature growth remains disciplined and does not compromise predictability.

---

## 21. Practical defaults for this repository

Given current SimpleAgent implementation, immediate defaults should be:

- keep deterministic routing (`/` and `!`) as-is,
- keep compact loop architecture,
- prioritize structured observability before adding new major features,
- add capabilities only when they directly improve coding throughput.

This keeps SimpleAgent true to the Pi lesson: **minimal, inspectable, effective**.
