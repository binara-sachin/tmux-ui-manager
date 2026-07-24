pub mod app;
pub mod columns;
pub mod drag;
pub mod hitmap;
pub mod overlays;
pub mod statusbar;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph};

use app::{App, Mode};
use theme::Theme;

/// §10.7: "very small popup... below 70x15 render a centered 'window too
/// small' notice instead of a broken layout."
const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 15;

/// Top-level frame layout: header (1 row) / columns / footer (1 row) (§6.1), plus
/// any overlay/toast drawn on top.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area, theme);
        return;
    }

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
        Mode::Confirm(overlay) => {
            overlays::render_confirm_overlay(frame, area, overlay, theme, app)
        }
        // Dragging has no separate overlay — the picked-up row, valid-target
        // highlighting, and insertion line are drawn inline by columns.rs.
        Mode::Normal | Mode::Dragging(_) => {}
    }

    if let Some(toast) = &app.toast {
        // Sits just above the footer row (§6.6).
        let toast_rect = Rect::new(area.x, rows[2].y.saturating_sub(1), area.width, 1);
        overlays::render_toast(frame, toast_rect, toast, theme);
    }
}

fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg())),
        area,
    );

    if area.height == 0 {
        return;
    }
    let message = "window too small";
    let width = (message.chars().count() as u16).min(area.width);
    let rect = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + area.height / 2,
        width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(message, Style::default().fg(theme.fg())))
            .style(Style::default().bg(theme.bg())),
        rect,
    );
}
