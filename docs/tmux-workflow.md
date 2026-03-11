# tmux Workflow for Long-Running Tasks

SimpleAgent stays synchronous by design. For long-lived processes (servers, watchers, tailing logs), use `tmux`.

## Quick start

```bash
tmux new -s simpleagent
```

Inside tmux:

- split panes: `Ctrl-b %` (vertical) / `Ctrl-b "` (horizontal)
- switch panes: `Ctrl-b o`
- create window: `Ctrl-b c`
- rename window: `Ctrl-b ,`

## Suggested layout

- Window `agent`: run SimpleAgent commands.
- Window `app`: run local app/server (`npm run dev`, `cargo watch`, etc.).
- Window `logs`: tail logs and diagnostics.

## Session persistence

Detach without stopping processes:

```bash
# inside tmux
Ctrl-b d
```

Re-attach later:

```bash
tmux attach -t simpleagent
```

## Practical handoff pattern

1. keep long-running process in `app` window,
2. interact with SimpleAgent in `agent` window,
3. when pausing work, run `/handoff <name>` and note current tmux window state in `TODO.md`.
