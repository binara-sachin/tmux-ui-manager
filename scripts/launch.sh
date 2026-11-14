#!/usr/bin/env sh
set -eu

CURRENT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$CURRENT_DIR/target/release/tmux-ui-manager"

# manager.tmux swallows a failed/skipped build so binding the key never
# aborts (a later manual `cargo build` + config reload should still work).
# That means the binary can still be missing here — without this check,
# `exec ... "$BINARY"` fails with a bare "command not found" (exit 127)
# that tmux reports as a cryptic 'run-shell ...' returned 127 message.
if [ ! -x "$BINARY" ]; then
	tmux display-message "tmux-ui-manager: binary not built at $BINARY — install Rust (brew install rust, or https://rustup.rs), then run 'cargo build --release' in $CURRENT_DIR and reload tmux config"
	# Exit 0, not 1: nothing consumes launch.sh's exit code, but run-shell
	# reports any nonzero status as its own extra "'<cmd>' returned N"
	# message — which would otherwise show up right under the message above.
	exit 0
fi

# Resolve the invoking client's session here, where run-shell still has the
# client context; inside the popup `#{client_session}` can come back empty.
CLIENT_SESSION="$(tmux display-message -p '#{session_id}' 2>/dev/null || true)"
COMMAND="TMUX_UI_CLIENT_SESSION='$CLIENT_SESSION' '$BINARY'"

WIDTH="${WIDTH:-90%}"
HEIGHT="${HEIGHT:-85%}"

# §3.2/§3.3: popup-style (-s) and border-style (-S) flags require tmux >= 3.4;
# on 3.2/3.3 omit them and let the binary paint its own full-bleed background.
VERSION_STRING="$(tmux -V)"
MAJOR="$(echo "$VERSION_STRING" | sed -E 's/^tmux[[:space:]]+([0-9]+)\..*/\1/')"
MINOR="$(echo "$VERSION_STRING" | sed -E 's/^tmux[[:space:]]+[0-9]+\.([0-9]+).*/\1/')"

if [ "$MAJOR" -gt 3 ] || { [ "$MAJOR" -eq 3 ] && [ "$MINOR" -ge 4 ]; }; then
	exec tmux display-popup -E \
		-w "$WIDTH" -h "$HEIGHT" \
		-b rounded \
		-s 'bg=#1e1e2e,fg=#cdd6f4' \
		-S 'fg=#45475a' \
		-T ' tmux :: manager ' \
		"$COMMAND"
else
	exec tmux display-popup -E \
		-w "$WIDTH" -h "$HEIGHT" \
		-b rounded \
		-T ' tmux :: manager ' \
		"$COMMAND"
fi
