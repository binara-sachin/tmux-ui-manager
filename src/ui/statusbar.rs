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
/// While dragging, the live "what would happen" sentence is the primary safety
/// mechanism (§6.5) — it's derived from `App::describe_planned_action`, the
/// same value `commit_drag` acts on, so they can't diverge. Mouse/hover hints
/// land in M3.
pub fn render_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let hint = match &app.mode {
        Mode::Normal => {
            "\u{21b5} attach  n new  r rename  x kill  z zoom  space/m move   \u{2191}\u{2193}/jk   \u{2190}\u{2192}/hl \u{b7} tab focus   g/G top/bottom   q quit"
                .to_string()
        }
        Mode::Input(_) => "\u{21b5} confirm   Esc cancel".to_string(),
        Mode::Confirm(_) => {
            "\u{2190}\u{2192}/hl/tab move   \u{21b5} activate   y yes   n/Esc no".to_string()
        }
        Mode::Dragging(_) => {
            let action = app.plan_current_drop();
            match app.describe_planned_action(&action) {
                Some(desc) => format!("drop: {desc}   \u{b7}   Esc cancel"),
                None => "drop: (no-op here)   \u{b7}   Esc cancel".to_string(),
            }
        }
    };
    let line = Line::from(Span::styled(hint, Style::default().fg(theme.meta())));
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme.bg())),
        area,
    );
}
