# Contributing

## Build & test

```sh
cargo build
make test        # unit + parser + render-snapshot tests, no tmux server required
make test-live   # integration tests against an isolated tmux server (`-L`), never your real one
cargo fmt && cargo clippy --all-targets -- -D warnings
```

`make test-live` spins up throwaway tmux servers under `-L manager-test`, exercises every function
in `src/tmux/actions.rs` against them, and tears them down. It requires a real `tmux` binary on
`PATH` but never touches your actual sessions.

Before opening a PR: `cargo fmt`, a clean `cargo clippy --all-targets -- -D warnings`, and both
`make test` and `make test-live` passing.

## Project layout

- `src/tmux/` — everything that talks to the real `tmux` binary: `snapshot.rs` (parses
  `list-panes -a -F ...` into the model), `actions.rs` (the mutating commands: new/rename/kill/move/
  split/join), `ids.rs` (stable `$n`/`@n`/`%n` identity types).
- `src/ui/` — ratatui rendering and interaction: `columns.rs` (Miller-column layout), `drag.rs` (the
  state machine shared by mouse drag and keyboard move-mode), `hitmap.rs` (mouse hit-testing),
  `overlays.rs` (rename/confirm/toast), `theme.rs` (Catppuccin Mocha + `@manager-color-*` overrides).
- `src/model.rs` — the in-memory tree (`Session { windows: Vec<Window> }`, etc.) that both the
  snapshot parser and the UI operate on.
- `src/input.rs` — crossterm event → app-level action mapping (keyboard and mouse).

## PR conventions

- One logical change per PR. Match the existing commit style: a short imperative summary (see
  `git log`), body only when the "why" isn't obvious from the diff.
- Any change to `src/tmux/actions.rs` or `snapshot.rs` needs a live-integration test in
  `tests/live_actions.rs`, not just a unit test.
- Any change to drag behavior, rendering, or overlays needs a render-snapshot test in
  `tests/render_snapshot.rs` (ratatui `TestBackend`).
