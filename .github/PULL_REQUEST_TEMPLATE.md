**What does this change and why?**


**Checklist**
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `make test` passes
- [ ] `make test-live` passes (requires `tmux` on `PATH`)
- [ ] Changes to `src/tmux/actions.rs` or `snapshot.rs` include a live-integration
      test in `tests/live_actions.rs`
- [ ] Changes to drag behavior, rendering, or overlays include a render-snapshot
      test in `tests/render_snapshot.rs`
- [ ] If this touches the manual acceptance script's territory (README), it's
      been re-run against a real tmux session — CI can't drive a mouse
