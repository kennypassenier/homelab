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
