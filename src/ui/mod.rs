pub mod app;
pub mod columns;
pub mod overlays;
pub mod statusbar;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use app::{App, Mode};
use theme::Theme;

/// Top-level frame layout: header (1 row) / columns / footer (1 row) (§6.1), plus
/// any overlay/toast drawn on top.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    statusbar::render_header(frame, rows[0], app, theme);
    columns::render_columns(frame, rows[1], app, theme);
    statusbar::render_footer(frame, rows[2], app, theme);

    match &app.mode {
        Mode::Input(overlay) => overlays::render_input_overlay(frame, area, overlay, theme),
        Mode::Confirm(overlay) => overlays::render_confirm_overlay(frame, area, overlay, theme),
        Mode::Normal => {}
    }

    if let Some(toast) = &app.toast {
        // Sits just above the footer row (§6.6).
        let toast_rect = Rect::new(area.x, rows[2].y.saturating_sub(1), area.width, 1);
        overlays::render_toast(frame, toast_rect, toast, theme);
    }
}
