#!/usr/bin/env sh
set -eu

CURRENT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$CURRENT_DIR/target/release/tmux-ui-manager"

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
		"$BINARY"
else
	exec tmux display-popup -E \
		-w "$WIDTH" -h "$HEIGHT" \
		-b rounded \
		-T ' tmux :: manager ' \
		"$BINARY"
fi
