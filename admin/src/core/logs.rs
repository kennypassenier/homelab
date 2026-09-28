//! feat-ops-4: container logs from Loki, the pure half. The query the
//! dashboard's server sends, and Loki's answer as flat lines, newest first.
//!
//! Labels as Alloy ships them (read from Loki 2026-09-28): `stack` (the
//! stack's name), `container_name` for docker lines, `unit` for journal
//! lines, `job` (`docker`, `systemd-journal`), `stream`, `host`; Loki adds
//! `detected_level`.

use serde::{Deserialize, Serialize};

/// What the page asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LogQuery {
    pub stack: String,
    /// One container; `journal` means the stack's systemd journal; None
    /// means everything the stack ships.
    #[serde(default)]
    pub app: Option<String>,
    /// Lines that contain this text (case-sensitive, as Loki's `|=`).
    #[serde(default)]
    pub q: Option<String>,
    /// How far back, seconds. Default one hour, at most a week.
    #[serde(default = "d_since")]
    pub since: u64,
    /// At most this many lines. Default 500, at most 5000.
    #[serde(default = "d_limit")]
    pub limit: usize,
}

fn d_since() -> u64 {
    3600
}
fn d_limit() -> usize {
    500
}

/// The longest window and the most lines one request may ask for.
pub const MAX_SINCE_S: u64 = 7 * 86400;
pub const MAX_LIMIT: usize = 5000;

/// A label value the dashboard puts in a selector: a stack or container
/// name. Anything else is refused rather than escaped.
fn plain(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// A LogQL string literal.
fn quoted(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The LogQL for `q`, or why it cannot be asked.
pub fn logql(q: &LogQuery) -> Result<String, String> {
    if !plain(&q.stack) {
        return Err(format!("{:?} is not a stack name", q.stack));
    }
    let mut selector = format!("stack={}", quoted(&q.stack));
    match q.app.as_deref().filter(|a| !a.is_empty()) {
        None => {}
        Some("journal") => selector.push_str(", job=\"systemd-journal\""),
        Some(app) if plain(app) => {
            selector.push_str(&format!(", container_name={}", quoted(app)));
        }
        Some(app) => return Err(format!("{:?} is not an app name", app)),
    }
    let mut out = format!("{{{}}}", selector);
    if let Some(text) = q.q.as_deref().filter(|t| !t.is_empty()) {
        if text.len() > 500 {
            return Err("the search text is longer than 500 characters".into());
        }
        out.push_str(&format!(" |= {}", quoted(text)));
    }
    Ok(out)
}

/// The window and line count actually asked, clamped.
pub fn window(q: &LogQuery, now: u64) -> (u64, u64, usize) {
    let since = q.since.clamp(60, MAX_SINCE_S);
    let limit = q.limit.clamp(1, MAX_LIMIT);
    (now.saturating_sub(since), now, limit)
}

/// One line as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogLine {
    /// Unix milliseconds.
    pub ts_ms: u64,
    /// The container, the journal unit, or the job.
    pub source: String,
    /// `stdout`, `stderr`, or empty.
    pub stream: String,
    /// Loki's detected level (`info`, `error`, …) or empty.
    pub level: String,
    pub line: String,
}

#[derive(Deserialize)]
struct Answer {
    status: String,
    data: Data,
}
#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    result: Vec<Stream>,
}
#[derive(Deserialize)]
struct Stream {
    #[serde(default)]
    stream: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    values: Vec<(String, String)>,
}

/// Terminal colour codes a container printed; they mean nothing on a page.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Loki's `query_range` answer as lines, newest first, at most `limit`.
pub fn parse_answer(body: &str, limit: usize) -> Result<Vec<LogLine>, String> {
    let a: Answer =
        serde_json::from_str(body).map_err(|e| format!("Loki's answer does not read: {e}"))?;
    if a.status != "success" {
        return Err(format!("Loki answered status {:?}", a.status));
    }
    let mut out = Vec::new();
    for s in a.data.result {
        let label = |k: &str| s.stream.get(k).cloned().unwrap_or_default();
        let source = [label("container_name"), label("unit"), label("job")]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        let level = label("detected_level");
        let level = if level == "unknown" {
            String::new()
        } else {
            level
        };
        for (ts, line) in &s.values {
            let ns: u128 = ts.parse().unwrap_or(0);
            out.push(LogLine {
                ts_ms: (ns / 1_000_000) as u64,
                source: source.clone(),
                stream: label("stream"),
                level: level.clone(),
                line: strip_ansi(line.trim_end()),
            });
        }
    }
    out.sort_by_key(|l| std::cmp::Reverse(l.ts_ms));
    out.truncate(limit);
    Ok(out)
}
