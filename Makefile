.PHONY: test test-live

# Fast tests: unit + parser fixtures, no tmux server required.
test:
	cargo test

# Live integration tests (§11.2): spins up real, isolated tmux servers.
# --test-threads=1 is belt-and-suspenders on top of tests/live_actions.rs's own
# Mutex — both exist because the tests mutate the process-global $TMUX env var.
test-live:
	cargo test --test live_actions -- --ignored --test-threads=1
