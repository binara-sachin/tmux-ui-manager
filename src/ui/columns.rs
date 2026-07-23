use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::model::{Pane, Session, Window};
use crate::ui::app::{App, Column, Mode};
use crate::ui::drag::{DragItem, DropTarget, PlannedAction};
use crate::ui::hitmap::ClickTarget;
use crate::ui::theme::Theme;

/// One rendered row's clickable height (1, or 2 for the two-line pane rows)
/// and the hit-map entry it registers — built in lockstep with each column's
/// `Vec<ListItem>` so the two never drift apart.
struct RowSpec {
    height: u16,
    target: ClickTarget,
}

/// Renders the three linked Miller columns (§6.1): SESSIONS -> WINDOWS -> PANES.
pub fn render_columns(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    app.begin_frame_hit_map();

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

/// Registers this frame's column-area (for scroll/hover hit-testing) and
/// row hit-map (for click/drop resolution) in one place, so every column
/// follows the exact same offset math the visible rows were built from.
fn layout_column(
    app: &App,
    column: Column,
    area: Rect,
    inner: Rect,
    specs: &[RowSpec],
    selected_row: Option<usize>,
) -> usize {
    app.set_column_area(column, area);
    let heights: Vec<u16> = specs.iter().map(|s| s.height).collect();
    let offset = clamp_scroll(
        app.scroll_offset(column),
        selected_row,
        &heights,
        inner.height,
    );
    app.set_scroll_offset(column, offset);

    let mut y = inner.y;
    for spec in specs.iter().skip(offset) {
        if y >= inner.y + inner.height {
            break;
        }
        let visible_height = spec.height.min((inner.y + inner.height).saturating_sub(y));
        if visible_height == 0 {
            break;
        }
        app.register_hit(
            Rect::new(inner.x, y, inner.width, visible_height),
            spec.target.clone(),
        );
        y += spec.height;
    }
    offset
}

/// Picks the smallest `offset` such that `selected` (if any) is fully within
/// the visible window, accounting for each row's height (panes are 2 lines).
/// Snaps down instantly if selection moved above the current offset (e.g. a
/// keyboard jump-to-top), then walks forward one row at a time if it's below
/// the visible window — the list is tiny, so this never loops meaningfully.
fn clamp_scroll(
    offset: usize,
    selected: Option<usize>,
    heights: &[u16],
    visible_height: u16,
) -> usize {
    let Some(selected) = selected else {
        return offset.min(heights.len());
    };
    let mut offset = offset.min(selected);
    loop {
        let mut used = 0u16;
        let mut last_visible = offset;
        for (i, h) in heights.iter().enumerate().skip(offset) {
            if used + h > visible_height {
                break;
            }
            used += h;
            last_visible = i;
        }
        if selected <= last_visible {
            break;
        }
        offset += 1;
    }
    offset
}

fn render_sessions(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Sessions;
    let dragging = matches!(app.mode, Mode::Dragging(_));
    let hovered = app.hover();
    let b = block("SESSIONS".to_string(), theme);
    let inner = b.inner(area);

    let mut items: Vec<ListItem> = Vec::with_capacity(app.snapshot.sessions.len() + 1);
    let mut specs: Vec<RowSpec> = Vec::with_capacity(app.snapshot.sessions.len() + 1);
    let mut selected_row = None;

    for s in &app.snapshot.sessions {
        let is_hovered = !dragging && hovered.as_ref() == Some(&ClickTarget::Session(s.id.clone()));
        if app.selected_session.as_ref() == Some(&s.id) {
            selected_row = Some(specs.len());
        }
        items.push(session_row(s, app, focused, inner.width, theme, is_hovered));
        specs.push(RowSpec {
            height: 1,
            target: ClickTarget::Session(s.id.clone()),
        });
    }

    let on_pseudo =
        focused && dragging && matches!(app.resolve_drop_target(), Some(DropTarget::NewSessionRow));
    let pseudo_valid = on_pseudo && cursor_is_valid_drop(app);
    let pseudo_hovered = !dragging && hovered == Some(ClickTarget::NewSessionRow);
    items.push(pseudo_row(
        "+ new session",
        theme,
        pseudo_valid,
        pseudo_hovered,
    ));
    specs.push(RowSpec {
        height: 1,
        target: ClickTarget::NewSessionRow,
    });

    let offset = layout_column(app, Column::Sessions, area, inner, &specs, selected_row);
    let mut state = ListState::default()
        .with_offset(offset)
        .with_selected(selected_row);
    frame.render_stateful_widget(List::new(items).block(b), area, &mut state);
}

fn render_windows(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Windows;
    let dragging = matches!(app.mode, Mode::Dragging(_));
    let hovered = app.hover();
    let title = match app.current_session() {
        Some(s) => format!("WINDOWS \u{b7} {}", s.name),
        None => "WINDOWS".to_string(),
    };
    let b = block(title, theme);
    let inner = b.inner(area);

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
    let mut items: Vec<ListItem> = Vec::with_capacity(windows.len() * 2 + 2);
    let mut specs: Vec<RowSpec> = Vec::with_capacity(windows.len() * 2 + 2);
    let mut selected_row = None;

    for w in windows {
        if let Some((anchor, after)) = &gap_target
            && anchor == &w.id
            && !after
        {
            items.push(insertion_line(inner.width, theme, gap_valid));
            specs.push(RowSpec {
                height: 1,
                target: ClickTarget::WindowGap {
                    anchor: anchor.clone(),
                    after: false,
                },
            });
        }
        let picked_up = dragged_window.as_ref() == Some(&w.id);
        let is_hovered = !dragging && hovered.as_ref() == Some(&ClickTarget::Window(w.id.clone()));
        if app.selected_window.as_ref() == Some(&w.id) {
            selected_row = Some(specs.len());
        }
        items.push(window_row(
            w,
            app,
            focused,
            inner.width,
            theme,
            picked_up,
            is_hovered,
        ));
        specs.push(RowSpec {
            height: 1,
            target: ClickTarget::Window(w.id.clone()),
        });
        if let Some((anchor, after)) = &gap_target
            && anchor == &w.id
            && *after
        {
            items.push(insertion_line(inner.width, theme, gap_valid));
            specs.push(RowSpec {
                height: 1,
                target: ClickTarget::WindowGap {
                    anchor: anchor.clone(),
                    after: true,
                },
            });
        }
    }

    let on_pseudo = focused && matches!(app.resolve_drop_target(), Some(DropTarget::NewWindowRow));
    let pseudo_valid = on_pseudo && cursor_is_valid_drop(app);
    let pseudo_hovered = !dragging && hovered == Some(ClickTarget::NewWindowRow);
    items.push(pseudo_row(
        "+ new window",
        theme,
        pseudo_valid,
        pseudo_hovered,
    ));
    specs.push(RowSpec {
        height: 1,
        target: ClickTarget::NewWindowRow,
    });

    let offset = layout_column(app, Column::Windows, area, inner, &specs, selected_row);
    let mut state = ListState::default()
        .with_offset(offset)
        .with_selected(selected_row);
    frame.render_stateful_widget(List::new(items).block(b), area, &mut state);
}

fn render_panes(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Column::Panes;
    let dragging = matches!(app.mode, Mode::Dragging(_));
    let hovered = app.hover();
    let title = match app.current_window() {
        Some(w) => format!("PANES \u{b7} {}:{}", w.index, w.name),
        None => "PANES".to_string(),
    };
    let b = block(title, theme);
    let inner = b.inner(area);

    let dragged_pane = match &app.mode {
        Mode::Dragging(drag) => match &drag.item {
            DragItem::Pane(id) => Some(id.clone()),
            DragItem::Window(_) => None,
        },
        _ => None,
    };

    let home = std::env::var("HOME").ok();
    let mut items: Vec<ListItem> = Vec::with_capacity(app.panes().len() + 1);
    let mut specs: Vec<RowSpec> = Vec::with_capacity(app.panes().len() + 1);
    let mut selected_row = None;
    for p in app.panes() {
        let picked_up = dragged_pane.as_ref() == Some(&p.id);
        let is_hovered = !dragging && hovered.as_ref() == Some(&ClickTarget::Pane(p.id.clone()));
        if app.selected_pane.as_ref() == Some(&p.id) {
            selected_row = Some(specs.len());
        }
        items.push(pane_row(
            p,
            app,
            focused,
            inner.width,
            theme,
            home.as_deref(),
            picked_up,
            is_hovered,
        ));
        specs.push(RowSpec {
            height: 2,
            target: ClickTarget::Pane(p.id.clone()),
        });
    }
    let pseudo_hovered = !dragging && hovered == Some(ClickTarget::NewSplitRow);
    items.push(pseudo_row("+ split pane", theme, false, pseudo_hovered));
    specs.push(RowSpec {
        height: 1,
        target: ClickTarget::NewSplitRow,
    });

    let offset = layout_column(app, Column::Panes, area, inner, &specs, selected_row);
    let mut state = ListState::default()
        .with_offset(offset)
        .with_selected(selected_row);
    frame.render_stateful_widget(List::new(items).block(b), area, &mut state);
}

/// The picked-up row's styling while dragging (§6.5): fg blue, italic, meta
/// replaced with a "→ drop on target" hint.
fn picked_up_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.accent())
        .add_modifier(Modifier::ITALIC)
}

#[allow(clippy::too_many_arguments)]
fn session_row(
    s: &Session,
    app: &App,
    column_focused: bool,
    width: u16,
    theme: &Theme,
    hovered: bool,
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
    styled_row(
        vec![line],
        selected,
        column_focused,
        drop_valid,
        hovered,
        theme,
    )
}

#[allow(clippy::too_many_arguments)]
fn window_row(
    w: &Window,
    app: &App,
    column_focused: bool,
    width: u16,
    theme: &Theme,
    picked_up: bool,
    hovered: bool,
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
        hovered,
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
    hovered: bool,
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
        hovered,
        theme,
    )
}

fn pseudo_row(label: &str, theme: &Theme, drop_valid: bool, hovered: bool) -> ListItem<'static> {
    let style = if drop_valid {
        Style::default().fg(theme.accent())
    } else {
        Style::default().fg(theme.overlay0)
    };
    let line = Line::from(Span::styled(label.to_string(), style));
    let bg = if drop_valid {
        theme.surface1
    } else if hovered {
        theme.mantle
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
/// the normal selection color; `hovered` (idle mouse hover only, never set
/// while dragging) gets a quiet `mantle` bg one notch above the plain bg,
/// reusing the same dim color already used for unfocused-column selection
/// rather than introducing a new one (§8.3: exact palette only).
#[allow(clippy::too_many_arguments)]
fn styled_row(
    mut lines: Vec<Line<'static>>,
    selected: bool,
    column_focused: bool,
    drop_valid: bool,
    hovered: bool,
    theme: &Theme,
) -> ListItem<'static> {
    let bg = if drop_valid {
        theme.surface1
    } else if selected && column_focused {
        theme.selection_bg()
    } else if selected || hovered {
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

    #[test]
    fn clamp_scroll_snaps_down_when_selection_is_above_the_offset() {
        let heights = vec![1u16; 10];
        assert_eq!(clamp_scroll(5, Some(2), &heights, 4), 2);
    }

    #[test]
    fn clamp_scroll_advances_when_selection_is_below_the_visible_window() {
        let heights = vec![1u16; 10];
        // offset 0, height 4 -> rows 0..4 visible; selecting row 7 must scroll.
        assert_eq!(clamp_scroll(0, Some(7), &heights, 4), 4);
    }

    #[test]
    fn clamp_scroll_accounts_for_two_row_pane_heights() {
        // Three 2-row panes + a 1-row pseudo row; visible_height 5 fits rows
        // 0 and 1 (4 rows) plus one more row of row 2 — not enough for row 2
        // to be *fully* visible, so selecting it should push offset forward.
        let heights = vec![2, 2, 2, 1];
        assert_eq!(clamp_scroll(0, Some(2), &heights, 5), 1);
    }

    #[test]
    fn clamp_scroll_stays_put_when_nothing_selected() {
        let heights = vec![1u16; 3];
        assert_eq!(clamp_scroll(1, None, &heights, 2), 1);
    }
}
