//! Live integration tests (§11.2): exercise every function in `tmux/actions.rs`
//! against a real, isolated tmux server and assert the resulting tree via a
//! fresh snapshot.
//!
//! `actions.rs` deliberately never passes `-L`/`-S` — production code relies on
//! the popup inheriting `$TMUX` from its enclosing server (see the ADR in
//! IMPLEMENTATION.md §1.4). To point that same bare `tmux <cmd>` invocation at an
//! isolated test server instead of the user's real one, we set the `TMUX` env var
//! to `<isolated_socket_path>,0,0` for the duration of each test (verified
//! empirically: only the socket_path component before the first comma matters —
//! a bogus pid/session-id suffix still routes correctly). Env vars are
//! process-global, so all tests here serialize on `SERIAL` rather than relying on
//! `cargo test`'s default parallel threads.
//!
//! Gated behind `#[ignore]` + `make test-live` so a tmux-less CI still passes;
//! run locally with tmux installed.

use std::process::Command;
use std::sync::Mutex;

use tmux_ui_manager::tmux::actions;
use tmux_ui_manager::tmux::ids::{PaneId, SessionId, WindowId};
use tmux_ui_manager::tmux::snapshot::take_snapshot;

static SERIAL: Mutex<()> = Mutex::new(());

struct TestServer {
    socket: String,
    prev_tmux: Option<String>,
}

impl TestServer {
    /// Starts a fresh isolated server (`-f /dev/null`, no user config) with one
    /// base session `t1`, and points the ambient `$TMUX` at it.
    fn start(name: &str) -> Self {
        // Acquire once per test via `SERIAL.lock()` at the call site would be
        // cleaner, but holding the guard here for the object's lifetime is what
        // actually prevents cross-test env-var races.
        let socket = format!("manager-live-{name}");

        let _ = Command::new("tmux")
            .args(["-L", &socket, "kill-server"])
            .output();

        let status = Command::new("tmux")
            .args([
                "-L",
                &socket,
                "-f",
                "/dev/null",
                "new-session",
                "-d",
                "-s",
                "t1",
                "-c",
                "/tmp",
            ])
            .status()
            .expect("failed to spawn isolated tmux server");
        assert!(status.success(), "failed to create base session t1");

        let socket_path_output = Command::new("tmux")
            .args(["-L", &socket, "display-message", "-p", "#{socket_path}"])
            .output()
            .expect("failed to query socket_path");
        assert!(socket_path_output.status.success());
        let socket_path = String::from_utf8_lossy(&socket_path_output.stdout)
            .trim()
            .to_string();

        let prev_tmux = std::env::var("TMUX").ok();
        // SAFETY: all mutation of the `TMUX` env var in this test binary goes
        // through `TestServer`, and every test holds `SERIAL` for its duration,
        // so no other thread reads/writes it concurrently.
        unsafe {
            std::env::set_var("TMUX", format!("{socket_path},0,0"));
        }

        Self { socket, prev_tmux }
    }

    /// Raw tmux calls for setup/assertions that go around the code under test
    /// (e.g. querying things `actions.rs` doesn't expose, like window count).
    fn tmux(&self, args: &[&str]) -> std::process::Output {
        let mut full = vec!["-L", self.socket.as_str()];
        full.extend_from_slice(args);
        Command::new("tmux")
            .args(full)
            .output()
            .expect("tmux command failed to run")
    }

    fn tmux_ok(&self, args: &[&str]) -> String {
        let out = self.tmux(args);
        assert!(
            out.status.success(),
            "tmux {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn snapshot(&self) -> tmux_ui_manager::model::Snapshot {
        take_snapshot().expect("snapshot should succeed against the isolated socket")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
        // SAFETY: see `start` — serialized via `SERIAL`.
        unsafe {
            match &self.prev_tmux {
                Some(v) => std::env::set_var("TMUX", v),
                None => std::env::remove_var("TMUX"),
            }
        }
    }
}

#[test]
#[ignore]
fn new_session_appears_in_snapshot() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("new-session");

    actions::new_session("fresh", "/tmp").expect("new_session should succeed");

    let snap = server.snapshot();
    assert!(snap.sessions.iter().any(|s| s.name == "fresh"));
}

#[test]
#[ignore]
fn new_window_appears_under_target_session() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("new-window");

    let session_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{session_id}"]));
    actions::new_window(&session_id, "/tmp").expect("new_window should succeed");

    let snap = server.snapshot();
    let session = snap
        .session(&session_id)
        .expect("session should still exist");
    assert_eq!(session.windows.len(), 2);
}

#[test]
#[ignore]
fn split_pane_adds_second_pane_to_window() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("split-pane");

    let pane_id = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{pane_id}"]));
    actions::split_pane(&pane_id, "/tmp").expect("split_pane should succeed");

    let snap = server.snapshot();
    let window = &snap.sessions[0].windows[0];
    assert_eq!(window.panes.len(), 2);
}

#[test]
#[ignore]
fn rename_session_updates_name() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("rename-session");

    let session_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{session_id}"]));
    actions::rename_session(&session_id, "renamed").expect("rename_session should succeed");

    let snap = server.snapshot();
    assert_eq!(snap.session(&session_id).unwrap().name, "renamed");
}

#[test]
#[ignore]
fn rename_window_updates_name() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("rename-window");

    let window_id =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{window_id}"]));
    actions::rename_window(&window_id, "renamed-window").expect("rename_window should succeed");

    let snap = server.snapshot();
    let window = &snap.sessions[0].windows[0];
    assert_eq!(window.id, window_id);
    assert_eq!(window.name, "renamed-window");
}

#[test]
#[ignore]
fn set_pane_title_updates_title() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("pane-title");

    let pane_id = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{pane_id}"]));
    actions::set_pane_title(&pane_id, "my title").expect("set_pane_title should succeed");

    let snap = server.snapshot();
    let pane = &snap.sessions[0].windows[0].panes[0];
    assert_eq!(pane.title, "my title");
}

#[test]
#[ignore]
fn toggle_zoom_sets_and_clears_window_zoomed_flag() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("zoom");

    // Zoom only means something with 2+ panes.
    let pane_id = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{pane_id}"]));
    server.tmux_ok(&["split-window", "-d", "-t", "t1", "-c", "/tmp"]);

    actions::toggle_zoom(&pane_id).expect("toggle_zoom (on) should succeed");
    let zoomed_flag =
        server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{window_zoomed_flag}"]);
    assert_eq!(zoomed_flag, "1");

    actions::toggle_zoom(&pane_id).expect("toggle_zoom (off) should succeed");
    let unzoomed_flag =
        server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{window_zoomed_flag}"]);
    assert_eq!(unzoomed_flag, "0");
}

#[test]
#[ignore]
fn kill_pane_removes_it_leaving_sibling() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("kill-pane");

    server.tmux_ok(&["split-window", "-d", "-t", "t1", "-c", "/tmp"]);
    let victim =
        PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1.1", "#{pane_id}"]));

    actions::kill_pane(&victim).expect("kill_pane should succeed");

    let snap = server.snapshot();
    let window = &snap.sessions[0].windows[0];
    assert_eq!(window.panes.len(), 1);
    assert!(!window.panes.iter().any(|p| p.id == victim));
}

#[test]
#[ignore]
fn kill_window_removes_it_leaving_sibling() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("kill-window");

    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-c", "/tmp"]);
    let victim =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:1", "#{window_id}"]));

    actions::kill_window(&victim).expect("kill_window should succeed");

    let snap = server.snapshot();
    let session = &snap.sessions[0];
    assert_eq!(session.windows.len(), 1);
    assert!(!session.windows.iter().any(|w| w.id == victim));
}

#[test]
#[ignore]
fn kill_session_removes_it_leaving_sibling() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("kill-session");

    server.tmux_ok(&["new-session", "-d", "-s", "victim", "-c", "/tmp"]);
    let victim_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "victim", "#{session_id}"]));

    actions::kill_session(&victim_id).expect("kill_session should succeed");

    let snap = server.snapshot();
    assert_eq!(snap.sessions.len(), 1);
    assert!(!snap.sessions.iter().any(|s| s.id == victim_id));
}

#[test]
#[ignore]
fn kill_session_on_the_last_session_succeeds_and_server_exits() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("kill-last-session");

    let session_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1", "#{session_id}"]));

    // §10.2: tmux itself reports success even though this brings the whole
    // server down — verified empirically before writing this guard's App-level
    // wording (kill_session has no special-casing of its own).
    actions::kill_session(&session_id)
        .expect("killing the last session should still report success");

    let list_output = server.tmux(&["list-sessions"]);
    assert!(!list_output.status.success(), "server should have exited");
}

// -- M2: move-mode drag actions (§6.5 table) ---------------------------------

#[test]
#[ignore]
fn move_window_to_session_appends_it() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("move-window-session");

    server.tmux_ok(&["new-session", "-d", "-s", "t2", "-c", "/tmp"]);
    server.tmux_ok(&["new-window", "-d", "-t", "t2:", "-c", "/tmp"]); // t2 now has 2 windows
    let window_id =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{window_id}"]));
    let t2_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t2", "#{session_id}"]));

    actions::move_window_to_session(&window_id, &t2_id)
        .expect("move_window_to_session should succeed");

    let snap = server.snapshot();
    let t2 = snap.session(&t2_id).expect("t2 should still exist");
    assert_eq!(t2.windows.len(), 3);
    assert!(t2.windows.iter().any(|w| w.id == window_id));
    // t1 had only that one window, so it should have auto-destroyed (§ finding).
    assert!(!snap.sessions.iter().any(|s| s.name == "t1"));
}

#[test]
#[ignore]
fn reorder_window_same_session_inserts_before_anchor() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("reorder-same-session");

    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "second"]);
    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "third"]);
    let first =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{window_id}"]));
    let third =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:2", "#{window_id}"]));

    actions::reorder_window(&third, &first, false).expect("reorder_window should succeed");

    let snap = server.snapshot();
    let windows = &snap.sessions[0].windows;
    assert_eq!(
        windows[0].id, third,
        "third should now be inserted before first"
    );
}

#[test]
#[ignore]
fn reorder_window_cross_session_moves_and_positions() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("reorder-cross-session");

    server.tmux_ok(&["new-session", "-d", "-s", "t2", "-c", "/tmp"]);
    server.tmux_ok(&["new-window", "-d", "-t", "t2:", "-n", "winB"]);
    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "winA"]);

    let win_a =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:1", "#{window_id}"]));
    let win_b =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t2:1", "#{window_id}"]));

    actions::reorder_window(&win_a, &win_b, false).expect("cross-session reorder should succeed");

    let snap = server.snapshot();
    let t2 = snap
        .sessions
        .iter()
        .find(|s| s.name == "t2")
        .expect("t2 should exist");
    let idx = t2
        .windows
        .iter()
        .position(|w| w.id == win_a)
        .expect("winA should be in t2 now");
    assert_eq!(
        t2.windows[idx + 1].id,
        win_b,
        "winA should land immediately before winB"
    );

    let t1 = snap
        .sessions
        .iter()
        .find(|s| s.name == "t1")
        .expect("t1 should still exist");
    assert!(!t1.windows.iter().any(|w| w.id == win_a));
}

#[test]
#[ignore]
fn join_pane_into_window_moves_pane_as_new_split() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("join-pane-into-window");

    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "target"]);
    let pane = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{pane_id}"]));
    let target_window =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:1", "#{window_id}"]));

    actions::join_pane_into_window(&pane, &target_window)
        .expect("join_pane_into_window should succeed");

    let snap = server.snapshot();
    let window = snap.sessions[0]
        .windows
        .iter()
        .find(|w| w.id == target_window)
        .unwrap();
    assert_eq!(window.panes.len(), 2);
    assert!(window.panes.iter().any(|p| p.id == pane));
}

#[test]
#[ignore]
fn join_pane_onto_pane_splits_that_specific_pane() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("join-pane-onto-pane");

    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "source"]);
    let source_pane =
        PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:1", "#{pane_id}"]));
    let target_pane =
        PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{pane_id}"]));

    actions::join_pane_onto_pane(&source_pane, &target_pane)
        .expect("join_pane_onto_pane should succeed");

    let snap = server.snapshot();
    let window0 = &snap.sessions[0].windows[0];
    assert_eq!(window0.panes.len(), 2);
    assert!(window0.panes.iter().any(|p| p.id == source_pane));
}

#[test]
#[ignore]
fn pane_to_new_window_breaks_a_sibling_pane_into_new_window() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("pane-to-new-window-multi");

    server.tmux_ok(&["split-window", "-d", "-t", "t1", "-c", "/tmp"]);
    server.tmux_ok(&["new-session", "-d", "-s", "t2", "-c", "/tmp"]);
    let pane = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1.1", "#{pane_id}"]));
    let t2_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t2", "#{session_id}"]));

    actions::pane_to_new_window(&pane, &t2_id).expect("pane_to_new_window should succeed");

    let snap = server.snapshot();
    let t1 = snap.sessions.iter().find(|s| s.name == "t1").unwrap();
    assert_eq!(
        t1.windows[0].panes.len(),
        1,
        "t1's window should have lost the broken-out pane"
    );
    let t2 = snap.sessions.iter().find(|s| s.name == "t2").unwrap();
    assert!(
        t2.windows
            .iter()
            .any(|w| w.panes.iter().any(|p| p.id == pane))
    );
}

#[test]
#[ignore]
fn pane_to_new_window_on_an_only_pane_preserves_window_identity() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("pane-to-new-window-only");

    server.tmux_ok(&["rename-window", "-t", "t1:0", "custom-name"]);
    server.tmux_ok(&["new-session", "-d", "-s", "t2", "-c", "/tmp"]);
    let window_id =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{window_id}"]));
    let pane = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{pane_id}"]));
    let t2_id =
        SessionId::new(server.tmux_ok(&["display-message", "-p", "-t", "t2", "#{session_id}"]));

    // §ground-rule-6 deviation: break-pane succeeds unconditionally here — no
    // only-pane fallback exists in actions.rs. This test locks in the exact
    // behavior that makes the fallback unnecessary: id and name preserved.
    actions::pane_to_new_window(&pane, &t2_id)
        .expect("pane_to_new_window should succeed on an only pane");

    let snap = server.snapshot();
    let t2 = snap.sessions.iter().find(|s| s.name == "t2").unwrap();
    let moved = t2
        .windows
        .iter()
        .find(|w| w.id == window_id)
        .expect("window id preserved");
    assert_eq!(moved.name, "custom-name");
    // t1 lost its only window, so it should have auto-destroyed.
    assert!(!snap.sessions.iter().any(|s| s.name == "t1"));
}

#[test]
#[ignore]
fn window_to_new_session_recipe_creates_session_with_the_window() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("window-to-new-session");

    server.tmux_ok(&["rename-window", "-t", "t1:0", "editor"]);
    server.tmux_ok(&["new-window", "-d", "-t", "t1:", "-n", "other"]); // t1 keeps a window after
    let window_id =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{window_id}"]));

    actions::window_to_new_session("fresh", "/tmp", &window_id).expect("recipe should succeed");

    let snap = server.snapshot();
    let fresh = snap
        .sessions
        .iter()
        .find(|s| s.name == "fresh")
        .expect("new session should exist");
    assert_eq!(
        fresh.windows.len(),
        1,
        "placeholder window should have been cleaned up"
    );
    assert_eq!(fresh.windows[0].id, window_id);
    assert_eq!(fresh.windows[0].name, "editor");

    let t1 = snap.sessions.iter().find(|s| s.name == "t1").unwrap();
    assert!(!t1.windows.iter().any(|w| w.id == window_id));
}

#[test]
#[ignore]
fn window_to_new_session_recipe_rolls_back_on_duplicate_name() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("window-to-new-session-rollback");

    server.tmux_ok(&["new-session", "-d", "-s", "taken", "-c", "/tmp"]);
    let window_id =
        WindowId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1:0", "#{window_id}"]));

    let result = actions::window_to_new_session("taken", "/tmp", &window_id);
    assert!(
        result.is_err(),
        "creating a session with a name already in use should fail"
    );

    let snap = server.snapshot();
    // t1 must be untouched (the window never moved) and "taken" must still be
    // exactly what it was before (rollback didn't kill the real session, just
    // never got the chance to create a second one).
    let t1 = snap.sessions.iter().find(|s| s.name == "t1").unwrap();
    assert!(t1.windows.iter().any(|w| w.id == window_id));
    assert_eq!(
        snap.sessions.iter().filter(|s| s.name == "taken").count(),
        1
    );
}

#[test]
#[ignore]
fn pane_to_new_session_recipe_creates_session_with_the_pane() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let server = TestServer::start("pane-to-new-session");

    server.tmux_ok(&["split-window", "-d", "-t", "t1", "-c", "/tmp"]);
    let pane = PaneId::new(server.tmux_ok(&["display-message", "-p", "-t", "t1.1", "#{pane_id}"]));

    actions::pane_to_new_session("fresh", "/tmp", &pane).expect("recipe should succeed");

    let snap = server.snapshot();
    let fresh = snap.sessions.iter().find(|s| s.name == "fresh").unwrap();
    assert_eq!(fresh.windows.len(), 1);
    assert!(fresh.windows[0].panes.iter().any(|p| p.id == pane));

    let t1 = snap.sessions.iter().find(|s| s.name == "t1").unwrap();
    assert_eq!(
        t1.windows[0].panes.len(),
        1,
        "t1 should have lost the broken-out pane"
    );
}
