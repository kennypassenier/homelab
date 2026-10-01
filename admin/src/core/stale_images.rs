//! feat-stacks-10 (overview of stale docker images): the fleet check
//! already answers one `noted` finding per pinned image whose upstream has
//! moved on (fix-83, `homelab_core::ops::pins::evaluate_pins`) — this page
//! adds nothing new to ask the host; it reads the same `/data/fleet-check`
//! findings the Health page already shows and turns the ones about pinned
//! images into a table, instead of a sentence to parse by eye.
//!
//! The finding's `subject` is "stack/container[, stack/container, …]" and
//! its `what` is "pinned to {version}; upstream {upstream} released
//! {latest} on {date}" (the exact wording `evaluate_pins` writes); this
//! module reads that back out rather than re-deriving it from host state,
//! so the table can never disagree with the fleet check's own sentence.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleImageRow {
    /// "stack/container".
    pub where_: String,
    pub pinned: String,
    pub upstream: String,
    pub latest: String,
    /// `YYYY-MM-DD`, when the finding's sentence carried one.
    pub released: Option<String>,
}

/// One finding as `/data/fleet-check` answers it: only the two fields this
/// reads.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FindingLike {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub what: String,
}

/// `"pinned to {version}; upstream {upstream} released {latest}[ on
/// {date}]"` — parsed back into its parts. None when the sentence does not
/// start the way `evaluate_pins` writes it (a finding from a different
/// check, or the wording changed underneath this reader).
fn parse_what(what: &str) -> Option<(String, String, String, Option<String>)> {
    let rest = what.strip_prefix("pinned to ")?;
    let (version, rest) = rest.split_once("; upstream ")?;
    let (upstream, rest) = rest.split_once(" released ")?;
    let (latest, date) = match rest.split_once(" on ") {
        Some((l, d)) => (l, Some(d.to_string())),
        None => (rest, None),
    };
    Some((
        version.to_string(),
        upstream.to_string(),
        latest.to_string(),
        date,
    ))
}

/// One table row per `stack/container` named in a finding (a finding may
/// cover several, when the same version/upstream pair shows up more than
/// once in the fleet).
pub fn from_findings(findings: &[FindingLike]) -> Vec<StaleImageRow> {
    let mut out = Vec::new();
    for f in findings {
        let Some((pinned, upstream, latest, released)) = parse_what(&f.what) else {
            continue;
        };
        for w in f.subject.split(", ").filter(|s| !s.is_empty()) {
            out.push(StaleImageRow {
                where_: w.to_string(),
                pinned: pinned.clone(),
                upstream: upstream.clone(),
                latest: latest.clone(),
                released: released.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.where_.cmp(&b.where_));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(subject: &str, what: &str) -> FindingLike {
        FindingLike {
            subject: subject.into(),
            what: what.into(),
        }
    }

    #[test]
    fn parses_one_finding_with_a_date() {
        let rows = from_findings(&[f(
            "media/sonarr",
            "pinned to 4.0.1; upstream github.com/Sonarr/Sonarr released 4.0.2 on 2026-09-20",
        )]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].where_, "media/sonarr");
        assert_eq!(rows[0].pinned, "4.0.1");
        assert_eq!(rows[0].upstream, "github.com/Sonarr/Sonarr");
        assert_eq!(rows[0].latest, "4.0.2");
        assert_eq!(rows[0].released.as_deref(), Some("2026-09-20"));
    }

    #[test]
    fn parses_a_finding_naming_several_containers() {
        let rows = from_findings(&[f(
            "media/radarr, paperwork/paperless",
            "pinned to 1.2.3; upstream github.com/x/y released 1.3.0",
        )]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].where_, "media/radarr");
        assert_eq!(rows[1].where_, "paperwork/paperless");
        assert!(rows[0].released.is_none());
    }

    #[test]
    fn a_finding_from_a_different_check_is_skipped() {
        let rows = from_findings(&[f(
            "gateway",
            "the files differ from what the host applied on 2026-09-20",
        )]);
        assert!(rows.is_empty());
    }

    #[test]
    fn rows_are_sorted_by_where() {
        let rows = from_findings(&[
            f("z/app", "pinned to 1; upstream u released 2"),
            f("a/app", "pinned to 1; upstream u released 2"),
        ]);
        assert_eq!(rows[0].where_, "a/app");
        assert_eq!(rows[1].where_, "z/app");
    }
}
