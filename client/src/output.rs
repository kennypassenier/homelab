//! How the command line prints what the host answers.

/// How a line of a fleet check is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Broken,
    Drift,
    Noted,
    Plain,
}

/// fix-103 (check-output-buries-problem, 2026-09-27): the tone of every line
/// of a fleet check reply. The whole check used to be printed in red, the
/// `noted` items ("nothing to do") included, so nine red lines hid the one
/// that needed action. Each group now keeps its own tone, the lines under a
/// group heading take the heading's, and the summary line takes the tone of
/// the worst group it counts.
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
