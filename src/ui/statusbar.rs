use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::ui::app::{App, Mode};
use crate::ui::theme::Theme;

/// Header row: title + live totals (§6.1).
pub fn render_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let totals = app.snapshot.totals;
    let left = "tmux :: manager".to_string();
    let right = format!(
        "{} sessions \u{b7} {} windows \u{b7} {} panes",
        totals.sessions, totals.windows, totals.panes
    );
    let pad = (area.width as usize).saturating_sub(left.chars().count() + right.chars().count());
    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(theme.fg())),
        Span::raw(" ".repeat(pad.max(1))),
        Span::styled(right, Style::default().fg(theme.meta())),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.bg())),
        area,
    );
}

/// Footer row: context-sensitive key hints (§6.1) — switches with `app.mode`.
/// Move-mode/drag hints are added in M2/M3.
pub fn render_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let hint = match &app.mode {
        Mode::Normal => {
            "\u{21b5} attach  n new  r rename  x kill  z zoom   \u{2191}\u{2193}/jk move   \u{2190}\u{2192}/hl \u{b7} tab focus   g/G top/bottom   q quit"
        }
        Mode::Input(_) => "\u{21b5} confirm   Esc cancel",
        Mode::Confirm(_) => "y yes   n/Esc no",
    };
    let line = Line::from(Span::styled(hint, Style::default().fg(theme.meta())));
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.bg())),
        area,
    );
}
