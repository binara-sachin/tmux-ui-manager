use crate::tmux::ids::{PaneId, SessionId, WindowId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub sessions: usize,
    pub windows: usize,
    pub panes: usize,
}

#[derive(Debug, Clone)]
pub struct Pane {
    pub id: PaneId,
    /// Not shown in v1 (rows are keyed by `%id`); kept for parser/model completeness.
    #[allow(dead_code)]
    pub index: u32,
    pub active: bool,
    pub command: String,
    pub path: String,
    /// Rename overlay (M1) reads/writes this via `select-pane -T`.
    #[allow(dead_code)]
    pub title: String,
    pub zoomed: bool,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    /// Not surfaced in v1's UI (selection state is what's visually marked); kept
    /// for parser/model completeness.
    #[allow(dead_code)]
    pub active: bool,
    /// Raw `#{window_layout}`, captured but unused until v2's board view (§12).
    #[allow(dead_code)]
    pub layout: String,
    pub panes: Vec<Pane>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub attached: bool,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub sessions: Vec<Session>,
    /// Session this popup's client is currently attached to (§4.2); not yet
    /// consumed in v1 UI beyond the attached-dot, which uses `Session::attached`.
    #[allow(dead_code)]
    pub client_session: Option<SessionId>,
    pub totals: Totals,
}

impl Snapshot {
    pub fn session(&self, id: &SessionId) -> Option<&Session> {
        self.sessions.iter().find(|s| &s.id == id)
    }
}

impl Session {
    pub fn window(&self, id: &WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| &w.id == id)
    }
}
