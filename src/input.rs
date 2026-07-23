use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::ui::app::Mode;

/// Intent extracted from a raw crossterm event, independent of `App`'s internals.
/// Mode-dependent: the same physical key means different things while an overlay
/// is open (e.g. `n` types the letter 'n' into an input field rather than
/// triggering "new"). Mouse events are threaded through as `AppEvent::None` until
/// M3 wires them up; mouse capture is enabled from M0 so the panic-hook teardown
/// has something to undo from day one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    MoveSelection(i32),
    MoveFocus(i32),
    JumpEdge { top: bool },
    Activate,
    Quit,
    Redraw,
    NewContextual,
    RenameContextual,
    KillContextual,
    ZoomContextual,
    InputChar(char),
    InputBackspace,
    InputConfirm,
    InputCancel,
    ConfirmYes,
    ConfirmNo,
    None,
}

pub fn translate(mode: &Mode, event: &Event) -> AppEvent {
    match event {
        Event::Key(key) => translate_key(mode, *key),
        Event::Resize(_, _) => AppEvent::Redraw,
        _ => AppEvent::None,
    }
}

fn translate_key(mode: &Mode, key: KeyEvent) -> AppEvent {
    if key.kind == KeyEventKind::Release {
        return AppEvent::None;
    }
    match mode {
        Mode::Normal => translate_normal_key(key),
        Mode::Input(_) => translate_input_key(key),
        Mode::Confirm(_) => translate_confirm_key(key),
    }
}

fn translate_normal_key(key: KeyEvent) -> AppEvent {
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
        KeyCode::Char('n') => AppEvent::NewContextual,
        KeyCode::Char('r') => AppEvent::RenameContextual,
        KeyCode::Char('x') => AppEvent::KillContextual,
        KeyCode::Char('z') => AppEvent::ZoomContextual,
        KeyCode::Char('q') | KeyCode::Esc => AppEvent::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => AppEvent::Quit,
        _ => AppEvent::None,
    }
}

fn translate_input_key(key: KeyEvent) -> AppEvent {
    match key.code {
        KeyCode::Enter => AppEvent::InputConfirm,
        KeyCode::Esc => AppEvent::InputCancel,
        KeyCode::Backspace => AppEvent::InputBackspace,
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            AppEvent::InputChar(c)
        }
        _ => AppEvent::None,
    }
}

fn translate_confirm_key(key: KeyEvent) -> AppEvent {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => AppEvent::ConfirmYes,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => AppEvent::ConfirmNo,
        _ => AppEvent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::overlays::{ConfirmKind, ConfirmOverlay, InputKind, InputOverlay};
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
            translate_key(&Mode::Normal, press(KeyCode::Char('j'))),
            AppEvent::MoveSelection(1)
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('k'))),
            AppEvent::MoveSelection(-1)
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('h'))),
            AppEvent::MoveFocus(-1)
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('l'))),
            AppEvent::MoveFocus(1)
        );
    }

    #[test]
    fn enter_activates_and_q_quits() {
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Enter)),
            AppEvent::Activate
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('q'))),
            AppEvent::Quit
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Esc)),
            AppEvent::Quit
        );
    }

    #[test]
    fn key_release_events_are_ignored() {
        let mut key = press(KeyCode::Char('j'));
        key.kind = KeyEventKind::Release;
        assert_eq!(translate_key(&Mode::Normal, key), AppEvent::None);
    }

    #[test]
    fn normal_mode_contextual_keys() {
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('n'))),
            AppEvent::NewContextual
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('r'))),
            AppEvent::RenameContextual
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('x'))),
            AppEvent::KillContextual
        );
        assert_eq!(
            translate_key(&Mode::Normal, press(KeyCode::Char('z'))),
            AppEvent::ZoomContextual
        );
    }

    #[test]
    fn input_mode_treats_shortcut_letters_as_text() {
        let mode = Mode::Input(InputOverlay::new(InputKind::NewSession, ""));
        assert_eq!(
            translate_key(&mode, press(KeyCode::Char('n'))),
            AppEvent::InputChar('n')
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Backspace)),
            AppEvent::InputBackspace
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Enter)),
            AppEvent::InputConfirm
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Esc)),
            AppEvent::InputCancel
        );
    }

    #[test]
    fn confirm_mode_only_responds_to_y_n_esc() {
        let mode = Mode::Confirm(ConfirmOverlay {
            kind: ConfirmKind::KillPane(crate::tmux::ids::PaneId::new("%1")),
            message: String::new(),
        });
        assert_eq!(
            translate_key(&mode, press(KeyCode::Char('y'))),
            AppEvent::ConfirmYes
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Char('n'))),
            AppEvent::ConfirmNo
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Esc)),
            AppEvent::ConfirmNo
        );
        assert_eq!(
            translate_key(&mode, press(KeyCode::Char('x'))),
            AppEvent::None
        );
    }
}
