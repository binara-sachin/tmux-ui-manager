use crate::model::{Session, Snapshot, Window};
use crate::tmux::actions;
use crate::tmux::ids::{PaneId, SessionId, WindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Sessions,
    Windows,
    Panes,
}

const COLUMNS: [Column; 3] = [Column::Sessions, Column::Windows, Column::Panes];

pub struct App {
    pub snapshot: Snapshot,
    pub focus: Column,
    pub selected_session: Option<SessionId>,
    pub selected_window: Option<WindowId>,
    pub selected_pane: Option<PaneId>,
    pub should_quit: bool,
}

impl App {
    pub fn new(snapshot: Snapshot) -> Self {
        let mut app = Self {
            snapshot,
            focus: Column::Sessions,
            selected_session: None,
            selected_window: None,
            selected_pane: None,
            should_quit: false,
        };
        let first_session = app.snapshot.sessions.first().map(|s| s.id.clone());
        app.set_selected_session(first_session);
        app
    }

    /// Replaces the model with a fresh snapshot (post-mutation or periodic tick, §4.3)
    /// and re-resolves selection by id in each column independently, falling back to
    /// the former neighbor (clamped), then the first item. Resolving session before
    /// window before pane means each step's fallback list already reflects the
    /// resolved parent.
    pub fn apply_refresh(&mut self, snapshot: Snapshot) {
        let prev_session_idx = self.session_index();
        let prev_window_idx = self.window_index();
        let prev_pane_idx = self.pane_index();
        let prev_session = self.selected_session.clone();
        let prev_window = self.selected_window.clone();
        let prev_pane = self.selected_pane.clone();

        self.snapshot = snapshot;

        let session_ids: Vec<SessionId> = self
            .snapshot
            .sessions
            .iter()
            .map(|s| s.id.clone())
            .collect();
        self.selected_session =
            resolve_after_refresh(&session_ids, prev_session.as_ref(), prev_session_idx);

        let window_ids: Vec<WindowId> = self.windows().iter().map(|w| w.id.clone()).collect();
        self.selected_window =
            resolve_after_refresh(&window_ids, prev_window.as_ref(), prev_window_idx);

        let pane_ids: Vec<PaneId> = self.panes().iter().map(|p| p.id.clone()).collect();
        self.selected_pane = resolve_after_refresh(&pane_ids, prev_pane.as_ref(), prev_pane_idx);
    }

    pub fn current_session(&self) -> Option<&Session> {
        self.selected_session
            .as_ref()
            .and_then(|id| self.snapshot.session(id))
    }

    pub fn current_window(&self) -> Option<&Window> {
        self.current_session()
            .and_then(|s| self.selected_window.as_ref().and_then(|id| s.window(id)))
    }

    pub fn windows(&self) -> &[Window] {
        self.current_session()
            .map(|s| s.windows.as_slice())
            .unwrap_or(&[])
    }

    pub fn panes(&self) -> &[crate::model::Pane] {
        self.current_window()
            .map(|w| w.panes.as_slice())
            .unwrap_or(&[])
    }

    pub fn move_focus(&mut self, delta: i32) {
        let idx = COLUMNS.iter().position(|c| *c == self.focus).unwrap() as i32;
        let len = COLUMNS.len() as i32;
        let new_idx = (idx + delta).rem_euclid(len);
        self.focus = COLUMNS[new_idx as usize];
    }

    pub fn move_selection(&mut self, delta: i32) {
        match self.focus {
            Column::Sessions => {
                let ids: Vec<SessionId> = self
                    .snapshot
                    .sessions
                    .iter()
                    .map(|s| s.id.clone())
                    .collect();
                let next = shift_selection(&ids, self.selected_session.as_ref(), delta);
                self.set_selected_session(next);
            }
            Column::Windows => {
                let ids: Vec<WindowId> = self.windows().iter().map(|w| w.id.clone()).collect();
                let next = shift_selection(&ids, self.selected_window.as_ref(), delta);
                self.set_selected_window(next);
            }
            Column::Panes => {
                let ids: Vec<PaneId> = self.panes().iter().map(|p| p.id.clone()).collect();
                self.selected_pane = shift_selection(&ids, self.selected_pane.as_ref(), delta);
            }
        }
    }

    pub fn jump_to_edge(&mut self, top: bool) {
        match self.focus {
            Column::Sessions => {
                let id = if top {
                    self.snapshot.sessions.first().map(|s| s.id.clone())
                } else {
                    self.snapshot.sessions.last().map(|s| s.id.clone())
                };
                self.set_selected_session(id);
            }
            Column::Windows => {
                let id = if top {
                    self.windows().first().map(|w| w.id.clone())
                } else {
                    self.windows().last().map(|w| w.id.clone())
                };
                self.set_selected_window(id);
            }
            Column::Panes => {
                self.selected_pane = if top {
                    self.panes().first().map(|p| p.id.clone())
                } else {
                    self.panes().last().map(|p| p.id.clone())
                };
            }
        }
    }

    /// Enter: attach/jump to whatever is selected in the focused column. On success
    /// the process should exit so the popup closes; on failure the error is
    /// swallowed for now — the toast overlay that surfaces it lands in M1.
    pub fn activate(&mut self) {
        let result = match self.focus {
            Column::Sessions => self.selected_session.as_ref().map(actions::attach_session),
            Column::Windows => match (&self.selected_session, &self.selected_window) {
                (Some(sid), Some(wid)) => Some(actions::jump_window(sid, wid)),
                _ => None,
            },
            Column::Panes => {
                match (
                    &self.selected_session,
                    &self.selected_window,
                    &self.selected_pane,
                ) {
                    (Some(sid), Some(wid), Some(pid)) => Some(actions::jump_pane(sid, wid, pid)),
                    _ => None,
                }
            }
        };

        if let Some(Ok(())) = result {
            self.should_quit = true;
        }
    }

    fn set_selected_session(&mut self, id: Option<SessionId>) {
        self.selected_session = id;
        let first_window = self
            .current_session()
            .and_then(|s| s.windows.first())
            .map(|w| w.id.clone());
        self.set_selected_window(first_window);
    }

    fn set_selected_window(&mut self, id: Option<WindowId>) {
        self.selected_window = id;
        let first_pane = self
            .current_window()
            .and_then(|w| w.panes.first())
            .map(|p| p.id.clone());
        self.selected_pane = first_pane;
    }

    fn session_index(&self) -> Option<usize> {
        self.selected_session
            .as_ref()
            .and_then(|id| self.snapshot.sessions.iter().position(|s| &s.id == id))
    }

    fn window_index(&self) -> Option<usize> {
        self.selected_window
            .as_ref()
            .and_then(|id| self.windows().iter().position(|w| &w.id == id))
    }

    fn pane_index(&self) -> Option<usize> {
        self.selected_pane
            .as_ref()
            .and_then(|id| self.panes().iter().position(|p| &p.id == id))
    }
}

fn shift_selection<T: PartialEq + Clone>(
    items: &[T],
    current: Option<&T>,
    delta: i32,
) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    let current_idx = current.and_then(|c| items.iter().position(|i| i == c));
    let idx = match current_idx {
        Some(i) => (i as i32 + delta).clamp(0, items.len() as i32 - 1) as usize,
        None => 0,
    };
    Some(items[idx].clone())
}

/// After a refresh, prefer the still-present id; otherwise fall back to the item
/// that now occupies the previous index (clamped), then the first item (§4.3).
fn resolve_after_refresh<T: PartialEq + Clone>(
    items: &[T],
    previous_id: Option<&T>,
    previous_index: Option<usize>,
) -> Option<T> {
    if let Some(id) = previous_id
        && items.contains(id)
    {
        return Some(id.clone());
    }
    if items.is_empty() {
        return None;
    }
    let idx = previous_index.unwrap_or(0).min(items.len() - 1);
    Some(items[idx].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pane, Totals};

    fn pane(id: &str, index: u32) -> Pane {
        Pane {
            id: PaneId::new(id.to_string()),
            index,
            active: index == 0,
            command: "zsh".into(),
            path: "/tmp".into(),
            title: String::new(),
            zoomed: false,
        }
    }

    fn window(id: &str, index: u32, panes: Vec<Pane>) -> Window {
        Window {
            id: WindowId::new(id.to_string()),
            index,
            name: format!("win{index}"),
            active: index == 0,
            layout: String::new(),
            panes,
        }
    }

    fn session(id: &str, name: &str, windows: Vec<Window>) -> Session {
        Session {
            id: SessionId::new(id.to_string()),
            name: name.into(),
            attached: false,
            windows,
        }
    }

    fn sample_snapshot() -> Snapshot {
        let s1 = session(
            "$1",
            "main",
            vec![
                window("@1", 0, vec![pane("%1", 0), pane("%2", 1)]),
                window("@2", 1, vec![pane("%3", 0)]),
            ],
        );
        let s2 = session("$2", "dotfiles", vec![window("@3", 0, vec![pane("%4", 0)])]);

        Snapshot {
            totals: Totals {
                sessions: 2,
                windows: 3,
                panes: 4,
            },
            client_session: None,
            sessions: vec![s1, s2],
        }
    }

    #[test]
    fn new_app_selects_first_session_window_pane() {
        let app = App::new(sample_snapshot());
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$1");
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@1");
        assert_eq!(app.selected_pane.as_ref().unwrap().as_target(), "%1");
    }

    #[test]
    fn changing_session_selection_cascades_to_first_window_and_pane() {
        let mut app = App::new(sample_snapshot());
        app.move_selection(1); // sessions column: $1 -> $2
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$2");
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@3");
        assert_eq!(app.selected_pane.as_ref().unwrap().as_target(), "%4");
    }

    #[test]
    fn changing_window_selection_cascades_to_first_pane() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.move_selection(1); // @1 -> @2
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@2");
        assert_eq!(app.selected_pane.as_ref().unwrap().as_target(), "%3");
    }

    #[test]
    fn selection_clamps_at_column_boundaries() {
        let mut app = App::new(sample_snapshot());
        app.move_selection(-5);
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$1");
        app.move_selection(5);
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$2");
        app.move_selection(5);
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$2");
    }

    #[test]
    fn focus_wraps_in_both_directions() {
        let mut app = App::new(sample_snapshot());
        assert_eq!(app.focus, Column::Sessions);
        app.move_focus(-1);
        assert_eq!(app.focus, Column::Panes);
        app.move_focus(1);
        assert_eq!(app.focus, Column::Sessions);
    }

    #[test]
    fn jump_to_edge_selects_first_or_last() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.jump_to_edge(false);
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@2");
        app.jump_to_edge(true);
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@1");
    }

    #[test]
    fn refresh_keeps_selection_when_id_still_present() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.move_selection(1); // @1 -> @2
        app.apply_refresh(sample_snapshot());
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@2");
        assert_eq!(app.selected_pane.as_ref().unwrap().as_target(), "%3");
    }

    #[test]
    fn refresh_falls_back_to_neighbor_index_when_selection_vanished() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.move_selection(1); // select @2 (index 1)

        // @2 is gone in the new snapshot; only @1 remains for session $1.
        let mut shrunk = sample_snapshot();
        shrunk.sessions[0].windows.truncate(1);
        app.apply_refresh(shrunk);

        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@1");
    }
}
