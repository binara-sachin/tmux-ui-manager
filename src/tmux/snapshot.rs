use std::fmt;
use std::process::Command;

use crate::model::{Pane, Session, Snapshot, Totals, Window};
use crate::tmux::ids::{PaneId, SessionId, WindowId};

/// ASCII Unit Separator, embedded (as a real byte) in the `-F` template we send to
/// tmux — chosen because session/window/pane names may contain any printable
/// character.
const TEMPLATE_SEP: char = '\u{1f}';

/// tmux's `-F` output always escapes non-printable bytes (confirmed empirically
/// against tmux 3.4 via a bare `std::process::Command`, no shell involved: a real
/// tab in a session name comes back as the two chars `\t`, a real backslash comes
/// back doubled as `\\`). So the `TEMPLATE_SEP` byte we send does NOT survive as a
/// byte — it comes back as this literal 4-character text instead. Splitting on
/// this text is safe for any realistic name/path/title: the only way a field
/// value could produce this exact substring is a literal backslash immediately
/// followed by the literal text `037` (e.g. a session named `foo\037bar`), which
/// doubles to `foo\\037bar` and happens to contain `\037` — accepted as a
/// vanishingly unlikely edge case for v1, not defended against.
const OUTPUT_SEP: &str = "\\037";

const FIELD_COUNT: usize = 15;

/// Builds the `-F` format string for the one-shot `list-panes -a` snapshot query (§4.2).
/// Field order must match `parse_pane_line`.
fn snapshot_format_string() -> String {
    [
        "#{session_id}",
        "#{session_name}",
        "#{session_attached}",
        "#{window_id}",
        "#{window_index}",
        "#{window_name}",
        "#{window_active}",
        "#{window_layout}",
        "#{pane_id}",
        "#{pane_index}",
        "#{pane_active}",
        "#{pane_current_command}",
        "#{pane_current_path}",
        "#{pane_title}",
        "#{window_zoomed_flag}",
    ]
    .join(&TEMPLATE_SEP.to_string())
}

#[derive(Debug)]
pub enum SnapshotError {
    Io(std::io::Error),
    TmuxFailed {
        command: &'static str,
        stderr: String,
    },
    Parse(ParseError),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Io(e) => write!(f, "failed to run tmux: {e}"),
            SnapshotError::TmuxFailed { command, stderr } => {
                write!(f, "`{command}` failed: {}", stderr.trim())
            }
            SnapshotError::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<ParseError> for SnapshotError {
    fn from(e: ParseError) -> Self {
        SnapshotError::Parse(e)
    }
}

#[derive(Debug)]
pub struct ParseError {
    pub line_number: usize,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "malformed list-panes output at line {}: {}",
            self.line_number, self.reason
        )
    }
}

impl std::error::Error for ParseError {}

/// Runs `tmux list-panes -a` and `tmux display-message -p '#{client_session}'`,
/// then assembles a full `Snapshot`. This is the only place that shells out for
/// the read side of the model.
pub fn take_snapshot() -> Result<Snapshot, SnapshotError> {
    let panes_output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", &snapshot_format_string()])
        .output()
        .map_err(SnapshotError::Io)?;

    if !panes_output.status.success() {
        return Err(SnapshotError::TmuxFailed {
            command: "tmux list-panes -a",
            stderr: String::from_utf8_lossy(&panes_output.stderr).into_owned(),
        });
    }

    let raw = String::from_utf8_lossy(&panes_output.stdout);

    let client_session = client_session_id()?;

    Ok(parse_list_panes(&raw, client_session)?)
}

/// Fetches the session id the popup's own client is attached to, if any.
/// Absent (e.g. a detached/control invocation) is not an error — `Snapshot::client_session`
/// is simply `None`.
fn client_session_id() -> Result<Option<SessionId>, SnapshotError> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{client_session}"])
        .output()
        .map_err(SnapshotError::Io)?;

    if !output.status.success() {
        return Ok(None);
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(SessionId::new(trimmed.to_string())))
    }
}

/// Parses the raw `list-panes -a` output (one pane per line, fields separated by
/// [`FIELD_SEP`]) into a full `Snapshot`, grouping rows by session then window while
/// preserving tmux's output order.
pub fn parse_list_panes(
    raw: &str,
    client_session: Option<SessionId>,
) -> Result<Snapshot, ParseError> {
    let mut sessions: Vec<Session> = Vec::new();
    let mut totals = Totals::default();

    for (i, line) in raw.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_number = i + 1;
        let row = parse_pane_line(line, line_number)?;

        let session = match sessions.iter_mut().find(|s| s.id == row.session_id) {
            Some(s) => s,
            None => {
                sessions.push(Session {
                    id: row.session_id.clone(),
                    name: row.session_name.clone(),
                    attached: row.session_attached,
                    windows: Vec::new(),
                });
                totals.sessions += 1;
                sessions.last_mut().unwrap()
            }
        };

        let window = match session.windows.iter_mut().find(|w| w.id == row.window_id) {
            Some(w) => w,
            None => {
                session.windows.push(Window {
                    id: row.window_id.clone(),
                    index: row.window_index,
                    name: row.window_name.clone(),
                    active: row.window_active,
                    layout: row.window_layout.clone(),
                    panes: Vec::new(),
                });
                totals.windows += 1;
                session.windows.last_mut().unwrap()
            }
        };

        window.panes.push(Pane {
            id: row.pane_id,
            index: row.pane_index,
            active: row.pane_active,
            command: row.pane_command,
            path: row.pane_path,
            title: row.pane_title,
            zoomed: row.window_zoomed,
        });
        totals.panes += 1;
    }

    Ok(Snapshot {
        sessions,
        client_session,
        totals,
    })
}

struct PaneRow {
    session_id: SessionId,
    session_name: String,
    session_attached: bool,
    window_id: WindowId,
    window_index: u32,
    window_name: String,
    window_active: bool,
    window_layout: String,
    pane_id: PaneId,
    pane_index: u32,
    pane_active: bool,
    pane_command: String,
    pane_path: String,
    pane_title: String,
    window_zoomed: bool,
}

fn parse_pane_line(line: &str, line_number: usize) -> Result<PaneRow, ParseError> {
    let fields: Vec<&str> = line.split(OUTPUT_SEP).collect();
    if fields.len() != FIELD_COUNT {
        return Err(ParseError {
            line_number,
            reason: format!("expected {FIELD_COUNT} fields, got {}", fields.len()),
        });
    }

    let parse_bool_flag = |s: &str, name: &str| -> Result<bool, ParseError> {
        match s {
            "0" => Ok(false),
            _ if !s.is_empty() => Ok(true),
            _ => Err(ParseError {
                line_number,
                reason: format!("expected {name} flag, got empty string"),
            }),
        }
    };

    let parse_index = |s: &str, name: &str| -> Result<u32, ParseError> {
        s.parse::<u32>().map_err(|_| ParseError {
            line_number,
            reason: format!("expected numeric {name}, got {s:?}"),
        })
    };

    Ok(PaneRow {
        session_id: SessionId::new(fields[0].to_string()),
        session_name: fields[1].to_string(),
        session_attached: parse_bool_flag(fields[2], "session_attached")?,
        window_id: WindowId::new(fields[3].to_string()),
        window_index: parse_index(fields[4], "window_index")?,
        window_name: fields[5].to_string(),
        window_active: parse_bool_flag(fields[6], "window_active")?,
        window_layout: fields[7].to_string(),
        pane_id: PaneId::new(fields[8].to_string()),
        pane_index: parse_index(fields[9], "pane_index")?,
        pane_active: parse_bool_flag(fields[10], "pane_active")?,
        pane_command: fields[11].to_string(),
        pane_path: fields[12].to_string(),
        pane_title: fields[13].to_string(),
        window_zoomed: parse_bool_flag(fields[14], "window_zoomed_flag")?,
    })
}
