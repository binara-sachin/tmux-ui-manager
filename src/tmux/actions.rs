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
