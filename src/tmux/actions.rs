use std::fmt;
use std::process::Command;

use crate::tmux::ids::{PaneId, SessionId, WindowId};

/// Every tmux mutation/jump in the app funnels through [`run_tmux`], so every
/// invocation is a `Command` argument vector — never a shell string (§10.4).
#[derive(Debug)]
pub struct ActionError {
    pub command: String,
    pub stderr: String,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` failed: {}", self.command, self.stderr.trim())
    }
}

impl std::error::Error for ActionError {}

fn run_tmux(args: &[&str]) -> Result<(), ActionError> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|e| ActionError {
            command: format!("tmux {}", args.join(" ")),
            stderr: e.to_string(),
        })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(ActionError {
            command: format!("tmux {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Attach/jump to a session (§5 row 1). Caller exits the process on success to
/// close the popup.
pub fn attach_session(id: &SessionId) -> Result<(), ActionError> {
    run_tmux(&["switch-client", "-t", id.as_target()])
}

/// Jump to a window (§5 row 2).
pub fn jump_window(session: &SessionId, window: &WindowId) -> Result<(), ActionError> {
    run_tmux(&["select-window", "-t", window.as_target()])?;
    run_tmux(&["switch-client", "-t", session.as_target()])
}

/// Jump to a pane (§5 row 3).
pub fn jump_pane(session: &SessionId, window: &WindowId, pane: &PaneId) -> Result<(), ActionError> {
    run_tmux(&["select-window", "-t", window.as_target()])?;
    run_tmux(&["select-pane", "-t", pane.as_target()])?;
    run_tmux(&["switch-client", "-t", session.as_target()])
}

/// New session (§5 row 4). Caller pre-validates the name (§10.5); tmux's own
/// error still surfaces via `ActionError` if a race slips a duplicate through.
pub fn new_session(name: &str, cwd: &str) -> Result<(), ActionError> {
    run_tmux(&["new-session", "-d", "-s", name, "-c", cwd])
}

/// New window in session S (§5 row 5).
pub fn new_window(session: &SessionId, cwd: &str) -> Result<(), ActionError> {
    let target = format!("{}:", session.as_target());
    run_tmux(&["new-window", "-d", "-t", &target, "-c", cwd])
}

/// Split pane P (§5 row 6). v1 always splits vertically — direction control is
/// a v2/board-view concern (§5 note).
pub fn split_pane(pane: &PaneId, cwd: &str) -> Result<(), ActionError> {
    run_tmux(&["split-window", "-d", "-t", pane.as_target(), "-c", cwd])
}

/// Rename session (§5 row 7).
pub fn rename_session(id: &SessionId, name: &str) -> Result<(), ActionError> {
    run_tmux(&["rename-session", "-t", id.as_target(), name])
}

/// Rename window (§5 row 8).
pub fn rename_window(id: &WindowId, name: &str) -> Result<(), ActionError> {
    run_tmux(&["rename-window", "-t", id.as_target(), name])
}

/// Set pane title (§5 row 9).
pub fn set_pane_title(id: &PaneId, title: &str) -> Result<(), ActionError> {
    run_tmux(&["select-pane", "-t", id.as_target(), "-T", title])
}

/// Kill session (§5 row 10). Attached/last-session confirm wording is decided by
/// the caller (App) before this runs; this function has no special-casing.
pub fn kill_session(id: &SessionId) -> Result<(), ActionError> {
    run_tmux(&["kill-session", "-t", id.as_target()])
}

/// Kill window (§5 row 11).
pub fn kill_window(id: &WindowId) -> Result<(), ActionError> {
    run_tmux(&["kill-window", "-t", id.as_target()])
}

/// Kill pane (§5 row 12).
pub fn kill_pane(id: &PaneId) -> Result<(), ActionError> {
    run_tmux(&["kill-pane", "-t", id.as_target()])
}

/// Zoom/unzoom pane (§5 row 13).
pub fn toggle_zoom(id: &PaneId) -> Result<(), ActionError> {
    run_tmux(&["resize-pane", "-Z", "-t", id.as_target()])
}
