use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::model::{Pane, Session, Window};
use crate::ui::app::{App, Column, Mode};
use crate::ui::drag::{DragItem, DropTarget, PlannedAction};
use crate::ui::theme::Theme;

/// Renders the three linked Miller columns (§6.1): SESSIONS -> WINDOWS -> PANES.
pub fn render_columns(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(40),
            Constraint::Percentage(32),
        ])
        .split(area);

    render_sessions(frame, chunks[0], app, theme);
    render_windows(frame, chunks[1], app, theme);
    render_panes(frame, chunks[2], app, theme);
}

fn block(title: String, theme: &Theme) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.panel_title())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()))
}

/// Whether the currently-focused column's cursor is resting on a *committable*
/// (non-`NoOp`) target — used to decide between the normal selection highlight
/// and the drag accent highlight (§6.5: valid targets get `bg surface1 + fg
/// blue`; invalid ones are "not highlighted", i.e. rendered like any other row).
fn cursor_is_valid_drop(app: &App) -> bool {
    matches!(app.mode, Mode::Dragging(_)) && !matches!(app.plan_current_drop(), PlannedAction::NoOp)
}

fn render_sessions(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Sessions;
    let dragging = matches!(app.mode, Mode::Dragging(_));
    let b = block("SESSIONS".to_string(), theme);
    let inner_width = b.inner(area).width;

    let mut items: Vec<ListItem> = app
        .snapshot
        .sessions
        .iter()
        .map(|s| session_row(s, app, focused, inner_width, theme))
        .collect();

    let on_pseudo =
        focused && dragging && matches!(app.resolve_drop_target(), Some(DropTarget::NewSessionRow));
    let pseudo_valid = on_pseudo && cursor_is_valid_drop(app);
    items.push(pseudo_row(
        "+ new session",
        inner_width,
        theme,
        pseudo_valid,
    ));

    frame.render_widget(List::new(items).block(b), area);
}

fn render_windows(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Windows;
    let title = match app.current_session() {
        Some(s) => format!("WINDOWS \u{b7} {}", s.name),
        None => "WINDOWS".to_string(),
    };
    let b = block(title, theme);
    let inner_width = b.inner(area).width;

    let dragged_window = match &app.mode {
        Mode::Dragging(drag) => match &drag.item {
            DragItem::Window(id) => Some(id.clone()),
            DragItem::Pane(_) => None,
        },
        _ => None,
    };
    let gap_target = if focused {
        match app.resolve_drop_target() {
            Some(DropTarget::WindowGap { anchor, after }) => Some((anchor, after)),
            _ => None,
        }
    } else {
        None
    };
    let gap_valid = gap_target.is_some() && cursor_is_valid_drop(app);

    let windows = app.windows();
    let mut items: Vec<ListItem> = Vec::with_capacity(windows.len() + 2);
    for w in windows {
        if let Some((anchor, after)) = &gap_target
            && anchor == &w.id
            && !after
        {
            items.push(insertion_line(inner_width, theme, gap_valid));
        }
        let picked_up = dragged_window.as_ref() == Some(&w.id);
        items.push(window_row(w, app, focused, inner_width, theme, picked_up));
        if let Some((anchor, after)) = &gap_target
            && anchor == &w.id
            && *after
        {
            items.push(insertion_line(inner_width, theme, gap_valid));
        }
    }

    let on_pseudo = focused && matches!(app.resolve_drop_target(), Some(DropTarget::NewWindowRow));
    let pseudo_valid = on_pseudo && cursor_is_valid_drop(app);
    items.push(pseudo_row("+ new window", inner_width, theme, pseudo_valid));

    frame.render_widget(List::new(items).block(b), area);
}

fn render_panes(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Panes;
    let title = match app.current_window() {
        Some(w) => format!("PANES \u{b7} {}:{}", w.index, w.name),
        None => "PANES".to_string(),
    };
    let b = block(title, theme);
    let inner_width = b.inner(area).width;

    let dragged_pane = match &app.mode {
        Mode::Dragging(drag) => match &drag.item {
            DragItem::Pane(id) => Some(id.clone()),
            DragItem::Window(_) => None,
        },
        _ => None,
    };

    let home = std::env::var("HOME").ok();
    let mut items: Vec<ListItem> = app
        .panes()
        .iter()
        .map(|p| {
            let picked_up = dragged_pane.as_ref() == Some(&p.id);
            pane_row(
                p,
                app,
                focused,
                inner_width,
                theme,
                home.as_deref(),
                picked_up,
            )
        })
        .collect();
    items.push(pseudo_row("+ split pane", inner_width, theme, false));

    frame.render_widget(List::new(items).block(b), area);
}

/// The picked-up row's styling while dragging (§6.5): fg blue, italic, meta
/// replaced with a "→ drop on target" hint.
fn picked_up_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.accent())
        .add_modifier(Modifier::ITALIC)
}

fn session_row(
    s: &Session,
    app: &App,
    column_focused: bool,
    width: u16,
    theme: &Theme,
) -> ListItem<'static> {
    let selected = app.selected_session.as_ref() == Some(&s.id);
    let dot = if s.attached { "\u{25cf}" } else { "\u{25cb}" };
    let dot_color = if s.attached {
        theme.active()
    } else {
        theme.meta()
    };
    let name_style = if selected && column_focused {
        Style::default().fg(theme.fg()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg())
    };
    let left = vec![
        Span::styled(format!("{dot} "), Style::default().fg(dot_color)),
        Span::styled(s.name.clone(), name_style),
    ];
    let right = format!("{} win", s.windows.len());
    let line = two_part_line(left, right, Style::default().fg(theme.meta()), width);

    let drop_valid = selected
        && column_focused
        && cursor_is_valid_drop(app)
        && matches!(app.resolve_drop_target(), Some(DropTarget::SessionRow(id)) if id == s.id);
    styled_row(vec![line], selected, column_focused, drop_valid, theme)
}

#[allow(clippy::too_many_arguments)]
fn window_row(
    w: &Window,
    app: &App,
    column_focused: bool,
    width: u16,
    theme: &Theme,
    picked_up: bool,
) -> ListItem<'static> {
    let selected = app.selected_window.as_ref() == Some(&w.id);
    let name_style = if picked_up {
        picked_up_style(theme)
    } else if selected && column_focused {
        Style::default().fg(theme.fg()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg())
    };
    let zoomed = w.panes.iter().any(|p| p.zoomed);
    let left = vec![Span::styled(format!("{}: {}", w.index, w.name), name_style)];

    let right = if picked_up {
        "\u{2192} drop on target".to_string()
    } else {
        let active_command = w
            .panes
            .iter()
            .find(|p| p.active)
            .map(|p| p.command.as_str())
            .unwrap_or("");
        if zoomed {
            format!("{active_command} Z")
        } else {
            active_command.to_string()
        }
    };
    let right_style = if picked_up {
        Style::default().fg(theme.accent())
    } else {
        Style::default().fg(theme.meta())
    };
    let line = two_part_line(left, right, right_style, width);

    let drop_valid = !picked_up
        && selected
        && column_focused
        && cursor_is_valid_drop(app)
        && matches!(app.resolve_drop_target(), Some(DropTarget::WindowRow(id)) if id == w.id);
    styled_row(
        vec![line],
        selected && !picked_up,
        column_focused,
        drop_valid,
        theme,
    )
}

#[allow(clippy::too_many_arguments)]
fn pane_row(
    p: &Pane,
    app: &App,
    column_focused: bool,
    width: u16,
    theme: &Theme,
    home: Option<&str>,
    picked_up: bool,
) -> ListItem<'static> {
    let selected = app.selected_pane.as_ref() == Some(&p.id);
    let dot = if p.active { "\u{25cf}" } else { "\u{25cb}" };
    let dot_color = if p.active {
        theme.active()
    } else {
        theme.meta()
    };
    let command_style = if picked_up {
        picked_up_style(theme)
    } else {
        Style::default().fg(theme.fg())
    };
    let line1 = if picked_up {
        Line::from(vec![
            Span::styled(format!("{dot} "), Style::default().fg(dot_color)),
            Span::styled(p.command.clone(), command_style),
            Span::raw("  "),
            Span::styled(
                "\u{2192} drop on target",
                Style::default().fg(theme.accent()),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(format!("{dot} "), Style::default().fg(dot_color)),
            Span::styled(p.command.clone(), command_style),
            Span::raw("  "),
            Span::styled(p.id.to_string(), Style::default().fg(theme.meta())),
        ])
    };
    let line2 = Line::from(Span::styled(
        abbreviate_home(&p.path, home),
        Style::default().fg(theme.meta()),
    ));
    let _ = width;

    let drop_valid = !picked_up
        && selected
        && column_focused
        && cursor_is_valid_drop(app)
        && matches!(app.resolve_drop_target(), Some(DropTarget::PaneRow(id)) if id == p.id);
    styled_row(
        vec![line1, line2],
        selected && !picked_up,
        column_focused,
        drop_valid,
        theme,
    )
}

fn pseudo_row(label: &str, width: u16, theme: &Theme, drop_valid: bool) -> ListItem<'static> {
    let style = if drop_valid {
        Style::default().fg(theme.accent())
    } else {
        Style::default().fg(theme.overlay0)
    };
    let line = Line::from(Span::styled(label.to_string(), style));
    let _ = width;
    let bg = if drop_valid {
        theme.surface1
    } else {
        theme.bg()
    };
    ListItem::new(vec![line]).style(Style::default().bg(bg))
}

/// The blue insertion line drawn between two rows for a window-reorder gap
/// target (§6.5).
fn insertion_line(width: u16, theme: &Theme, valid: bool) -> ListItem<'static> {
    let color = if valid {
        theme.accent()
    } else {
        theme.overlay0
    };
    let fill = "\u{2500}".repeat((width as usize).saturating_sub(2).max(1));
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled(fill, Style::default().fg(color)),
    ]);
    ListItem::new(vec![line]).style(Style::default().bg(theme.bg()))
}

/// Prefixes every row with a 2-column gutter: a `│` accent bar (green) on the
/// focused column's selected row, two spaces otherwise — keeps multi-line rows
/// (panes) aligned since every line gets the same prefix width. `drop_valid`
/// overrides the background/gutter color to the drag accent (blue) instead of
/// the normal selection color.
fn styled_row(
    mut lines: Vec<Line<'static>>,
    selected: bool,
    column_focused: bool,
    drop_valid: bool,
    theme: &Theme,
) -> ListItem<'static> {
    let bg = if drop_valid {
        theme.surface1
    } else if selected && column_focused {
        theme.selection_bg()
    } else if selected {
        theme.mantle
    } else {
        theme.bg()
    };
    let gutter_color = if drop_valid {
        theme.accent()
    } else {
        theme.active()
    };

    for (i, line) in lines.iter_mut().enumerate() {
        let gutter = if i == 0 && (selected && column_focused || drop_valid) {
            Span::styled("\u{2502} ", Style::default().fg(gutter_color))
        } else {
            Span::raw("  ")
        };
        let mut spans = vec![gutter];
        spans.extend(line.spans.clone());
        *line = Line::from(spans);
    }

    ListItem::new(lines).style(Style::default().bg(bg).fg(theme.fg()))
}

fn two_part_line(
    left: Vec<Span<'static>>,
    right: String,
    right_style: Style,
    width: u16,
) -> Line<'static> {
    let left_len: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let right_len = right.chars().count();
    // Reserve 2 columns for the gutter prefix added by `styled_row`.
    let available = (width as usize).saturating_sub(2);
    let pad = available
        .saturating_sub(left_len + right_len)
        .max(if right_len > 0 { 1 } else { 0 });

    let mut spans = left;
    if right_len > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(right, right_style));
    }
    Line::from(spans)
}

fn abbreviate_home(path: &str, home: Option<&str>) -> String {
    match home {
        Some(h) if !h.is_empty() && path.starts_with(h) => {
            format!("~{}", &path[h.len()..])
        }
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviate_home_replaces_prefix() {
        assert_eq!(
            abbreviate_home("/Users/binara/dev/api-server", Some("/Users/binara")),
            "~/dev/api-server"
        );
    }

    #[test]
    fn abbreviate_home_leaves_unrelated_paths_untouched() {
        assert_eq!(
            abbreviate_home("/opt/data", Some("/Users/binara")),
            "/opt/data"
        );
    }

    #[test]
    fn two_part_line_pads_between_left_and_right() {
        let line = two_part_line(
            vec![Span::raw("name")],
            "meta".to_string(),
            Style::default(),
            20,
        );
        let total: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(total, 18); // width(20) - 2-col gutter reserved by styled_row
    }
}
