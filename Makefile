.PHONY: test test-live verify-install demo

# Fast tests: unit + parser fixtures, no tmux server required.
test:
	cargo test

# Live integration tests (§11.2): spins up real, isolated tmux servers.
# --test-threads=1 is belt-and-suspenders on top of tests/live_actions.rs's own
# Mutex — both exist because the tests mutate the process-global $TMUX env var.
test-live:
	cargo test --test live_actions -- --ignored --test-threads=1

# Clean-room TPM install verification: isolated $HOME + isolated tmux server,
# clones this plugin from GitHub exactly as a new user's TPM would, then
# smoke-tests the built binary against whatever tmux version is on PATH.
verify-install:
	scripts/verify-install.sh

# Regenerates the README demo GIF. Requires cargo (for a release build),
# tmux, and vhs (https://github.com/charmbracelet/vhs) — which itself needs
# ttyd, ffmpeg, and a Chrome/Chromium binary on PATH.
demo: target/release/tmux-ui-manager
	vhs demo.tape

target/release/tmux-ui-manager:
	cargo build --release
