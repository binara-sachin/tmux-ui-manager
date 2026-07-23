//! Mouse hit-map (§6.4): "every rendered row registers its `Rect` + entity
//! reference in a hit-map rebuilt each frame... hit-testing is a linear scan —
//! the list is tiny." `ClickTarget` is `DropTarget` (§6.5/`ui::drag`) plus one
//! click-only variant (`NewSplitRow`, never a valid drag target); the overlap
//! is deliberate — most rows serve double duty as both a click destination in
//! `Mode::Normal` and a drop target while `Mode::Dragging`.

use ratatui::layout::Rect;

use crate::tmux::ids::{PaneId, SessionId, WindowId};
use crate::ui::drag::DropTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
    NewSessionRow,
    NewWindowRow,
    /// "+ split pane" pseudo-row — click-to-create only, never a drop target
    /// (dragging a pane onto it isn't in the §6.5 valid-target table).
    NewSplitRow,
    /// A window-reorder insertion line; only present in the hit-map while a
    /// window drag is in progress.
    WindowGap {
        anchor: WindowId,
        after: bool,
    },
}

impl ClickTarget {
    pub fn as_drop_target(&self) -> Option<DropTarget> {
        Some(match self {
            ClickTarget::Session(id) => DropTarget::SessionRow(id.clone()),
            ClickTarget::Window(id) => DropTarget::WindowRow(id.clone()),
            ClickTarget::Pane(id) => DropTarget::PaneRow(id.clone()),
            ClickTarget::NewSessionRow => DropTarget::NewSessionRow,
            ClickTarget::NewWindowRow => DropTarget::NewWindowRow,
            ClickTarget::WindowGap { anchor, after } => DropTarget::WindowGap {
                anchor: anchor.clone(),
                after: *after,
            },
            ClickTarget::NewSplitRow => return None,
        })
    }
}

pub type HitMap = Vec<(Rect, ClickTarget)>;

pub fn rect_contains(rect: &Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Linear scan per §6.4 — the row list is a handful of items at most.
pub fn hit_test(map: &HitMap, x: u16, y: u16) -> Option<ClickTarget> {
    map.iter()
        .find(|(rect, _)| rect_contains(rect, x, y))
        .map(|(_, target)| target.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wid(s: &str) -> WindowId {
        WindowId::new(s.to_string())
    }

    #[test]
    fn hit_test_finds_the_containing_rect() {
        let map: HitMap = vec![
            (Rect::new(0, 0, 10, 1), ClickTarget::NewSessionRow),
            (Rect::new(0, 1, 10, 1), ClickTarget::Window(wid("@1"))),
        ];
        assert_eq!(hit_test(&map, 3, 1), Some(ClickTarget::Window(wid("@1"))));
        assert_eq!(hit_test(&map, 3, 0), Some(ClickTarget::NewSessionRow));
    }

    #[test]
    fn hit_test_misses_outside_every_rect() {
        let map: HitMap = vec![(Rect::new(0, 0, 10, 1), ClickTarget::NewSessionRow)];
        assert_eq!(hit_test(&map, 3, 5), None);
        assert_eq!(hit_test(&map, 20, 0), None);
    }

    #[test]
    fn rect_contains_excludes_the_far_edge() {
        let rect = Rect::new(5, 5, 3, 2);
        assert!(rect_contains(&rect, 5, 5));
        assert!(rect_contains(&rect, 7, 6));
        assert!(!rect_contains(&rect, 8, 6));
        assert!(!rect_contains(&rect, 5, 7));
    }
}
