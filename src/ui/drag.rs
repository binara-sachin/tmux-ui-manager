//! Drag state machine data types (§6.5) — the shared vocabulary between M2's
//! keyboard-driven target cursor and M3's mouse hit-map, so both resolve to the
//! same `DropTarget` and feed the same `plan_drop` mapping. Keeping the action
//! and its footer-sentence description (§6.5: "the primary safety mechanism")
//! derived from one match arm means they cannot diverge.

use crate::tmux::ids::{PaneId, SessionId, WindowId};

/// The window or pane picked up by `Space`/`m` (§6.5). Sessions aren't
/// draggable in v1 — picking one up shows a toast instead (§6.5 table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragItem {
    Window(WindowId),
    Pane(PaneId),
}

/// Where the drag cursor currently rests, resolved from column focus + selection
/// (§6.5's "target cursor" for keyboard, and later M3's resolved hit-map hit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    SessionRow(SessionId),
    /// Insert position within a session's window list: before `anchor`, or
    /// after it when `after` is true (used for the one "after the last window"
    /// position — every other gap is expressed as "before window i").
    WindowGap {
        anchor: WindowId,
        after: bool,
    },
    WindowRow(WindowId),
    PaneRow(PaneId),
    NewSessionRow,
    NewWindowRow,
}

/// What committing (Enter) at the current `DropTarget` would do. `NoOp` covers
/// every case the §6.5 table marks "not highlighted" — dropping a window on its
/// own session row, a pane on itself, a window-gap bracketing the window's own
/// current position, or any target not in the item's valid-target set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedAction {
    NoOp,
    MoveWindowToSession {
        window: WindowId,
        session: SessionId,
    },
    ReorderWindow {
        window: WindowId,
        anchor: WindowId,
        after: bool,
    },
    WindowToNewSession {
        window: WindowId,
    },
    JoinPaneIntoWindow {
        pane: PaneId,
        window: WindowId,
    },
    SplitPaneOntoPane {
        pane: PaneId,
        target: PaneId,
    },
    PaneToNewWindowInSession {
        pane: PaneId,
        session: SessionId,
    },
    PaneToNewSession {
        pane: PaneId,
    },
}

/// Whether dropping `dragged` at `WindowGap { anchor, after }` would actually
/// change anything, given the current order of the window list being viewed.
/// Both directions matter: gap `{anchor, after: false}` is a no-op when
/// `dragged` is already immediately before `anchor`; `{anchor, after: true}` is
/// a no-op when it's already immediately after. (Checking only "is the anchor
/// the dragged window itself" — the naive version of this — misses the second
/// case: a window's own default gap position sits *after* itself, not on it.)
/// General over both the keyboard cursor's index-based gaps and M3's
/// mouse-resolved anchor/after pairs, since it only needs list adjacency.
fn window_gap_is_noop(
    window_list: &[WindowId],
    dragged: &WindowId,
    anchor: &WindowId,
    after: bool,
) -> bool {
    if anchor == dragged {
        return true;
    }
    let Some(anchor_pos) = window_list.iter().position(|id| id == anchor) else {
        return false;
    };
    if after {
        window_list
            .get(anchor_pos + 1)
            .is_some_and(|id| id == dragged)
    } else {
        anchor_pos > 0 && &window_list[anchor_pos - 1] == dragged
    }
}

/// Pure mapping from (dragged item, drop target, context) to the action that
/// would run on commit. `origin_session` is the session the item started in
/// (needed to reject "append to its own session" as a no-op, §6.5); `own_window`
/// is the window a dragged pane already lives in (needed to reject "join into
/// its own window"); `window_list` is the ordered ids of whatever session's
/// windows are currently in view (needed for `window_gap_is_noop`).
pub fn plan_drop(
    item: &DragItem,
    target: &DropTarget,
    origin_session: Option<&SessionId>,
    own_window: Option<&WindowId>,
    window_list: &[WindowId],
) -> PlannedAction {
    match (item, target) {
        (DragItem::Window(w), DropTarget::SessionRow(s)) => {
            if Some(s) == origin_session {
                PlannedAction::NoOp
            } else {
                PlannedAction::MoveWindowToSession {
                    window: w.clone(),
                    session: s.clone(),
                }
            }
        }
        (DragItem::Window(w), DropTarget::WindowGap { anchor, after }) => {
            if window_gap_is_noop(window_list, w, anchor, *after) {
                PlannedAction::NoOp
            } else {
                PlannedAction::ReorderWindow {
                    window: w.clone(),
                    anchor: anchor.clone(),
                    after: *after,
                }
            }
        }
        (DragItem::Window(w), DropTarget::NewSessionRow) => {
            PlannedAction::WindowToNewSession { window: w.clone() }
        }
        (DragItem::Window(_), _) => PlannedAction::NoOp,

        (DragItem::Pane(p), DropTarget::WindowRow(w)) => {
            if own_window == Some(w) {
                PlannedAction::NoOp
            } else {
                PlannedAction::JoinPaneIntoWindow {
                    pane: p.clone(),
                    window: w.clone(),
                }
            }
        }
        (DragItem::Pane(p), DropTarget::PaneRow(target_pane)) => {
            if target_pane == p {
                PlannedAction::NoOp
            } else {
                PlannedAction::SplitPaneOntoPane {
                    pane: p.clone(),
                    target: target_pane.clone(),
                }
            }
        }
        (DragItem::Pane(p), DropTarget::SessionRow(s)) => PlannedAction::PaneToNewWindowInSession {
            pane: p.clone(),
            session: s.clone(),
        },
        (DragItem::Pane(p), DropTarget::NewWindowRow) => match origin_session {
            Some(s) => PlannedAction::PaneToNewWindowInSession {
                pane: p.clone(),
                session: s.clone(),
            },
            None => PlannedAction::NoOp,
        },
        (DragItem::Pane(p), DropTarget::NewSessionRow) => {
            PlannedAction::PaneToNewSession { pane: p.clone() }
        }
        (DragItem::Pane(_), DropTarget::WindowGap { .. }) => PlannedAction::NoOp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wid(s: &str) -> WindowId {
        WindowId::new(s.to_string())
    }
    fn pid(s: &str) -> PaneId {
        PaneId::new(s.to_string())
    }
    fn sid(s: &str) -> SessionId {
        SessionId::new(s.to_string())
    }

    // @1, @2, @3 in that order — the window list backing most gap tests below.
    fn three_windows() -> Vec<WindowId> {
        vec![wid("@1"), wid("@2"), wid("@3")]
    }

    #[test]
    fn window_onto_own_session_is_noop() {
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::SessionRow(sid("$1")),
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn window_onto_other_session_moves() {
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::SessionRow(sid("$2")),
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(
            action,
            PlannedAction::MoveWindowToSession {
                window: wid("@1"),
                session: sid("$2")
            }
        );
    }

    #[test]
    fn window_onto_own_gap_anchor_is_noop() {
        // Gap { anchor: @1, after: false } literally means "insert @1 before @1".
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::WindowGap {
                anchor: wid("@1"),
                after: false,
            },
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn window_onto_the_gap_immediately_after_its_own_current_position_is_noop() {
        // List is [@1, @2, @3]; dragging @1. Gap { anchor: @2, after: false }
        // ("insert @1 before @2") changes nothing, since @1 is already right
        // before @2 — this is the bracketing case identity-only checks miss.
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::WindowGap {
                anchor: wid("@2"),
                after: false,
            },
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn window_onto_the_gap_immediately_before_its_own_current_position_is_noop() {
        // Dragging @2 (sits between @1 and @3): Gap { anchor: @1, after: true }
        // ("insert @2 after @1") is also a no-op for the same reason, mirrored.
        let action = plan_drop(
            &DragItem::Window(wid("@2")),
            &DropTarget::WindowGap {
                anchor: wid("@1"),
                after: true,
            },
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn window_onto_a_real_gap_reorders() {
        // Dragging @1 onto "after @3" is a genuine move (not adjacent to @1's
        // current spot at all).
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::WindowGap {
                anchor: wid("@3"),
                after: true,
            },
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(
            action,
            PlannedAction::ReorderWindow {
                window: wid("@1"),
                anchor: wid("@3"),
                after: true
            }
        );
    }

    #[test]
    fn window_onto_gap_in_a_list_without_the_anchor_is_not_a_noop() {
        // Dragging @1 while viewing a *different* session's window list that
        // doesn't even contain @1 — every gap there is a real target.
        let other_session_windows = vec![wid("@9")];
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::WindowGap {
                anchor: wid("@9"),
                after: false,
            },
            Some(&sid("$1")),
            None,
            &other_session_windows,
        );
        assert_eq!(
            action,
            PlannedAction::ReorderWindow {
                window: wid("@1"),
                anchor: wid("@9"),
                after: false
            }
        );
    }

    #[test]
    fn window_onto_new_session_row_uses_recipe() {
        let action = plan_drop(
            &DragItem::Window(wid("@1")),
            &DropTarget::NewSessionRow,
            Some(&sid("$1")),
            None,
            &three_windows(),
        );
        assert_eq!(
            action,
            PlannedAction::WindowToNewSession { window: wid("@1") }
        );
    }

    #[test]
    fn window_onto_window_row_or_pane_row_is_noop() {
        assert_eq!(
            plan_drop(
                &DragItem::Window(wid("@1")),
                &DropTarget::WindowRow(wid("@2")),
                Some(&sid("$1")),
                None,
                &three_windows(),
            ),
            PlannedAction::NoOp
        );
        assert_eq!(
            plan_drop(
                &DragItem::Window(wid("@1")),
                &DropTarget::PaneRow(pid("%1")),
                Some(&sid("$1")),
                None,
                &three_windows(),
            ),
            PlannedAction::NoOp
        );
    }

    #[test]
    fn pane_onto_own_window_is_noop() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::WindowRow(wid("@1")),
            None,
            Some(&wid("@1")),
            &[],
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn pane_onto_other_window_joins() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::WindowRow(wid("@2")),
            None,
            Some(&wid("@1")),
            &[],
        );
        assert_eq!(
            action,
            PlannedAction::JoinPaneIntoWindow {
                pane: pid("%1"),
                window: wid("@2")
            }
        );
    }

    #[test]
    fn pane_onto_itself_is_noop() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::PaneRow(pid("%1")),
            None,
            None,
            &[],
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn pane_onto_other_pane_splits() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::PaneRow(pid("%2")),
            None,
            None,
            &[],
        );
        assert_eq!(
            action,
            PlannedAction::SplitPaneOntoPane {
                pane: pid("%1"),
                target: pid("%2")
            }
        );
    }

    #[test]
    fn pane_onto_session_row_breaks_into_new_window() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::SessionRow(sid("$2")),
            None,
            None,
            &[],
        );
        assert_eq!(
            action,
            PlannedAction::PaneToNewWindowInSession {
                pane: pid("%1"),
                session: sid("$2")
            }
        );
    }

    #[test]
    fn pane_onto_new_window_row_uses_currently_viewed_session() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::NewWindowRow,
            Some(&sid("$1")),
            None,
            &[],
        );
        assert_eq!(
            action,
            PlannedAction::PaneToNewWindowInSession {
                pane: pid("%1"),
                session: sid("$1")
            }
        );
    }

    #[test]
    fn pane_onto_new_window_row_without_a_viewed_session_is_noop() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::NewWindowRow,
            None,
            None,
            &[],
        );
        assert_eq!(action, PlannedAction::NoOp);
    }

    #[test]
    fn pane_onto_new_session_row_uses_recipe() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::NewSessionRow,
            None,
            None,
            &[],
        );
        assert_eq!(action, PlannedAction::PaneToNewSession { pane: pid("%1") });
    }

    #[test]
    fn pane_onto_window_gap_is_noop() {
        let action = plan_drop(
            &DragItem::Pane(pid("%1")),
            &DropTarget::WindowGap {
                anchor: wid("@1"),
                after: false,
            },
            None,
            None,
            &[],
        );
        assert_eq!(action, PlannedAction::NoOp);
    }
}
