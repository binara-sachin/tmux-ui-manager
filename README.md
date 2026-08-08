# tmux-ui-manager

[![CI](https://github.com/binara-sachin/tmux-ui-manager/actions/workflows/ci.yml/badge.svg)](https://github.com/binara-sachin/tmux-ui-manager/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

An interactive, mouse-capable TUI for managing tmux sessions, windows, and panes — like `prefix+s` / `prefix+w`, but with full edit capabilities: rearrange, rename, kill, create, and drag-and-drop windows/panes between sessions.

Three linked Miller columns (**SESSIONS → WINDOWS → PANES**), Catppuccin Mocha themed, keyboard- and mouse-driven, opened as a tmux popup.

![demo](demo.gif)

## Requirements

- tmux ≥ 3.2 (popup styling and a few niceties need ≥ 3.4 — the plugin detects this and adapts automatically)
- Rust toolchain (only needed to build the binary — see install below)
- [TPM](https://github.com/tmux-plugins/tpm) (or manual install)

## Install

### Via TPM

Add to `~/.tmux.conf`

```tmux
set -g @plugin 'binara-sachin/tmux-ui-manager'
```

Then `prefix + I` to install. TPM builds the binary automatically on first install (via `scripts/build.sh`); if `cargo` isn't on your `PATH`, tmux will show a message telling you to install Rust first (`brew install rust`, or [rustup.rs](https://rustup.rs)).

### Manual

```sh
git clone binara-sachin/tmux-ui-manager ~/.tmux/plugins/tmux-ui-manager
echo "run-shell ~/.tmux/plugins/tmux-ui-manager/manager.tmux" >> ~/.tmux.conf
tmux source ~/.tmux.conf
```

Or, if you already have this checked out locally (no separate clone needed), just point `run-shell` at wherever it lives on disk:

```tmux
run-shell /path/to/tmux-ui-manager/manager.tmux
```

## Usage

Default binding: `prefix + e` opens the popup. (Chosen so it never clobbers tmux's own `s`/`w` session/window pickers — see Options below if you'd rather bind something else.)

| Key | Action |
|---|---|
| ↑/↓, j/k | move selection in the focused column |
| ←/→, h/l, Tab/Shift-Tab | move focus between columns |
| Enter | attach/jump to the selected session/window/pane (closes the popup) |
| n | new: session column → new session · window column → new window · pane column → split pane |
| r | rename selected (pre-filled input) |
| x | kill selected (confirm overlay, unless `@manager-confirm-kill` is `off`) |
| z | zoom toggle (panes only) |
| Space or m | pick up the selected window/pane and enter move-mode |
| Esc, q | cancel the current overlay/drag/move-mode; quit if already idle |
| g / G | jump to top/bottom of the focused column |

Mouse: hover to highlight, click to select/focus, double-click to attach/jump, scroll wheel to scroll a column, click a `+ ...` row to create, drag a window/pane to move it (release on an invalid target, or off the columns entirely, cancels — nothing commits until you land somewhere valid).

## Options

All optional; set with `set -g <option> <value>` in `~/.tmux.conf`.

| Option | Default | Meaning |
|---|---|---|
| `@manager-key` | `e` | prefix key binding |
| `@manager-width` / `@manager-height` | `90%` / `85%` | popup size |
| `@manager-color-bg` | `#1e1e2e` | background |
| `@manager-color-text` | `#cdd6f4` | primary text |
| `@manager-color-meta` | `#a6adc8` | right-aligned/secondary text |
| `@manager-color-active` | `#a6e3a1` | attached-session/active-pane dot |
| `@manager-color-accent` | `#89b4fa` | drag/drop highlight |
| `@manager-color-danger` | `#f38ba8` | destructive actions, error toasts |
| `@manager-color-border` | `#45475a` | panel borders |
| `@manager-color-panel-title` | `#6c7086` | column title text |
| `@manager-color-selection-bg` | `#313244` | focused selection background |
| `@manager-confirm-kill` | `on` | set to `off` to skip the kill confirmation (power users only — there is no undo) |

Colors are literal `#rrggbb` hex only (no named colors) and apply on top of the built-in Catppuccin Mocha defaults — set only the ones you want to change.

## Manual acceptance script

Automated tests (`make test`, `make test-live`) cover the parser, every `tmux::actions` function against an isolated test server, the drag state machine, and render-snapshot regressions. They can't drive a real mouse or eyeball the popup, so this script is the final human check before tagging a release — walk through it once against a real tmux session (ideally on the target setup: macOS/Ghostty/tmux ≥ 3.4).

Run this against a throwaway tmux server, not your daily-driver one — steps below kill sessions/windows/panes for real. A bare `-f /dev/null` server has no config, so `manager.tmux` needs sourcing explicitly, and its *default* prefix is plain tmux's own (`C-b`), not whatever you've bound in your real `~/.tmux.conf`:

```sh
tmux -L manager-acceptance -f /dev/null new-session -d -s main -n editor -c ~
tmux -L manager-acceptance split-window -d -t main:editor -c ~
tmux -L manager-acceptance new-window -d -t main: -n logs -c ~
tmux -L manager-acceptance new-session -d -s scratch -c ~
tmux -L manager-acceptance run-shell /path/to/tmux-ui-manager/manager.tmux
tmux -L manager-acceptance attach -t main
```

(From inside that session, the popup opens with `C-b e` — not `prefix + e` as bound in your real config, since this is a bare server. If you'd rather test against your actual `~/.tmux.conf` and its real prefix, skip the isolated server and just use a couple of distinctly-named throwaway sessions in your normal tmux instead — either way, verify the *binding* actually fires: a keybinding that silently doesn't fire is exactly the kind of thing worth catching here, not just testing the binary directly.)

1. **Open + layout.** Press the prefix, then `e` (see above). Confirm: three columns, Catppuccin Mocha colors, header shows correct session/window/pane counts, footer shows keyboard hints. Resize the terminal below 70×15 — confirm a centered "window too small" notice replaces the layout, then resize back and confirm normal rendering resumes.
2. **Navigate + select, keyboard and mouse.** Use `hjkl`/arrows/Tab to move focus and selection; confirm selecting a session filters its windows, and selecting a window filters its panes (Miller-column cascade). Then click a different session's row with the mouse — same cascade should happen, plus a subtle hover highlight as you move the pointer before clicking.
3. **Create.** Press `n` on the sessions column → type a name → Enter → new session appears. Press `n` on the windows column → new window appears immediately (no prompt). Press `n` on the panes column → pane splits immediately. Repeat window/session creation once each by clicking the `+ new window` / `+ new session` rows instead of pressing `n`.
4. **Rename.** Press `r` on a session, window, and pane — each opens a pre-filled input; edit and confirm each. Try renaming a session to an existing name — confirm the inline validation error.
5. **Kill + confirm.** Press `x` on a pane — confirm overlay appears with `[y]es`/`[n]o`; click `[n]o` with the mouse — nothing happens. Press `x` again, this time press `n` on the keyboard — same result. Press `x`, then `y` — pane is killed. Kill a whole session that's attached — confirm the wording explicitly mentions the client will jump/detach.
6. **Zoom.** On a pane, press `z` — window row shows the `Z` suffix. Press `z` again — it clears.
7. **Window → another session (keyboard).** Focus windows column, `Space` on a window to pick it up, `h`/`←` to move the target cursor to the sessions column, navigate to a *different* session's row — footer reads `drop: move window '<name>' → session '<name>'`. Press `Enter` — window moves; Miller columns refresh to show it under the new session.
8. **Window reorder (mouse drag).** Pick up a window by pressing the mouse button down on its row and dragging (don't release) up/down past sibling windows in the same session — confirm the blue insertion line appears between rows and the footer updates live as you move; release over a real gap — window reorders. Try reordering into position it's already in — confirm it does *not* highlight and the footer reads "no-op".
9. **Window → new session (either input method).** Pick up a window, move/drag it onto the `+ new session` row, release/commit — an input overlay opens pre-filled with the window's name; confirm — a brand-new session now holds exactly that window (no leftover placeholder session).
10. **Pane → window / pane → pane (mouse drag).** Pick up a pane by dragging it onto a *different* window's row — it joins as a new split there. Pick up another pane and drag it onto a specific pane row (not a whole window) — it splits that exact pane. Confirm dropping a pane onto its own current window is a no-op (not highlighted).
11. **Pane → new window / new session (keyboard).** `Space` on a pane, move the cursor to the windows column's `+ new window` row, commit — pane breaks out into a new window in the currently-viewed session. Pick up another pane, move it to the `+ new session` row, commit — same prefilled-name flow as step 9, but for a pane.
12. **Cancel paths + auto-scroll + attach.** Start a drag and press `Esc` — silently cancels, selection reverts. Start a drag (mouse) and release outside all three columns (e.g. over the footer) — also cancels, nothing commits. If you have a session with enough windows to overflow the column, drag a window there and hold the pointer at the bottom edge — it auto-scrolls every ~150 ms; confirm the `…` overflow indicator only shows while there's actually more below. Finally, double-click (or click + Enter) a session — the popup closes and your client attaches/jumps to it.

If every step matches, tag `v0.1.0`.

## Development

```sh
make test            # unit + parser + render-snapshot tests
make test-live        # integration tests against an isolated tmux server (`-L`), never the real one
make verify-install   # clean-room TPM install + popup smoke test in an isolated $HOME
make demo             # regenerate the README demo GIF (needs vhs, ttyd, ffmpeg, a Chromium/Chrome binary)
cargo fmt && cargo clippy --all-targets -- -D warnings
```
