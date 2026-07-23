use crate::model::{Session, Snapshot, Window};
use crate::tmux::actions::{self, ActionError};
use crate::tmux::ids::{PaneId, SessionId, WindowId};
use crate::tmux::snapshot::take_snapshot;
use crate::ui::drag::{DragItem, DropTarget, PlannedAction, plan_drop};
use crate::ui::overlays::{ConfirmKind, ConfirmOverlay, InputKind, InputOverlay, Toast};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Sessions,
    Windows,
    Panes,
}

const COLUMNS: [Column; 3] = [Column::Sessions, Column::Windows, Column::Panes];

/// The selection to restore if a drag is cancelled (§6.5: Esc = silent cancel,
/// nothing moved).
#[derive(Debug, Clone)]
pub struct OriginSelection {
    pub focus: Column,
    pub session: Option<SessionId>,
    pub window: Option<WindowId>,
    pub pane: Option<PaneId>,
}

/// Move-mode state (§6.5). The target cursor rides on the normal
/// `selected_session`/`selected_window`/`selected_pane` fields (so cascade
/// navigation "just works" while dragging) plus the two extra landable
/// positions those fields can't represent: a pseudo-row, or — for a dragged
/// window — a gap between window rows.
#[derive(Debug, Clone)]
pub struct DragState {
    pub item: DragItem,
    pub origin: OriginSelection,
    /// True when the cursor is past the last row in the focused column, on the
    /// "+ new session" (Sessions column) or "+ new window" (Windows column,
    /// pane-drag only) pseudo-row.
    pub on_new_session_row: bool,
    pub on_new_window_row: bool,
    /// Gap index `0..=windows().len()` for a window-drag in the Windows column;
    /// `None` until first navigated there (reset every time focus enters that
    /// column, see `reset_drag_cursor_for_focus`).
    pub gap_index: Option<usize>,
}

/// The app's interaction mode. Idle is `Normal`; §6.5's "Dragging" state is
/// `Mode::Dragging` (M2). Committing a drag onto "+ new session" transitions
/// into `Mode::Input` carrying the drag item (see `InputKind::NewSessionFromDrag`).
pub enum Mode {
    Normal,
    Input(InputOverlay),
    Confirm(ConfirmOverlay),
    Dragging(DragState),
}

pub struct App {
    pub snapshot: Snapshot,
    pub focus: Column,
    pub selected_session: Option<SessionId>,
    pub selected_window: Option<WindowId>,
    pub selected_pane: Option<PaneId>,
    pub should_quit: bool,
    pub mode: Mode,
    pub toast: Option<Toast>,
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
            mode: Mode::Normal,
            toast: None,
        };
        let first_session = app.snapshot.sessions.first().map(|s| s.id.clone());
        app.set_selected_session(first_session);
        app
    }

    /// Clears an expired toast; call once per loop iteration before drawing.
    pub fn expire_toast(&mut self) {
        if self.toast.as_ref().is_some_and(Toast::expired) {
            self.toast = None;
        }
    }

    fn selected_pane_ref(&self) -> Option<&crate::model::Pane> {
        self.panes()
            .iter()
            .find(|p| Some(&p.id) == self.selected_pane.as_ref())
    }

    /// Runs a mutation's result through the common §4.3 policy: re-snapshot
    /// regardless of outcome, and surface a toast on failure with tmux's stderr.
    fn report(&mut self, result: Result<(), ActionError>) {
        if let Err(e) = result {
            self.toast = Some(Toast::error(e.to_string()));
        }
        self.refresh_from_tmux();
    }

    fn refresh_from_tmux(&mut self) {
        if let Ok(snapshot) = take_snapshot() {
            self.apply_refresh(snapshot);
        }
    }

    /// `n`: contextual new (§6.3). Sessions get a name-input overlay; windows/panes
    /// are created immediately since tmux supplies sensible defaults for those.
    pub fn open_new(&mut self) {
        match self.focus {
            Column::Sessions => {
                self.mode = Mode::Input(InputOverlay::new(InputKind::NewSession, String::new()));
            }
            Column::Windows => {
                let Some(sid) = self.selected_session.clone() else {
                    return;
                };
                let cwd = self
                    .current_session()
                    .and_then(active_pane_path)
                    .map(str::to_string)
                    .unwrap_or_else(home_dir);
                let result = actions::new_window(&sid, &cwd);
                self.report(result);
            }
            Column::Panes => {
                let Some(pid) = self.selected_pane.clone() else {
                    return;
                };
                let cwd = self
                    .selected_pane_ref()
                    .map(|p| p.path.clone())
                    .unwrap_or_else(home_dir);
                let result = actions::split_pane(&pid, &cwd);
                self.report(result);
            }
        }
    }

    /// `r`: rename selected (§6.3) — opens an input overlay pre-filled with the
    /// current name/title.
    pub fn open_rename(&mut self) {
        match self.focus {
            Column::Sessions => {
                if let Some(session) = self.current_session() {
                    let kind = InputKind::RenameSession(session.id.clone());
                    self.mode = Mode::Input(InputOverlay::new(kind, session.name.clone()));
                }
            }
            Column::Windows => {
                if let Some(window) = self.current_window() {
                    let kind = InputKind::RenameWindow(window.id.clone());
                    self.mode = Mode::Input(InputOverlay::new(kind, window.name.clone()));
                }
            }
            Column::Panes => {
                if let Some(pane) = self.selected_pane_ref() {
                    let kind = InputKind::RenamePaneTitle(pane.id.clone());
                    self.mode = Mode::Input(InputOverlay::new(kind, pane.title.clone()));
                }
            }
        }
    }

    /// `x`: kill selected (§6.3) — opens a confirm overlay. Session kills get
    /// stronger wording when killing the attached or last session (§5, §10.2).
    pub fn open_kill_confirm(&mut self) {
        match self.focus {
            Column::Sessions => {
                if let Some(session) = self.current_session() {
                    let is_last = self.snapshot.sessions.len() == 1;
                    let is_attached = self.snapshot.client_session.as_ref() == Some(&session.id);
                    let message = if is_last {
                        format!(
                            "kill '{}' — this is the last session, the tmux server will exit. kill?",
                            session.name
                        )
                    } else if is_attached {
                        format!(
                            "kill attached session '{}'? (client will jump/detach)",
                            session.name
                        )
                    } else {
                        format!("kill session '{}'?", session.name)
                    };
                    self.mode = Mode::Confirm(ConfirmOverlay {
                        kind: ConfirmKind::KillSession(session.id.clone()),
                        message,
                    });
                }
            }
            Column::Windows => {
                if let Some(window) = self.current_window() {
                    self.mode = Mode::Confirm(ConfirmOverlay {
                        kind: ConfirmKind::KillWindow(window.id.clone()),
                        message: format!("kill window '{}'?", window.name),
                    });
                }
            }
            Column::Panes => {
                if let Some(pane) = self.selected_pane_ref() {
                    self.mode = Mode::Confirm(ConfirmOverlay {
                        kind: ConfirmKind::KillPane(pane.id.clone()),
                        message: format!("kill pane '{}'?", pane.id),
                    });
                }
            }
        }
    }

    /// `z`: zoom toggle, pane column only (§6.3).
    pub fn toggle_zoom(&mut self) {
        if self.focus != Column::Panes {
            return;
        }
        if let Some(pid) = self.selected_pane.clone() {
            let result = actions::toggle_zoom(&pid);
            self.report(result);
        }
    }

    /// `Space`/`m`: pick up the selected window/pane and enter move-mode
    /// (§6.5). Sessions aren't draggable in v1.
    pub fn enter_move_mode(&mut self) {
        if matches!(self.mode, Mode::Dragging(_)) {
            return;
        }
        match self.focus {
            Column::Sessions => {
                self.toast = Some(Toast::info("sessions can't be moved"));
            }
            Column::Windows => {
                if let Some(wid) = self.selected_window.clone() {
                    self.begin_drag(DragItem::Window(wid));
                }
            }
            Column::Panes => {
                if let Some(pid) = self.selected_pane.clone() {
                    self.begin_drag(DragItem::Pane(pid));
                }
            }
        }
    }

    fn begin_drag(&mut self, item: DragItem) {
        let origin = OriginSelection {
            focus: self.focus,
            session: self.selected_session.clone(),
            window: self.selected_window.clone(),
            pane: self.selected_pane.clone(),
        };
        self.mode = Mode::Dragging(DragState {
            item,
            origin,
            on_new_session_row: false,
            on_new_window_row: false,
            gap_index: None,
        });
        self.reset_drag_cursor_for_focus();
    }

    /// Esc while dragging: silent cancel, restoring the pre-drag selection
    /// (§6.5 — "no toast, silent cancel is fine").
    pub fn cancel_drag(&mut self) {
        if let Mode::Dragging(drag) = &self.mode {
            let origin = drag.origin.clone();
            self.focus = origin.focus;
            self.selected_session = origin.session;
            self.selected_window = origin.window;
            self.selected_pane = origin.pane;
        }
        self.mode = Mode::Normal;
    }

    /// ←/→ (and h/l) while dragging: move focus among only the columns the
    /// dragged item type can target (§6.5 table) — a window drag never visits
    /// the Panes column, a pane drag visits all three.
    pub fn drag_move_focus(&mut self, delta: i32) {
        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let columns: &[Column] = match drag.item {
            DragItem::Window(_) => &[Column::Sessions, Column::Windows],
            DragItem::Pane(_) => &[Column::Sessions, Column::Windows, Column::Panes],
        };
        let idx = columns.iter().position(|c| *c == self.focus).unwrap_or(0) as i32;
        let len = columns.len() as i32;
        let new_idx = (idx + delta).rem_euclid(len);
        self.focus = columns[new_idx as usize];
        self.reset_drag_cursor_for_focus();
    }

    /// Resets the per-column drag-cursor sub-state to a sensible default every
    /// time focus enters that column — simpler and more predictable than trying
    /// to preserve a stale position across an unrelated session's window list.
    fn reset_drag_cursor_for_focus(&mut self) {
        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let item = drag.item.clone();
        match (&item, self.focus) {
            (DragItem::Window(wid), Column::Windows) => {
                let default = self
                    .windows()
                    .iter()
                    .position(|w| &w.id == wid)
                    .map(|p| p + 1)
                    .unwrap_or(self.windows().len());
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.gap_index = Some(default);
                }
            }
            (DragItem::Pane(_), Column::Windows) => {
                self.selected_window = self.windows().first().map(|w| w.id.clone());
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_window_row = false;
                }
            }
            (DragItem::Pane(_), Column::Panes) => {
                self.selected_pane = self.panes().first().map(|p| p.id.clone());
            }
            (_, Column::Sessions) => {
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_session_row = false;
                }
            }
            _ => {}
        }
    }

    /// ↑/↓ (and j/k) while dragging: move the target cursor within the focused
    /// column (§6.5). Invalid gap positions (bracketing the dragged window's own
    /// current spot) are skipped in the direction of travel; everything else is
    /// plain clamped stepping — validity for row-type targets is a resolve-time
    /// concern (not-highlighted / no-op on commit), not a navigation-time one.
    pub fn drag_move_cursor(&mut self, delta: i32) {
        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let item = drag.item.clone();
        match (&item, self.focus) {
            (_, Column::Sessions) => self.drag_move_sessions_cursor(delta),
            (DragItem::Window(wid), Column::Windows) => {
                self.drag_move_window_gap(wid.clone(), delta)
            }
            (DragItem::Pane(_), Column::Windows) => self.drag_move_windows_cursor(delta),
            (DragItem::Pane(_), Column::Panes) => self.move_selection(delta),
            (DragItem::Window(_), Column::Panes) => {}
        }
    }

    fn drag_move_sessions_cursor(&mut self, delta: i32) {
        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let on_pseudo = drag.on_new_session_row;
        let sessions_len = self.snapshot.sessions.len();
        let cur_idx = self.session_index();

        if delta > 0 {
            if on_pseudo {
                return;
            }
            let at_last = cur_idx
                .map(|i| i + 1 == sessions_len)
                .unwrap_or(sessions_len == 0);
            if at_last {
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_session_row = true;
                }
                return;
            }
            self.move_selection(1);
        } else {
            if on_pseudo {
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_session_row = false;
                }
                return;
            }
            self.move_selection(-1);
        }
    }

    fn drag_move_windows_cursor(&mut self, delta: i32) {
        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let on_pseudo = drag.on_new_window_row;
        let windows_len = self.windows().len();
        let cur_idx = self.window_index();

        if delta > 0 {
            if on_pseudo {
                return;
            }
            let at_last = cur_idx
                .map(|i| i + 1 == windows_len)
                .unwrap_or(windows_len == 0);
            if at_last {
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_window_row = true;
                }
                return;
            }
            self.move_selection(1);
        } else {
            if on_pseudo {
                if let Mode::Dragging(drag) = &mut self.mode {
                    drag.on_new_window_row = false;
                }
                return;
            }
            self.move_selection(-1);
        }
    }

    fn drag_move_window_gap(&mut self, dragged: WindowId, delta: i32) {
        let len = self.windows().len();
        let dragged_pos = self.windows().iter().position(|w| w.id == dragged);
        let total = len + 1;
        if total == 0 {
            return;
        }

        let Mode::Dragging(drag) = &self.mode else {
            return;
        };
        let current = drag
            .gap_index
            .unwrap_or_else(|| dragged_pos.map(|p| p + 1).unwrap_or(len));
        let step = delta.signum();
        let mut idx = current as i32;
        // Stays at `current` unless the walk actually finds a non-no-op gap —
        // hitting the boundary while still on a no-op (e.g. every gap on one
        // side of a 2-window session bar the far end brackets the dragged
        // window) must refuse to move, not land on that invalid gap.
        let mut result = current;
        loop {
            let next = (idx + step).clamp(0, total as i32 - 1);
            if next == idx {
                break;
            }
            idx = next;
            let is_noop = dragged_pos.is_some_and(|p| idx as usize == p || idx as usize == p + 1);
            if !is_noop {
                result = idx as usize;
                break;
            }
        }

        if let Mode::Dragging(drag) = &mut self.mode {
            drag.gap_index = Some(result);
        }
    }

    /// Resolves the current cursor position to a [`DropTarget`], given whatever
    /// column/selection/gap state it currently rests on.
    pub fn resolve_drop_target(&self) -> Option<DropTarget> {
        let Mode::Dragging(drag) = &self.mode else {
            return None;
        };
        match (&drag.item, self.focus) {
            (_, Column::Sessions) => {
                if drag.on_new_session_row {
                    Some(DropTarget::NewSessionRow)
                } else {
                    self.selected_session.clone().map(DropTarget::SessionRow)
                }
            }
            (DragItem::Window(_), Column::Windows) => {
                let idx = drag.gap_index?;
                let windows = self.windows();
                if windows.is_empty() {
                    None
                } else if idx < windows.len() {
                    Some(DropTarget::WindowGap {
                        anchor: windows[idx].id.clone(),
                        after: false,
                    })
                } else {
                    windows.last().map(|w| DropTarget::WindowGap {
                        anchor: w.id.clone(),
                        after: true,
                    })
                }
            }
            (DragItem::Pane(_), Column::Windows) => {
                if drag.on_new_window_row {
                    Some(DropTarget::NewWindowRow)
                } else {
                    self.selected_window.clone().map(DropTarget::WindowRow)
                }
            }
            (DragItem::Pane(_), Column::Panes) => {
                self.selected_pane.clone().map(DropTarget::PaneRow)
            }
            (DragItem::Window(_), Column::Panes) => None,
        }
    }

    /// What committing right now would do (§6.5's "live description... the
    /// primary safety mechanism" is derived from this same value, see
    /// `ui::statusbar`).
    pub fn plan_current_drop(&self) -> PlannedAction {
        let Mode::Dragging(drag) = &self.mode else {
            return PlannedAction::NoOp;
        };
        let Some(target) = self.resolve_drop_target() else {
            return PlannedAction::NoOp;
        };
        let own_window = match &drag.item {
            DragItem::Pane(pid) => self.window_containing_pane(pid),
            DragItem::Window(_) => None,
        };
        let window_list: Vec<WindowId> = self.windows().iter().map(|w| w.id.clone()).collect();
        plan_drop(
            &drag.item,
            &target,
            drag.origin.session.as_ref(),
            own_window.as_ref(),
            &window_list,
        )
    }

    /// Enter while dragging: commit the currently planned action (§6.5).
    pub fn commit_drag(&mut self) {
        let action = self.plan_current_drop();
        match action {
            PlannedAction::NoOp => {}
            PlannedAction::MoveWindowToSession { window, session } => {
                let result = actions::move_window_to_session(&window, &session);
                self.mode = Mode::Normal;
                self.report(result);
            }
            PlannedAction::ReorderWindow {
                window,
                anchor,
                after,
            } => {
                let result = actions::reorder_window(&window, &anchor, after);
                self.mode = Mode::Normal;
                self.report(result);
            }
            PlannedAction::WindowToNewSession { window } => {
                let prefill = self
                    .snapshot
                    .sessions
                    .iter()
                    .flat_map(|s| &s.windows)
                    .find(|w| w.id == window)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                self.mode = Mode::Input(InputOverlay::new(
                    InputKind::NewSessionFromDrag(DragItem::Window(window)),
                    prefill,
                ));
            }
            PlannedAction::JoinPaneIntoWindow { pane, window } => {
                let result = actions::join_pane_into_window(&pane, &window);
                self.mode = Mode::Normal;
                self.report(result);
            }
            PlannedAction::SplitPaneOntoPane { pane, target } => {
                let result = actions::join_pane_onto_pane(&pane, &target);
                self.mode = Mode::Normal;
                self.report(result);
            }
            PlannedAction::PaneToNewWindowInSession { pane, session } => {
                let result = actions::pane_to_new_window(&pane, &session);
                self.mode = Mode::Normal;
                self.report(result);
            }
            PlannedAction::PaneToNewSession { pane } => {
                self.mode = Mode::Input(InputOverlay::new(
                    InputKind::NewSessionFromDrag(DragItem::Pane(pane)),
                    String::new(),
                ));
            }
        }
    }

    fn window_name(&self, id: &WindowId) -> String {
        self.snapshot
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .find(|w| &w.id == id)
            .map(|w| w.name.clone())
            .unwrap_or_default()
    }

    fn session_name(&self, id: &SessionId) -> String {
        self.snapshot
            .session(id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// The live "what would happen" sentence (§6.5 — "the primary safety
    /// mechanism"), derived from the exact same [`PlannedAction`] `commit_drag`
    /// runs, so the two can never diverge. `None` means "no-op" (not
    /// highlighted, nothing shown).
    pub fn describe_planned_action(&self, action: &PlannedAction) -> Option<String> {
        match action {
            PlannedAction::NoOp => None,
            PlannedAction::MoveWindowToSession { window, session } => Some(format!(
                "move window '{}' \u{2192} session '{}'",
                self.window_name(window),
                self.session_name(session)
            )),
            PlannedAction::ReorderWindow {
                window,
                anchor,
                after,
            } => {
                let side = if *after { "after" } else { "before" };
                Some(format!(
                    "reorder window '{}' {side} '{}'",
                    self.window_name(window),
                    self.window_name(anchor)
                ))
            }
            PlannedAction::WindowToNewSession { window } => Some(format!(
                "window '{}' \u{2192} new session",
                self.window_name(window)
            )),
            PlannedAction::JoinPaneIntoWindow { pane, window } => Some(format!(
                "join pane {pane} \u{2192} window '{}'",
                self.window_name(window)
            )),
            PlannedAction::SplitPaneOntoPane { pane, target } => {
                Some(format!("split pane {target} with {pane}"))
            }
            PlannedAction::PaneToNewWindowInSession { pane, session } => Some(format!(
                "pane {pane} \u{2192} new window in '{}'",
                self.session_name(session)
            )),
            PlannedAction::PaneToNewSession { pane } => {
                Some(format!("pane {pane} \u{2192} new session"))
            }
        }
    }

    pub fn input_char(&mut self, c: char) {
        if let Mode::Input(overlay) = &mut self.mode {
            overlay.push_char(c);
        }
    }

    pub fn input_backspace(&mut self) {
        if let Mode::Input(overlay) = &mut self.mode {
            overlay.backspace();
        }
    }

    pub fn input_cancel(&mut self) {
        self.mode = Mode::Normal;
    }

    /// Enter on an input overlay: validate, then either surface an inline error
    /// (stay open) or run the action and close (§6.6, §10.5).
    pub fn input_confirm(&mut self) {
        let Mode::Input(overlay) = &self.mode else {
            return;
        };
        let text = overlay.text.trim().to_string();
        let kind = overlay.kind.clone();

        let validation = match &kind {
            InputKind::NewSession | InputKind::NewSessionFromDrag(_) => {
                validate_session_name(&text, &self.snapshot.sessions, None)
            }
            InputKind::RenameSession(id) => {
                validate_session_name(&text, &self.snapshot.sessions, Some(id))
            }
            InputKind::RenameWindow(_) | InputKind::RenamePaneTitle(_) => validate_non_empty(&text),
        };

        if let Err(msg) = validation {
            if let Mode::Input(overlay) = &mut self.mode {
                overlay.error = Some(msg);
            }
            return;
        }

        let result = match &kind {
            InputKind::NewSession => actions::new_session(&text, &home_dir()),
            InputKind::RenameSession(id) => actions::rename_session(id, &text),
            InputKind::RenameWindow(id) => actions::rename_window(id, &text),
            InputKind::RenamePaneTitle(id) => actions::set_pane_title(id, &text),
            InputKind::NewSessionFromDrag(DragItem::Window(wid)) => {
                actions::window_to_new_session(&text, &home_dir(), wid)
            }
            InputKind::NewSessionFromDrag(DragItem::Pane(pid)) => {
                actions::pane_to_new_session(&text, &home_dir(), pid)
            }
        };

        self.mode = Mode::Normal;
        self.report(result);
    }

    pub fn confirm_yes(&mut self) {
        let Mode::Confirm(overlay) = &self.mode else {
            return;
        };
        let kind = overlay.kind.clone();
        self.mode = Mode::Normal;
        let result = match kind {
            ConfirmKind::KillSession(id) => actions::kill_session(&id),
            ConfirmKind::KillWindow(id) => actions::kill_window(&id),
            ConfirmKind::KillPane(id) => actions::kill_pane(&id),
        };
        self.report(result);
    }

    pub fn confirm_no(&mut self) {
        self.mode = Mode::Normal;
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

        // §10.10: if the dragged entity died mid-drag (external process exit,
        // caught by the periodic tick), cancel the drag silently — no toast.
        if let Mode::Dragging(drag) = &self.mode {
            let still_exists = match &drag.item {
                DragItem::Window(wid) => self.window_exists(wid),
                DragItem::Pane(pid) => self.pane_exists(pid),
            };
            if !still_exists {
                self.mode = Mode::Normal;
            }
        }
    }

    fn window_exists(&self, id: &WindowId) -> bool {
        self.snapshot
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .any(|w| &w.id == id)
    }

    fn pane_exists(&self, id: &PaneId) -> bool {
        self.snapshot
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .flat_map(|w| &w.panes)
            .any(|p| &p.id == id)
    }

    fn window_containing_pane(&self, id: &PaneId) -> Option<WindowId> {
        self.snapshot
            .sessions
            .iter()
            .flat_map(|s| &s.windows)
            .find(|w| w.panes.iter().any(|p| &p.id == id))
            .map(|w| w.id.clone())
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

/// cwd of session S's active window's active pane, used as the default `-c` for
/// `new-window` (§5 row 5).
fn active_pane_path(session: &Session) -> Option<&str> {
    session
        .windows
        .iter()
        .find(|w| w.active)
        .and_then(|w| w.panes.iter().find(|p| p.active))
        .map(|p| p.path.as_str())
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

fn validate_non_empty(name: &str) -> Result<(), String> {
    if name.is_empty() {
        Err("cannot be empty".to_string())
    } else {
        Ok(())
    }
}

/// §5's "New session" row / §10.5: non-empty, no `:` or `.` (both are meaningful
/// in tmux target syntax), and unique among existing session names. `renaming`
/// excludes that session's own current name from the duplicate check.
fn validate_session_name(
    name: &str,
    sessions: &[Session],
    renaming: Option<&SessionId>,
) -> Result<(), String> {
    validate_non_empty(name)?;
    if name.contains(':') || name.contains('.') {
        return Err("name cannot contain ':' or '.'".to_string());
    }
    let duplicate = sessions
        .iter()
        .any(|s| s.name == name && Some(&s.id) != renaming);
    if duplicate {
        Err(format!("session '{name}' already exists"))
    } else {
        Ok(())
    }
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

    #[test]
    fn open_new_on_sessions_column_opens_empty_input_overlay() {
        let mut app = App::new(sample_snapshot());
        app.open_new();
        match &app.mode {
            Mode::Input(overlay) => {
                assert_eq!(overlay.kind, InputKind::NewSession);
                assert_eq!(overlay.text, "");
            }
            _ => panic!("expected Mode::Input"),
        }
    }

    #[test]
    fn open_rename_prefills_with_current_name() {
        let mut app = App::new(sample_snapshot());
        app.open_rename();
        match &app.mode {
            Mode::Input(overlay) => {
                assert_eq!(overlay.kind, InputKind::RenameSession(SessionId::new("$1")));
                assert_eq!(overlay.text, "main");
            }
            _ => panic!("expected Mode::Input"),
        }

        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.open_rename();
        match &app.mode {
            Mode::Input(overlay) => {
                assert_eq!(overlay.kind, InputKind::RenameWindow(WindowId::new("@1")));
                assert_eq!(overlay.text, "win0");
            }
            _ => panic!("expected Mode::Input"),
        }
    }

    #[test]
    fn open_kill_confirm_uses_plain_wording_for_ordinary_session() {
        let mut app = App::new(sample_snapshot());
        app.open_kill_confirm();
        match &app.mode {
            Mode::Confirm(overlay) => {
                assert_eq!(overlay.kind, ConfirmKind::KillSession(SessionId::new("$1")));
                assert!(overlay.message.contains("kill session 'main'?"));
            }
            _ => panic!("expected Mode::Confirm"),
        }
    }

    #[test]
    fn open_kill_confirm_warns_about_attached_client_session() {
        let mut snapshot = sample_snapshot();
        snapshot.client_session = Some(SessionId::new("$1"));
        let mut app = App::new(snapshot);
        app.open_kill_confirm();
        match &app.mode {
            Mode::Confirm(overlay) => {
                assert!(overlay.message.contains("attached"));
                assert!(overlay.message.contains("jump/detach"));
            }
            _ => panic!("expected Mode::Confirm"),
        }
    }

    #[test]
    fn open_kill_confirm_warns_about_last_session_regardless_of_attachment() {
        let mut snapshot = sample_snapshot();
        snapshot.sessions.truncate(1); // only $1 left
        let mut app = App::new(snapshot);
        app.open_kill_confirm();
        match &app.mode {
            Mode::Confirm(overlay) => {
                assert!(overlay.message.contains("last session"));
                assert!(overlay.message.contains("server will exit"));
            }
            _ => panic!("expected Mode::Confirm"),
        }
    }

    #[test]
    fn zoom_toggle_is_noop_outside_panes_column() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Sessions;
        // Should not panic or attempt a tmux call worth asserting on; the guard
        // is simply "did we even try". No selected_pane needed for it to no-op.
        app.toggle_zoom();
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn input_editing_and_cancel() {
        let mut app = App::new(sample_snapshot());
        app.open_new(); // Mode::Input(NewSession, "")
        app.input_char('a');
        app.input_char('b');
        match &app.mode {
            Mode::Input(overlay) => assert_eq!(overlay.text, "ab"),
            _ => panic!("expected Mode::Input"),
        }
        app.input_backspace();
        match &app.mode {
            Mode::Input(overlay) => assert_eq!(overlay.text, "a"),
            _ => panic!("expected Mode::Input"),
        }
        app.input_cancel();
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn confirm_no_returns_to_normal_without_acting() {
        let mut app = App::new(sample_snapshot());
        app.open_kill_confirm();
        app.confirm_no();
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn validate_session_name_rejects_empty_illegal_chars_and_duplicates() {
        let sessions = sample_snapshot().sessions;
        assert!(validate_session_name("", &sessions, None).is_err());
        assert!(validate_session_name("foo:bar", &sessions, None).is_err());
        assert!(validate_session_name("foo.bar", &sessions, None).is_err());
        assert!(validate_session_name("main", &sessions, None).is_err());
        assert!(validate_session_name("main", &sessions, Some(&SessionId::new("$1"))).is_ok());
        assert!(validate_session_name("brand-new", &sessions, None).is_ok());
    }

    // -- M2: move-mode / drag state machine ---------------------------------

    #[test]
    fn space_on_sessions_column_toasts_instead_of_dragging() {
        let mut app = App::new(sample_snapshot());
        app.enter_move_mode();
        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.toast.is_some());
    }

    #[test]
    fn space_on_windows_column_picks_up_the_selected_window() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode();
        match &app.mode {
            Mode::Dragging(drag) => assert_eq!(drag.item, DragItem::Window(WindowId::new("@1"))),
            _ => panic!("expected Mode::Dragging"),
        }
    }

    #[test]
    fn space_on_panes_column_picks_up_the_selected_pane() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Panes;
        app.enter_move_mode();
        match &app.mode {
            Mode::Dragging(drag) => assert_eq!(drag.item, DragItem::Pane(PaneId::new("%1"))),
            _ => panic!("expected Mode::Dragging"),
        }
    }

    #[test]
    fn cancel_drag_restores_origin_selection() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode();
        app.drag_move_focus(-1); // -> Sessions column
        app.drag_move_cursor(1); // -> $2 "dotfiles"
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$2");

        app.cancel_drag();
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.focus, Column::Windows);
        assert_eq!(app.selected_session.as_ref().unwrap().as_target(), "$1");
        assert_eq!(app.selected_window.as_ref().unwrap().as_target(), "@1");
    }

    #[test]
    fn window_drag_focus_never_reaches_panes_column() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode();
        app.drag_move_focus(1); // would be Panes in Normal mode's 3-column cycle
        assert_eq!(app.focus, Column::Sessions);
        app.drag_move_focus(1);
        assert_eq!(app.focus, Column::Windows);
    }

    #[test]
    fn pane_drag_focus_cycles_all_three_columns() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Panes;
        app.enter_move_mode();
        app.drag_move_focus(1);
        assert_eq!(app.focus, Column::Sessions);
        app.drag_move_focus(1);
        assert_eq!(app.focus, Column::Windows);
        app.drag_move_focus(1);
        assert_eq!(app.focus, Column::Panes);
    }

    #[test]
    fn sessions_cursor_extends_onto_new_session_pseudo_row_and_back() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode();
        app.drag_move_focus(-1); // Sessions column, on $1

        app.drag_move_cursor(1); // -> $2 (last real session)
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::SessionRow(SessionId::new("$2")))
        );

        app.drag_move_cursor(1); // -> pseudo row
        assert_eq!(app.resolve_drop_target(), Some(DropTarget::NewSessionRow));

        app.drag_move_cursor(1); // already at the end, no further movement
        assert_eq!(app.resolve_drop_target(), Some(DropTarget::NewSessionRow));

        app.drag_move_cursor(-1); // back onto $2
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::SessionRow(SessionId::new("$2")))
        );
    }

    #[test]
    fn window_gap_cursor_skips_gaps_bracketing_the_dragged_window() {
        // $1 "main" has windows @1 (index 0) and @2 (index 1); dragging @1.
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode(); // picks up @1, gap_index defaults to 1 (bracketing @1)

        // Gaps for 2 windows: 0 (before @1), 1 (between @1/@2), 2 (after @2).
        // Gaps 0 and 1 both bracket @1 and must be skipped; only gap 2 is real.
        app.drag_move_cursor(1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowGap {
                anchor: WindowId::new("@2"),
                after: true
            })
        );
        // Moving further down stays at the boundary (only one non-bracketing gap).
        app.drag_move_cursor(1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowGap {
                anchor: WindowId::new("@2"),
                after: true
            })
        );

        app.drag_move_cursor(-1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowGap {
                anchor: WindowId::new("@2"),
                after: true
            })
        );
    }

    #[test]
    fn window_gap_cursor_in_a_different_session_has_no_bracketing_skip() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode(); // picks up @1 from $1
        app.drag_move_focus(-1); // Sessions column
        app.drag_move_cursor(1); // -> $2 "dotfiles" (single window @3)
        app.drag_move_focus(1); // back into Windows column, viewing $2 now

        // $2 has exactly one window (@3): gaps are [before @3, after @3], neither
        // brackets @1 (which lives in $1), so both are real, valid targets.
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowGap {
                anchor: WindowId::new("@3"),
                after: true
            })
        );
        app.drag_move_cursor(-1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowGap {
                anchor: WindowId::new("@3"),
                after: false
            })
        );
    }

    #[test]
    fn pane_drag_windows_cursor_extends_onto_new_window_pseudo_row() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Panes;
        app.enter_move_mode(); // picks up %1 from window @1
        // Columns cycle [Sessions, Windows, Panes]; from Panes, -1 reaches
        // Windows directly (+1 would wrap around through Sessions instead).
        app.drag_move_focus(-1); // Windows column, viewing $1 (2 windows: @1, @2)

        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowRow(WindowId::new("@1")))
        );
        app.drag_move_cursor(1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::WindowRow(WindowId::new("@2")))
        );
        app.drag_move_cursor(1);
        assert_eq!(app.resolve_drop_target(), Some(DropTarget::NewWindowRow));
    }

    #[test]
    fn pane_drag_panes_cursor_has_no_pseudo_row() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Panes;
        app.enter_move_mode(); // picks up %1; %2 is the only other pane in @1
        app.drag_move_cursor(1);
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::PaneRow(PaneId::new("%2")))
        );
        app.drag_move_cursor(1); // no further real pane; stays put
        assert_eq!(
            app.resolve_drop_target(),
            Some(DropTarget::PaneRow(PaneId::new("%2")))
        );
    }

    #[test]
    fn committing_window_onto_new_session_row_opens_prefilled_input() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode();
        app.drag_move_focus(-1);
        app.drag_move_cursor(1); // $1 -> $2
        app.drag_move_cursor(1); // $2 -> new-session pseudo row
        app.commit_drag();

        match &app.mode {
            Mode::Input(overlay) => {
                assert_eq!(
                    overlay.kind,
                    InputKind::NewSessionFromDrag(DragItem::Window(WindowId::new("@1")))
                );
                assert_eq!(overlay.text, "win0"); // prefilled with the window's name
            }
            _ => panic!("expected Mode::Input"),
        }
    }

    #[test]
    fn refresh_silently_cancels_a_drag_whose_item_vanished() {
        let mut app = App::new(sample_snapshot());
        app.focus = Column::Windows;
        app.enter_move_mode(); // dragging @1
        assert!(matches!(app.mode, Mode::Dragging(_)));

        let mut shrunk = sample_snapshot();
        shrunk.sessions[0].windows.remove(0); // @1 is gone
        app.apply_refresh(shrunk);

        assert!(matches!(app.mode, Mode::Normal));
        assert!(app.toast.is_none()); // silent cancel, no toast (§10.10)
    }
}
