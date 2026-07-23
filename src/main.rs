use std::io::{self, Stdout};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use tmux_ui_manager::input::{self, AppEvent};
use tmux_ui_manager::tmux::snapshot::take_snapshot;
use tmux_ui_manager::ui::{self, app::App, theme::Theme};

const TICK_RATE: Duration = Duration::from_secs(2);
/// §6.5 auto-scroll cadence: while dragging with the pointer parked on an
/// overflowing column's top/bottom row, scroll it once per this interval.
/// Driven by shortening the event loop's poll timeout rather than a separate
/// timer thread — a timeout with no event *is* the tick.
const AUTO_SCROLL_RATE: Duration = Duration::from_millis(150);

fn main() {
    if std::env::var_os("TMUX").is_none() {
        eprintln!("tmux-ui-manager: must run inside tmux");
        std::process::exit(1);
    }

    match tmux_version() {
        Some((major, minor)) if (major, minor) >= (3, 2) => {}
        Some((major, minor)) => {
            eprintln!("tmux-ui-manager: requires tmux >= 3.2, found {major}.{minor}");
            std::process::exit(1);
        }
        None => {
            eprintln!("tmux-ui-manager: could not determine tmux version (is tmux on PATH?)");
            std::process::exit(1);
        }
    }

    install_panic_hook();

    let mut terminal = match setup_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tmux-ui-manager: failed to initialize terminal: {e}");
            std::process::exit(1);
        }
    };

    let result = run(&mut terminal);

    teardown_terminal(&mut terminal);

    if let Err(e) = result {
        eprintln!("tmux-ui-manager: {e}");
        std::process::exit(1);
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = take_snapshot()?;
    let mut app = App::new(snapshot);
    let theme = Theme::default();

    loop {
        app.expire_toast();
        terminal.draw(|f| ui::draw(f, &app, &theme))?;

        if app.should_quit {
            return Ok(());
        }

        // Wait up to the tick interval for the first event; a timeout is itself
        // the periodic-refresh signal (§4.3) — unless a drag has the pointer
        // parked on an overflowing column's edge, in which case a much
        // shorter timeout drives the auto-scroll cadence instead.
        let wait = if app.wants_auto_scroll() {
            AUTO_SCROLL_RATE
        } else {
            TICK_RATE
        };
        if event::poll(wait)? {
            handle_event(&mut app, event::read()?);
            // Drain the rest of the batch before drawing again (§7 rule 1) — a
            // flood of mouse-move events must never queue up multiple renders.
            while event::poll(Duration::ZERO)? {
                handle_event(&mut app, event::read()?);
            }
        } else if app.wants_auto_scroll() {
            app.auto_scroll_tick();
        } else {
            app.apply_refresh(take_snapshot()?);
        }
    }
}

fn handle_event(app: &mut App, event: Event) {
    match input::translate(&app.mode, &event) {
        AppEvent::MoveSelection(delta) => app.move_selection(delta),
        AppEvent::MoveFocus(delta) => app.move_focus(delta),
        AppEvent::JumpEdge { top } => app.jump_to_edge(top),
        AppEvent::Activate => app.activate(),
        AppEvent::Quit => app.should_quit = true,
        AppEvent::NewContextual => app.open_new(),
        AppEvent::RenameContextual => app.open_rename(),
        AppEvent::KillContextual => app.open_kill_confirm(),
        AppEvent::ZoomContextual => app.toggle_zoom(),
        AppEvent::InputChar(c) => app.input_char(c),
        AppEvent::InputBackspace => app.input_backspace(),
        AppEvent::InputConfirm => app.input_confirm(),
        AppEvent::InputCancel => app.input_cancel(),
        AppEvent::ConfirmYes => app.confirm_yes(),
        AppEvent::ConfirmNo => app.confirm_no(),
        AppEvent::EnterMoveMode => app.enter_move_mode(),
        AppEvent::DragMoveFocus(delta) => app.drag_move_focus(delta),
        AppEvent::DragMoveCursor(delta) => app.drag_move_cursor(delta),
        AppEvent::DragCommit => app.commit_drag(),
        AppEvent::DragCancel => app.cancel_drag(),
        AppEvent::MouseMoved { x, y } => app.mouse_hover(x, y),
        AppEvent::MouseDown { x, y } => app.mouse_down(x, y),
        AppEvent::MouseDragged { x, y } => app.mouse_drag(x, y),
        AppEvent::MouseUp { x, y } => app.mouse_up(x, y),
        AppEvent::MouseScroll { x, y, delta } => app.mouse_scroll(x, y, delta),
        AppEvent::Redraw | AppEvent::None => {}
    }
}

fn tmux_version() -> Option<(u32, u32)> {
    let output = Command::new("tmux").arg("-V").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_tmux_version(&String::from_utf8_lossy(&output.stdout))
}

fn parse_tmux_version(raw: &str) -> Option<(u32, u32)> {
    let token = raw.split_whitespace().last()?;
    let mut parts = token.splitn(2, '.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor_str = parts.next()?;
    let minor_digits: String = minor_str
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let minor: u32 = minor_digits.parse().ok()?;
    Some((major, minor))
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

/// Restores the terminal to its pre-launch state. Idempotent-ish: called both on
/// normal exit and (via the panic hook) on panic, so a popup that dies never
/// leaves the user's real terminal stuck in mouse-report/alternate-screen mode
/// (§7 rule 5 — the single worst possible bug for this setup).
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
    restore_terminal();
    let _ = terminal.show_cursor();
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_version() {
        assert_eq!(parse_tmux_version("tmux 3.4\n"), Some((3, 4)));
    }

    #[test]
    fn parses_version_with_letter_suffix() {
        assert_eq!(parse_tmux_version("tmux 3.3a\n"), Some((3, 3)));
    }

    #[test]
    fn rejects_unparseable_output() {
        assert_eq!(parse_tmux_version("not tmux at all"), None);
    }
}
