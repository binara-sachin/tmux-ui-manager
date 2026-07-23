use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Intent extracted from a raw crossterm event, independent of `App`'s internals.
/// Mouse events are threaded through as `AppEvent::None` until M3 wires them up;
/// mouse capture is enabled from M0 so the panic-hook teardown has something to
/// undo from day one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    MoveSelection(i32),
    MoveFocus(i32),
    JumpEdge { top: bool },
    Activate,
    Quit,
    Redraw,
    None,
}

pub fn translate(event: &Event) -> AppEvent {
    match event {
        Event::Key(key) => translate_key(*key),
        Event::Resize(_, _) => AppEvent::Redraw,
        _ => AppEvent::None,
    }
}

fn translate_key(key: KeyEvent) -> AppEvent {
    if key.kind == KeyEventKind::Release {
        return AppEvent::None;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => AppEvent::MoveSelection(-1),
        KeyCode::Down | KeyCode::Char('j') => AppEvent::MoveSelection(1),
        KeyCode::Left | KeyCode::Char('h') => AppEvent::MoveFocus(-1),
        KeyCode::Right | KeyCode::Char('l') => AppEvent::MoveFocus(1),
        KeyCode::Tab => AppEvent::MoveFocus(1),
        KeyCode::BackTab => AppEvent::MoveFocus(-1),
        KeyCode::Enter => AppEvent::Activate,
        KeyCode::Char('g') => AppEvent::JumpEdge { top: true },
        KeyCode::Char('G') => AppEvent::JumpEdge { top: false },
        KeyCode::Char('q') | KeyCode::Esc => AppEvent::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => AppEvent::Quit,
        _ => AppEvent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn hjkl_map_to_selection_and_focus() {
        assert_eq!(
            translate_key(press(KeyCode::Char('j'))),
            AppEvent::MoveSelection(1)
        );
        assert_eq!(
            translate_key(press(KeyCode::Char('k'))),
            AppEvent::MoveSelection(-1)
        );
        assert_eq!(
            translate_key(press(KeyCode::Char('h'))),
            AppEvent::MoveFocus(-1)
        );
        assert_eq!(
            translate_key(press(KeyCode::Char('l'))),
            AppEvent::MoveFocus(1)
        );
    }

    #[test]
    fn enter_activates_and_q_quits() {
        assert_eq!(translate_key(press(KeyCode::Enter)), AppEvent::Activate);
        assert_eq!(translate_key(press(KeyCode::Char('q'))), AppEvent::Quit);
        assert_eq!(translate_key(press(KeyCode::Esc)), AppEvent::Quit);
    }

    #[test]
    fn key_release_events_are_ignored() {
        let mut key = press(KeyCode::Char('j'));
        key.kind = KeyEventKind::Release;
        assert_eq!(translate_key(key), AppEvent::None);
    }
}
