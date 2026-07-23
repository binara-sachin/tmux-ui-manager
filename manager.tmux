#!/usr/bin/env bash
set -euo pipefail

CURRENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

tmux_option_or_default() {
	local option="$1"
	local default="$2"
	local value
	value="$(tmux show-option -gqv "$option")"
	if [ -z "$value" ]; then
		echo "$default"
	else
		echo "$value"
	fi
}

KEY="$(tmux_option_or_default "@manager-key" "e")"
WIDTH="$(tmux_option_or_default "@manager-width" "90%")"
HEIGHT="$(tmux_option_or_default "@manager-height" "85%")"

BINARY="$CURRENT_DIR/target/release/tmux-ui-manager"

if [ ! -x "$BINARY" ]; then
	# Don't let a failed build abort the script (set -e) before we get a chance
	# to bind the key — build.sh reports its own error via display-message, and
	# binding still succeeds so a later manual `cargo build` + config reload
	# works without re-running this file (§3.1: never fail silently).
	"$CURRENT_DIR/scripts/build.sh" || true
fi

tmux bind-key "$KEY" run-shell "WIDTH='$WIDTH' HEIGHT='$HEIGHT' '$CURRENT_DIR/scripts/launch.sh'"
