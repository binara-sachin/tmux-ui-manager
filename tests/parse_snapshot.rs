use tmux_ui_manager::tmux::snapshot::parse_list_panes;

/// Captured via `tmux -L manager-test -f /dev/null list-panes -a -F '...'` (§11.1),
/// against a tmux version that escapes the `\x1f` template separator into the
/// literal 4-character text `\037` (tmux 3.4). Two sessions: "main" (2 windows:
/// a plain one, and one split into 2 panes with the first pane zoomed and given
/// an explicit title) and a session named with spaces and unicode. Hand-authoring
/// this file would defeat the point of the test: only real tmux output exercises
/// the actual wire format.
const FIXTURE: &str = include_str!("fixtures/multi_session_zoomed.txt");

/// Same shape as `FIXTURE`, but from a tmux version that passes the `\x1f`
/// separator through as the raw byte instead of escaping it (confirmed against
/// tmux 3.6a/3.7b) — the case that crashed the popup on first open before
/// `split_fields` learned to treat both forms as a valid delimiter.
const UNESCAPED_FIXTURE: &str = include_str!("fixtures/unescaped_separator.txt");

#[test]
fn groups_rows_into_sessions_windows_panes_preserving_tmux_order() {
    let snapshot = parse_list_panes(FIXTURE, None).expect("fixture should parse");

    assert_eq!(snapshot.totals.sessions, 2);
    assert_eq!(snapshot.totals.windows, 3);
    assert_eq!(snapshot.totals.panes, 4);
    assert_eq!(snapshot.sessions.len(), 2);

    // tmux listed the unicode session first in this capture — order must be preserved, not sorted.
    let unicode_session = &snapshot.sessions[0];
    assert_eq!(unicode_session.id.as_target(), "$1");
    assert_eq!(unicode_session.name, "d\u{f3}tf\u{ef}les \u{2615}");
    assert!(!unicode_session.attached);
    assert_eq!(unicode_session.windows.len(), 1);

    let main_session = &snapshot.sessions[1];
    assert_eq!(main_session.id.as_target(), "$0");
    assert_eq!(main_session.name, "main");
    assert_eq!(main_session.windows.len(), 2);
}

#[test]
fn window_with_space_in_name_and_two_panes_parses_correctly() {
    let snapshot = parse_list_panes(FIXTURE, None).expect("fixture should parse");
    let main_session = &snapshot.sessions[1];

    let editor = &main_session.windows[0];
    assert_eq!(editor.id.as_target(), "@0");
    assert_eq!(editor.name, "editor");
    assert_eq!(editor.panes.len(), 1);

    let logs = &main_session.windows[1];
    assert_eq!(logs.id.as_target(), "@1");
    assert_eq!(logs.name, "logs and stuff");
    assert_eq!(logs.panes.len(), 2);
}

#[test]
fn zoomed_flag_and_explicit_pane_title_are_captured() {
    let snapshot = parse_list_panes(FIXTURE, None).expect("fixture should parse");
    let logs = &snapshot.sessions[1].windows[1];

    let left = &logs.panes[0];
    assert_eq!(left.id.as_target(), "%1");
    assert!(left.active);
    assert!(left.zoomed);
    assert_eq!(left.title, "left pane title");

    let right = &logs.panes[1];
    assert_eq!(right.id.as_target(), "%2");
    assert!(!right.active);
    // window_zoomed_flag is per-window, so the sibling pane reports zoomed too (§10.6).
    assert!(right.zoomed);
}

#[test]
fn client_session_is_threaded_through_when_provided() {
    use tmux_ui_manager::tmux::ids::SessionId;

    let snapshot =
        parse_list_panes(FIXTURE, Some(SessionId::new("$0"))).expect("fixture should parse");
    assert_eq!(snapshot.client_session.unwrap().as_target(), "$0");
}

#[test]
fn rejects_a_line_with_the_wrong_field_count() {
    let err = parse_list_panes("$0\\037main\\0370", None).expect_err("should fail to parse");
    assert!(err.to_string().contains("line 1"));
}

#[test]
fn blank_trailing_lines_are_ignored() {
    let with_trailing_newline = format!("{FIXTURE}\n");
    let snapshot = parse_list_panes(&with_trailing_newline, None).expect("should still parse");
    assert_eq!(snapshot.totals.sessions, 2);
}

#[test]
fn parses_correctly_when_tmux_does_not_escape_the_separator() {
    let snapshot = parse_list_panes(UNESCAPED_FIXTURE, None).expect("fixture should parse");

    assert_eq!(snapshot.totals.sessions, 1);
    assert_eq!(snapshot.totals.windows, 1);
    assert_eq!(snapshot.totals.panes, 1);

    let session = &snapshot.sessions[0];
    assert_eq!(session.id.as_target(), "$0");
    assert_eq!(session.name, "main");

    let window = &session.windows[0];
    assert_eq!(window.id.as_target(), "@0");
    assert_eq!(window.name, "editor");

    let pane = &window.panes[0];
    assert_eq!(pane.id.as_target(), "%0");
    assert_eq!(pane.command, "bash");
    assert_eq!(pane.path, "/tmp/project");
}
