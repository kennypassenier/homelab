//! How the command line prints what the host answers.

use std::sync::atomic::{AtomicBool, Ordering};

/// fix-109 (colour-codes-when-piped, 2026-09-27): colour only when standard
/// output is a terminal and `NO_COLOR` is unset or empty (no-color.org).
/// The codes were hard-coded, so every pipe, log and saved check carried
/// `\x1b[31m` and a grep over them broke.
pub fn should_colour(stdout_is_terminal: bool, no_color: Option<&str>) -> bool {
    stdout_is_terminal && no_color.is_none_or(str::is_empty)
}

static COLOUR: AtomicBool = AtomicBool::new(false);

/// Decided once, in `main`, before anything is printed.
pub fn set_colour(on: bool) {
    COLOUR.store(on, Ordering::Relaxed);
}

/// An escape code that prints itself only when colour is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint(pub &'static str);

impl std::fmt::Display for Paint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if COLOUR.load(Ordering::Relaxed) {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

/// How a line of a fleet check is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Broken,
    Drift,
    Noted,
    Plain,
}

/// fix-103 (see REGISTER.md): the tone of every line of a fleet check reply.
/// Each group keeps its own tone, the lines under a group heading take the
/// heading's, and the summary line takes the tone of the worst group it
/// counts.
pub fn check_tones(msg: &str) -> Vec<(Tone, &str)> {
    let mut tone = Tone::Plain;
    msg.lines()
        .map(|line| {
            if let Some(summary) = line.strip_prefix("fleet check:") {
                tone = if summary.contains(" broken") {
                    Tone::Broken
                } else if summary.contains(" drift") {
                    Tone::Drift
                } else if summary.contains(" noted") {
                    Tone::Noted
                } else {
                    Tone::Plain
                };
            } else if line.starts_with("broken — ") {
                tone = Tone::Broken;
            } else if line.starts_with("drift — ") {
                tone = Tone::Drift;
            } else if line.starts_with("noted — ") {
                tone = Tone::Noted;
            }
            (tone, line)
        })
        .collect()
}

/// The host's reply to `command` as a person reads it: the checks listing
/// without the blank line that left "✓ " alone on a row, and the tool status
/// as a table instead of its JSON. Anything else, and a reply that does not
/// parse, comes back as it was.
pub fn readable_reply(command: &homelab_proto::Command, message: &str) -> String {
    use homelab_proto::Command;
    match command {
        Command::ListManualChecks { json: false } => message.trim_start_matches('\n').to_string(),
        Command::ToolStatus => tool_table(message).unwrap_or_else(|| message.to_string()),
        _ => message.to_string(),
    }
}

/// What the follow rule says about one tool, in words.
fn tool_verdict(v: &serde_json::Value) -> String {
    let version = v["version"].as_str().unwrap_or("?");
    match v["state"].as_str() {
        Some("current") => "up to date".into(),
        Some("install") => format!("update to {version}"),
        Some("hold") if v["major"].as_bool() == Some(true) => {
            format!("{version} held back (new major)")
        }
        Some("hold") => format!("{version} held back"),
        _ => "unknown".into(),
    }
}

/// `{"tools":[...]}` as aligned rows; None when it is not that shape.
fn tool_table(message: &str) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_str(message).ok()?;
    let tools = doc["tools"].as_array()?;
    let mut rows = vec![["TOOL", "INSTALLED", "NEWEST", "FOLLOW", "STATE"].map(String::from)];
    for t in tools {
        let text = |k: &str| t[k].as_str().unwrap_or("-").to_string();
        rows.push(match t["error"].as_str() {
            Some(e) => [
                text("name"),
                "-".into(),
                "-".into(),
                "-".into(),
                format!("error: {e}"),
            ],
            None => [
                text("name"),
                text("installed"),
                text("newest"),
                text("follow"),
                tool_verdict(&t["verdict"]),
            ],
        });
    }
    let width = |c: usize| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..4).map(width).collect();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let cells: Vec<String> = (0..4)
                .map(|c| format!("{:<w$}", r[c], w = widths[c]))
                .collect();
            format!("{}  {}", cells.join("  "), r[4])
        })
        .collect();
    Some(lines.join("\n  "))
}
