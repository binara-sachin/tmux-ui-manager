pub mod app;
pub mod columns;
pub mod statusbar;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use app::App;
use theme::Theme;

/// Top-level frame layout: header (1 row) / columns / footer (1 row) (§6.1).
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
    statusbar::render_footer(frame, rows[2], theme);
}
