//! Render snapshot tests (§11.3): "render known model states (idle, dragging,
//! confirm overlay, too-small) and assert buffer contents. This is the
//! glitch-regression net." Uses ratatui's `TestBackend` so no real terminal is
//! needed. Buffer equality across two draws of the *same* unchanged app state
//! is the concrete, checkable form of §7's "no flicker / no animation" rule —
//! if rendering is deterministic, ratatui's cell-diffing double buffer will
//! see zero changed cells and write nothing to the real backend.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use tmux_ui_manager::model::{Pane, Session, Snapshot, Totals, Window};
use tmux_ui_manager::tmux::ids::{PaneId, SessionId, WindowId};
use tmux_ui_manager::ui::app::{App, Column};
use tmux_ui_manager::ui::theme::Theme;
use tmux_ui_manager::ui::{self};

fn pane(id: &str, index: u32, command: &str) -> Pane {
    Pane {
        id: PaneId::new(id.to_string()),
        index,
        active: index == 0,
        command: command.to_string(),
        path: "/home/binara/dev/api-server".to_string(),
        title: String::new(),
        zoomed: false,
    }
}

fn window(id: &str, index: u32, name: &str, panes: Vec<Pane>) -> Window {
    Window {
        id: WindowId::new(id.to_string()),
        index,
        name: name.to_string(),
        active: index == 0,
        layout: String::new(),
        panes,
    }
}

fn sample_snapshot() -> Snapshot {
    let s1 = Session {
        id: SessionId::new("$1"),
        name: "main".to_string(),
        attached: true,
        windows: vec![
            window(
                "@1",
                0,
                "editor",
                vec![pane("%1", 0, "nvim"), pane("%2", 1, "node")],
            ),
            window("@2", 1, "shell", vec![pane("%3", 0, "zsh")]),
        ],
    };
    let s2 = Session {
        id: SessionId::new("$2"),
        name: "dotfiles".to_string(),
        attached: false,
        windows: vec![window("@3", 0, "vim", vec![pane("%4", 0, "vim")])],
    };
    Snapshot {
        totals: Totals {
            sessions: 2,
            windows: 3,
            panes: 4,
        },
        client_session: Some(SessionId::new("$1")),
        sessions: vec![s1, s2],
    }
}

fn draw_once(app: &App, theme: &Theme) -> Buffer {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal");
    terminal
        .draw(|f| ui::draw(f, app, theme))
        .expect("draw should not shell out or panic");
    terminal.backend().buffer().clone()
}

fn buffer_text(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn idle_render_shows_the_three_columns_and_their_contents() {
    let app = App::new(sample_snapshot());
    let theme = Theme::default();
    let text = buffer_text(&draw_once(&app, &theme));

    assert!(text.contains("SESSIONS"));
    assert!(text.contains("WINDOWS"));
    assert!(text.contains("PANES"));
    assert!(text.contains("main"));
    assert!(text.contains("dotfiles"));
    assert!(text.contains("editor"));
    assert!(text.contains("nvim"));
    assert!(text.contains("new session"));
    assert!(text.contains("new window"));
    assert!(text.contains("split pane"));
    // Footer shows the normal keyboard hints, not a drag sentence.
    assert!(text.contains("attach"));
    assert!(!text.contains("drop:"));
}

#[test]
fn dragging_render_shows_the_picked_up_hint_and_footer_sentence() {
    let mut app = App::new(sample_snapshot());
    let theme = Theme::default();
    app.focus = Column::Windows;
    app.enter_move_mode(); // picks up window @1 ("editor")

    let text = buffer_text(&draw_once(&app, &theme));

    assert!(text.contains("drop on target"));
    assert!(text.contains("drop:"));
    assert!(text.contains("cancel"));
}

#[test]
fn identical_consecutive_renders_of_unchanged_state_are_pixel_for_pixel_equal() {
    // The property the render-discipline rules (§7) actually depend on:
    // nothing in the render path introduces per-frame nondeterminism (a
    // timestamp, a counter, anything jitter-like) that would make ratatui's
    // double-buffer diffing see spurious changes — and thus spurious writes —
    // when the app state hasn't changed at all.
    let app = App::new(sample_snapshot());
    let theme = Theme::default();

    let first = draw_once(&app, &theme);
    let second = draw_once(&app, &theme);
    assert_eq!(first, second);
    assert!(first.diff(&second).is_empty());
}

#[test]
fn identical_consecutive_renders_while_dragging_are_also_stable() {
    let mut app = App::new(sample_snapshot());
    let theme = Theme::default();
    app.focus = Column::Panes;
    app.enter_move_mode(); // picks up pane %1

    let first = draw_once(&app, &theme);
    let second = draw_once(&app, &theme);
    assert_eq!(first, second);
    assert!(first.diff(&second).is_empty());
}
