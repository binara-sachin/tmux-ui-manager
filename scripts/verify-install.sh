#!/usr/bin/env sh
# Clean-room verification of the TPM install path (README "Via TPM" section).
#
# Spins up an isolated $HOME + isolated tmux server (-L), installs TPM and
# this plugin exactly as a new user would (clone + prefix+I equivalent), then
# smoke-tests that the popup actually renders instead of crashing on the
# real, freshly-built binary. Safe to run repeatedly; each run starts from a
# blank scratch dir and tears its tmux server down on exit.
#
# Requires: tmux, cargo, git on PATH.
set -eu

SOCKET="tmux-ui-manager-verify-$$"
SCRATCH="$(mktemp -d)"
trap 'tmux -L "$SOCKET" kill-server >/dev/null 2>&1 || true; rm -rf "$SCRATCH"' EXIT

# Overriding $HOME below isolates TPM/plugin state, but a rustup-installed
# cargo is a shim that looks up its toolchain via $RUSTUP_HOME (and
# $CARGO_HOME), both of which default from $HOME — so without this, `cargo
# build` inside the isolated server fails to find any toolchain at all. Only
# matters for the one command that starts the server (new-session): the
# server inherits that environment for every later run-shell/pane spawn.
REAL_RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
REAL_CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"

echo "== scratch HOME: $SCRATCH"
mkdir -p "$SCRATCH/.tmux/plugins"

echo "== cloning TPM"
git clone --depth 1 -q https://github.com/tmux-plugins/tpm "$SCRATCH/.tmux/plugins/tpm"

cat > "$SCRATCH/.tmux.conf" <<EOF
set -g @plugin 'tmux-plugins/tpm'
set -g @plugin 'binara-sachin/tmux-ui-manager'
run '~/.tmux/plugins/tpm/tpm'
EOF

echo "== starting isolated tmux server ($SOCKET)"
HOME="$SCRATCH" RUSTUP_HOME="$REAL_RUSTUP_HOME" CARGO_HOME="$REAL_CARGO_HOME" \
	tmux -L "$SOCKET" -f "$SCRATCH/.tmux.conf" new-session -d -s verify -x 200 -y 50

echo "== running TPM's install script directly (equivalent of prefix+I)"
HOME="$SCRATCH" tmux -L "$SOCKET" run-shell "$SCRATCH/.tmux/plugins/tpm/bindings/install_plugins"

PLUGIN_DIR="$SCRATCH/.tmux/plugins/tmux-ui-manager"
BINARY="$PLUGIN_DIR/target/release/tmux-ui-manager"

if [ ! -x "$BINARY" ]; then
	echo "FAIL: $BINARY was not built by the install step" >&2
	exit 1
fi
echo "== binary built: $BINARY"

echo "== confirming prefix+e binding was wired up automatically"
if ! HOME="$SCRATCH" tmux -L "$SOCKET" list-keys -T prefix e | grep -q "$PLUGIN_DIR/scripts/launch.sh"; then
	echo "FAIL: prefix+e is not bound to launch.sh after install" >&2
	exit 1
fi

echo "== smoke-testing the binary against this tmux version ($(tmux -V))"
HOME="$SCRATCH" tmux -L "$SOCKET" send-keys -t verify "clear; $BINARY" Enter
sleep 1
OUTPUT="$(HOME="$SCRATCH" tmux -L "$SOCKET" capture-pane -p -t verify)"
HOME="$SCRATCH" tmux -L "$SOCKET" send-keys -t verify q

if echo "$OUTPUT" | grep -q "malformed list-panes output"; then
	echo "FAIL: binary crashed parsing list-panes output on $(tmux -V)" >&2
	echo "$OUTPUT" >&2
	exit 1
fi
if ! echo "$OUTPUT" | grep -q "SESSIONS"; then
	echo "FAIL: popup did not render the expected SESSIONS column" >&2
	echo "$OUTPUT" >&2
	exit 1
fi

echo "== PASS: clean TPM install + popup render succeeded on $(tmux -V)"
