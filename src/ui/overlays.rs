use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tmux::ids::{PaneId, SessionId, WindowId};
use crate::ui::app::App;
use crate::ui::drag::DragItem;
use crate::ui::theme::Theme;

const TOAST_LIFETIME: Duration = Duration::from_secs(3);

/// What an [`InputOverlay`] is editing, and the target it will act on when confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    NewSession,
    RenameSession(SessionId),
    RenameWindow(WindowId),
    RenamePaneTitle(PaneId),
    /// Committed a drag onto "+ new session" (§6.5) — carries the dragged item
    /// so the new-session recipe can run once the name is confirmed.
    NewSessionFromDrag(DragItem),
}

impl InputKind {
    pub fn label(&self) -> &'static str {
        match self {
            InputKind::NewSession | InputKind::NewSessionFromDrag(_) => "new session name",
            InputKind::RenameSession(_) => "rename session",
            InputKind::RenameWindow(_) => "rename window",
            InputKind::RenamePaneTitle(_) => "pane title",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputOverlay {
    pub kind: InputKind,
    pub text: String,
    pub error: Option<String>,
}

impl InputOverlay {
    pub fn new(kind: InputKind, prefill: impl Into<String>) -> Self {
        Self {
            kind,
            text: prefill.into(),
            error: None,
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.text.push(c);
        self.error = None;
    }

    pub fn backspace(&mut self) {
        self.text.pop();
        self.error = None;
    }
}

/// What a [`ConfirmOverlay`] will do on `y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    KillSession(SessionId),
    KillWindow(WindowId),
    KillPane(PaneId),
}

#[derive(Debug, Clone)]
pub struct ConfirmOverlay {
    pub kind: ConfirmKind,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub is_error: bool,
    created_at: Instant,
}

impl Toast {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
            created_at: Instant::now(),
        }
    }

    pub fn expired(&self) -> bool {
        self.created_at.elapsed() >= TOAST_LIFETIME
    }
}

/// Centers a `width`x`height` box within `area`, clamped so it never exceeds it.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

pub fn render_input_overlay(frame: &mut Frame, area: Rect, overlay: &InputOverlay, theme: &Theme) {
    let rect = centered_rect(50, 5, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", overlay.kind.label()),
            Style::default()
                .fg(theme.panel_title())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(rect);
    frame.render_widget(block.style(Style::default().bg(theme.base)), rect);

    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.meta())),
        Span::styled(overlay.text.clone(), Style::default().fg(theme.fg())),
    ]);
    let error_line = overlay
        .error
        .as_ref()
        .map(|e| Line::from(Span::styled(e.clone(), Style::default().fg(theme.danger()))))
        .unwrap_or_default();

    frame.render_widget(
        Paragraph::new(vec![input_line, error_line]).style(Style::default().bg(theme.base)),
        inner,
    );
}

/// `[y]es`/`[n]o` are plain-text spans, not widgets — their clickable rects
/// (§6.6: "mouse-clickable buttons") are derived by hand here from the exact
/// same layout used to draw them (first line's leading spans), then handed to
/// `App` so a click can be resolved without re-deriving this math elsewhere.
const YES_LABEL: &str = "[y]es";
const NO_LABEL: &str = "[n]o";
const BUTTON_GAP: u16 = 2; // the "  " between the two labels

pub fn render_confirm_overlay(
    frame: &mut Frame,
    area: Rect,
    overlay: &ConfirmOverlay,
    theme: &Theme,
    app: &App,
) {
    let rect = centered_rect(50, 4, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));
    let inner = block.inner(rect);
    frame.render_widget(block.style(Style::default().bg(theme.base)), rect);

    let message_line = Line::from(Span::styled(
        overlay.message.clone(),
        Style::default().fg(theme.fg()),
    ));
    let buttons_line = Line::from(vec![
        Span::styled(YES_LABEL, Style::default().fg(theme.danger())),
        Span::raw(" ".repeat(BUTTON_GAP as usize)),
        Span::styled(NO_LABEL, Style::default().fg(theme.meta())),
    ]);

    frame.render_widget(
        Paragraph::new(vec![message_line, buttons_line]).style(Style::default().bg(theme.base)),
        inner,
    );

    if inner.height >= 2 {
        let buttons_y = inner.y + 1;
        let yes_len = YES_LABEL.chars().count() as u16;
        let no_len = NO_LABEL.chars().count() as u16;
        let yes_rect = Rect::new(inner.x, buttons_y, yes_len, 1);
        let no_rect = Rect::new(inner.x + yes_len + BUTTON_GAP, buttons_y, no_len, 1);
        app.set_confirm_buttons(yes_rect, no_rect);
    }
}

pub fn render_toast(frame: &mut Frame, area: Rect, toast: &Toast, theme: &Theme) {
    let bg = if toast.is_error {
        theme.red
    } else {
        theme.base
    };
    let fg = if toast.is_error {
        theme.crust
    } else {
        theme.fg()
    };

    let max_width = area.width as usize;
    let message = if toast.message.chars().count() > max_width {
        toast.message.chars().take(max_width).collect()
    } else {
        toast.message.clone()
    };

    let line = Line::from(Span::styled(message, Style::default().fg(fg)));
    frame.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_overlay_editing_clears_error() {
        let mut overlay = InputOverlay::new(InputKind::NewSession, "");
        overlay.error = Some("duplicate".to_string());
        overlay.push_char('a');
        assert_eq!(overlay.text, "a");
        assert!(overlay.error.is_none());

        overlay.error = Some("duplicate".to_string());
        overlay.backspace();
        assert_eq!(overlay.text, "");
        assert!(overlay.error.is_none());
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 20, 10);
        let rect = centered_rect(50, 5, area);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 5);
    }
}
