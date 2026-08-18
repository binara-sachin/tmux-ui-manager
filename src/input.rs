use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::ui::app::Mode;

/// Intent extracted from a raw crossterm event, independent of `App`'s internals.
/// Mode-dependent: the same physical key means different things while an overlay
/// is open (e.g. `n` types the letter 'n' into an input field rather than
/// triggering "new"). Mouse events (§6.4) are only meaningful in `Normal` and
/// `Dragging` mode; overlays ignore them (mouse-clickable confirm buttons are
/// a `M4` nicety, not in scope here).
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
    ConfirmMove,
    ConfirmActivate,
    EnterMoveMode,
    DragMoveFocus(i32),
    DragMoveCursor(i32),
    DragCommit,
    DragCancel,
    MouseMoved { x: u16, y: u16 },
    MouseDown { x: u16, y: u16 },
    MouseDragged { x: u16, y: u16 },
    MouseUp { x: u16, y: u16 },
    MouseScroll { x: u16, y: u16, delta: i32 },
    None,
}

pub fn translate(mode: &Mode, event: &Event) -> AppEvent {
    match event {
        Event::Key(key) => translate_key(mode, *key),
        Event::Mouse(mouse) => translate_mouse(mode, *mouse),
        Event::Resize(_, _) => AppEvent::Redraw,
        _ => AppEvent::None,
    }
}

fn translate_mouse(mode: &Mode, mouse: MouseEvent) -> AppEvent {
    // The input overlay (new/rename text entry) has no mouse affordances of
    // its own, so its mouse events are ignored rather than falling through to
    // the column beneath. The confirm overlay is the exception: §6.6 asks for
    // clickable [y]es/[n]o buttons, so its clicks flow through the same
    // MouseDown/MouseUp events — `App` resolves them against the overlay's
    // own button rects rather than the (covered, stale) column hit-map.
    if matches!(mode, Mode::Input(_)) {
        return AppEvent::None;
    }
    let (x, y) = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Moved => AppEvent::MouseMoved { x, y },
        MouseEventKind::Down(MouseButton::Left) => AppEvent::MouseDown { x, y },
        MouseEventKind::Drag(MouseButton::Left) => AppEvent::MouseDragged { x, y },
        MouseEventKind::Up(MouseButton::Left) => AppEvent::MouseUp { x, y },
        MouseEventKind::ScrollDown => AppEvent::MouseScroll { x, y, delta: 1 },
        MouseEventKind::ScrollUp => AppEvent::MouseScroll { x, y, delta: -1 },
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
        Mode::Dragging(_) => translate_dragging_key(key),
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
        KeyCode::Char(' ') | KeyCode::Char('m') => AppEvent::EnterMoveMode,
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
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Char('h')
        | KeyCode::Char('l')
        | KeyCode::Tab
        | KeyCode::BackTab => AppEvent::ConfirmMove,
        KeyCode::Enter => AppEvent::ConfirmActivate,
        _ => AppEvent::None,
    }
}

fn translate_dragging_key(key: KeyEvent) -> AppEvent {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => AppEvent::DragMoveCursor(-1),
        KeyCode::Down | KeyCode::Char('j') => AppEvent::DragMoveCursor(1),
        KeyCode::Left | KeyCode::Char('h') => AppEvent::DragMoveFocus(-1),
        KeyCode::Right | KeyCode::Char('l') => AppEvent::DragMoveFocus(1),
        KeyCode::Enter => AppEvent::DragCommit,
        KeyCode::Esc => AppEvent::DragCancel,
        _ => AppEvent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::overlays::{
        ConfirmButton, ConfirmKind, ConfirmOverlay, InputKind, InputOverlay,
    };
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
    fn confirm_mode_responds_to_y_n_esc_and_navigation() {
        let mode = Mode::Confirm(ConfirmOverlay {
            kind: ConfirmKind::KillPane(crate::tmux::ids::PaneId::new("%1")),
            message: String::new(),
            selected: ConfirmButton::No,
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
        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Char('h'),
            KeyCode::Char('l'),
            KeyCode::Tab,
            KeyCode::BackTab,
        ] {
            assert_eq!(translate_key(&mode, press(code)), AppEvent::ConfirmMove);
        }
        assert_eq!(
            translate_key(&mode, press(KeyCode::Enter)),
            AppEvent::ConfirmActivate
        );
    }
}
