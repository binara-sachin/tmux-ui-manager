#!/usr/bin/env sh
set -eu

CURRENT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
	tmux display-message "tmux-ui-manager: cargo not found — install Rust (brew install rust, or https://rustup.rs) then reload tmux config"
	exit 1
fi

cd "$CURRENT_DIR"
if ! cargo build --release; then
	tmux display-message "tmux-ui-manager: cargo build --release failed — see terminal output"
	exit 1
fi
