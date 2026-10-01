//! Reading `docker compose ps --format json` instead of its plain-text
//! table (fix-132, serde-parsing-of-compose-ps): the text form changes
//! column widths across compose versions and was matched with
//! `--services`/`--status running`, which answers only "is anything
//! running", not which service or what state it is in.
//!
//! `docker compose ps --format json` prints one JSON object per line (JSON
//! Lines), not a JSON array, the same across every compose version in the
//! fleet (measured 2026-09-30 on the CT 996 golden template, compose
//! v2.29).

use serde::Deserialize;

/// One service's row from `docker compose ps --format json`. Only the
/// fields the orchestrator reads; compose's own JSON carries more.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComposePsEntry {
    #[serde(rename = "Service")]
    pub service: String,
    #[serde(rename = "State")]
    pub state: String,
    /// Empty when the service declares no healthcheck.
    #[serde(rename = "Health", default)]
    pub health: String,
}

impl ComposePsEntry {
    pub fn running(&self) -> bool {
        self.state == "running"
    }
}

/// Whether a probe could read compose's own answer at all.
///
/// shell-strings-quoting / mock-executor-weak-assertions (expert panel,
/// 2026-09-27): a probe that cannot run, or whose output this parser cannot
/// read, must never be folded into "no services are running" — that reads
/// as a dead stack the moment the probe itself is what broke. Keeping it as
/// its own state is the fix-132 "Unknown" shape the staleness probe already
/// uses (`deploy.rs`'s `older_than_files`), applied here too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposePs {
    /// Parsed entries, possibly empty (compose answered: nothing is up).
    Answered(Vec<ComposePsEntry>),
    /// The command failed, or its output held no line this parser could
    /// read as a compose-ps entry. Carries why, for the caller's own error.
    Unknown(String),
}

impl ComposePs {
    /// Entries, or an empty list for [`ComposePs::Unknown`] — for callers
    /// that already gate on `out.success()` themselves and only want the
    /// best-effort list (an orphan/diagnostic read, never a pass/fail gate).
    pub fn entries_or_empty(&self) -> &[ComposePsEntry] {
        match self {
            ComposePs::Answered(v) => v,
            ComposePs::Unknown(_) => &[],
        }
    }
}

/// Parse a successful `docker compose ps --format json` run's stdout.
///
/// A line that is not valid JSON, or not an object with the fields above, is
/// skipped rather than failing the whole read — one unparseable row (a
/// compose warning sharing stdout) must not hide every other service's real
/// state. Only when NOT ONE line parses is the result `Unknown`: that shape
/// means either compose printed nothing, or printed something this parser
/// does not recognise at all, and either way the caller must not read it as
/// "no services".
pub fn parse_compose_ps(out: &str) -> ComposePs {
    let lines: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return ComposePs::Answered(Vec::new());
    }
    let entries: Vec<ComposePsEntry> = lines
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if entries.is_empty() {
        return ComposePs::Unknown(format!(
            "no line of `docker compose ps --format json` output parsed as a service: {}",
            out.chars().take(200).collect::<String>()
        ));
    }
    ComposePs::Answered(entries)
}

/// Service names reported as `running`, or `Unknown`'s message on the left.
pub fn running_services(out: &str) -> Result<Vec<String>, String> {
    match parse_compose_ps(out) {
        ComposePs::Answered(entries) => Ok(entries
            .into_iter()
            .filter(|e| e.running())
            .map(|e| e.service)
            .collect()),
        ComposePs::Unknown(why) => Err(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_json_object_per_line_parses_into_entries() {
        let out = "{\"Service\":\"app\",\"State\":\"running\",\"Health\":\"healthy\"}\n\
                    {\"Service\":\"db\",\"State\":\"exited\",\"Health\":\"\"}\n";
        let ComposePs::Answered(entries) = parse_compose_ps(out) else {
            panic!("expected Answered");
        };
        assert_eq!(entries.len(), 2);
        assert!(entries[0].running(), "{:?}", entries[0]);
        // The negative twin: the exited service is not reported running.
        assert!(!entries[1].running(), "{:?}", entries[1]);
    }

    #[test]
    fn empty_output_is_answered_with_no_entries() {
        // compose prints nothing when nothing exists yet — a real answer,
        // not a probe failure.
        assert_eq!(parse_compose_ps(""), ComposePs::Answered(Vec::new()));
    }

    #[test]
    fn unparseable_output_is_unknown_not_empty() {
        let got = parse_compose_ps("docker: permission denied\n");
        match got {
            ComposePs::Unknown(why) => assert!(why.contains("permission denied"), "{why}"),
            ComposePs::Answered(_) => {
                panic!("a line nothing could parse must not read as 'answered: nothing'")
            }
        }
    }

    #[test]
    fn running_services_lists_only_running_and_errors_on_unknown() {
        let out = "{\"Service\":\"a\",\"State\":\"running\",\"Health\":\"\"}\n\
                    {\"Service\":\"b\",\"State\":\"restarting\",\"Health\":\"\"}\n";
        assert_eq!(running_services(out).unwrap(), vec!["a".to_string()]);
        assert!(running_services("not json at all").is_err());
    }
}
