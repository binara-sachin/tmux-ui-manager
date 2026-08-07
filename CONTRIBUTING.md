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
- `implementation-docs/IMPLEMENTATION.md` — the design spec this was built against: UI states,
  the drag-target table (§6.5), edge cases (§10), and what's deliberately deferred to v2 (§12).

## PR conventions

- One logical change per PR. Match the existing commit style: a short imperative summary (see
  `git log`), body only when the "why" isn't obvious from the diff.
- Any change to `src/tmux/actions.rs` or `snapshot.rs` needs a live-integration test in
  `tests/live_actions.rs`, not just a unit test — those are the two modules where real `tmux`
  version/output quirks live.
- Any change to drag behavior, rendering, or overlays needs a render-snapshot test in
  `tests/render_snapshot.rs` (ratatui `TestBackend`) — this is the regression net for flicker and
  layout bugs that unit tests can't see.
- If you touch anything in the manual acceptance script's territory (README's "Manual acceptance
  script" section), re-run it against a real tmux session before requesting review — CI can't drive
  a mouse.

## Good first issue: v2 board view

The backend/data model already carries what a board-view renderer needs — `window_layout` is
captured in the snapshot (`src/tmux/snapshot.rs`) specifically so v2 wouldn't require a backend
change (see IMPLEMENTATION.md §12). What's missing is purely a new renderer:

- Parse `#{window_layout}`'s checksum + nested `{}`/`[]` geometry string into a tree of splits.
- Render each window as a fixed-height card showing its real pane split (box-drawing borders,
  one label per tile).
- Toggle between list view and this board view with `v`. Same model, same actions, same drag state
  machine (`src/ui/drag.rs`) — only the renderer and its hit-map differ (`src/ui/columns.rs` →
  `src/ui/board.rs`).

This is scoped as additive: it shouldn't require changes to `src/tmux/` or the drag state machine,
only a new module under `src/ui/`. Good entry point if you want to work on the rendering side
without needing to understand the tmux-interaction layer first.
