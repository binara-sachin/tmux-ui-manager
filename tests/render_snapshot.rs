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
use tmux_ui_manager::ui::app::{App, Column, Mode};
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

/// A single session with `count` one-pane windows named `win0`, `win1`, ... —
/// enough to overflow a normal-height Windows column, for scroll/overflow
/// tests that need real clipping to exercise.
fn many_windows_snapshot(count: u32) -> Snapshot {
    let windows: Vec<Window> = (0..count)
        .map(|i| {
            window(
                &format!("@{i}"),
                i,
                &format!("win{i}"),
                vec![pane(&format!("%{i}"), 0, "zsh")],
            )
        })
        .collect();
    Snapshot {
        totals: Totals {
            sessions: 1,
            windows: count as usize,
            panes: count as usize,
        },
        client_session: None,
        sessions: vec![Session {
            id: SessionId::new("$1"),
            name: "work".to_string(),
            attached: true,
            windows,
        }],
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

/// Row/column of the first occurrence of `needle` in a `buffer_text` dump.
/// `str::find` returns a *byte* offset, but this codebase's rows mix
/// multi-byte box-drawing characters (`│`, `─`, ...) with ASCII — so the byte
/// offset has to be converted to a character count (each glyph here is
/// single-width) to land on the right terminal column.
fn find_text_position(text: &str, needle: &str) -> Option<(u16, u16)> {
    for (row, line) in text.lines().enumerate() {
        if let Some(byte_col) = line.find(needle) {
            let char_col = line[..byte_col].chars().count();
            return Some((char_col as u16, row as u16));
        }
    }
    None
}

#[test]
fn confirm_overlay_renders_clickable_buttons_and_clicking_no_cancels() {
    // §6.6: "mouse-clickable buttons". Only exercises the [n]o path — clicking
    // [y]es would run a real kill-session tmux call, which (like activate())
    // is reserved for the isolated-socket live tests, not plain unit tests.
    let mut app = App::new(sample_snapshot());
    let theme = Theme::default();
    app.open_kill_confirm();

    let text = buffer_text(&draw_once(&app, &theme));
    assert!(text.contains("[y]es"));
    assert!(text.contains("[n]o"));

    let (x, y) = find_text_position(&text, "[n]o").expect("the no button should be rendered");
    app.mouse_down(x, y);
    assert!(matches!(app.mode, Mode::Normal));
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
fn wheel_scroll_persists_across_a_redraw_of_unchanged_state() {
    // Regression test for a bug where the per-frame "keep the selection
    // visible" logic ran unconditionally, snapping any wheel-set scroll
    // offset straight back to wherever the (unchanged) selection was on
    // *every* render — a field-level assertion on the offset can't catch
    // this; it only shows up once you actually render twice.
    let mut app = App::new(many_windows_snapshot(30));
    app.focus = Column::Windows;
    let theme = Theme::default();

    // First render populates the hit-map/column-rects mouse_scroll needs.
    // Note: "win0" alone is a bad probe here — it's still the *selected*
    // window throughout, so its name also appears in the Panes column title
    // ("PANES · 0:win0") regardless of Windows-column scroll. Match the
    // Windows column's own row rendering ("0: win0", index-colon-space-name)
    // instead, which only appears when that row is actually visible.
    let first = buffer_text(&draw_once(&app, &theme));
    assert!(first.contains("0: win0"));

    // Scroll the (unfocused-by-mouse, but that's fine — scroll ignores
    // keyboard focus per §6.4) windows column down, well clear of the
    // still-selected win0.
    for _ in 0..8 {
        app.mouse_scroll(35, 10, 1);
    }

    let scrolled = buffer_text(&draw_once(&app, &theme));
    assert!(
        !scrolled.contains("0: win0"),
        "expected the wheel scroll to move win0's row out of view"
    );
    assert!(
        scrolled.contains("8: win8"),
        "expected the wheel scroll to reveal later windows"
    );

    // The regression: re-rendering with nothing else changed must NOT
    // snap back to reveal win0 again.
    let redrawn = buffer_text(&draw_once(&app, &theme));
    assert_eq!(
        scrolled, redrawn,
        "scroll position must survive a redraw with no new input"
    );
}

#[test]
fn keyboard_navigation_still_reveals_the_selection_after_a_wheel_scroll() {
    // The other half of the same invariant: a *real* navigation must still
    // pull the view back to the selection, even right after a manual scroll.
    let mut app = App::new(many_windows_snapshot(30));
    app.focus = Column::Windows;
    let theme = Theme::default();
    let _ = draw_once(&app, &theme);

    for _ in 0..8 {
        app.mouse_scroll(35, 10, 1);
    }
    let scrolled = buffer_text(&draw_once(&app, &theme));
    assert!(!scrolled.contains("0: win0"));

    app.jump_to_edge(false); // jump to the last window — a genuine selection change
    let revealed = buffer_text(&draw_once(&app, &theme));
    assert!(
        revealed.contains("29: win29"),
        "jumping to the last window must scroll it into view"
    );
}

#[test]
fn overflow_indicator_appears_only_while_the_column_is_actually_clipped() {
    let mut app = App::new(many_windows_snapshot(30));
    app.focus = Column::Windows;
    let theme = Theme::default();

    let idle = buffer_text(&draw_once(&app, &theme));
    assert!(
        idle.contains('\u{2026}'),
        "30 windows in a normal-height popup should overflow and show the … indicator"
    );

    // Scroll all the way to the bottom: nothing left below, indicator gone.
    for _ in 0..40 {
        app.mouse_scroll(35, 10, 1);
    }
    let bottomed_out = buffer_text(&draw_once(&app, &theme));
    assert!(
        !bottomed_out.contains('\u{2026}'),
        "once the last row is visible there is nothing left to indicate"
    );
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
