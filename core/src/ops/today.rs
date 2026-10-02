//! fix-68 · one answer to "is anything waiting for me?".
//!
//! On 2026-09-27 the house gave four answers in the same minute: `doctor`
//! said Ok, `check` listed nine findings with one broken, `status` printed
//! 1,473 lines of raw state, and the TUI ticker said "ALL SYSTEMS NOMINAL"
//! (four-answers-to-is-anything-wrong). The morning check took four verbs,
//! and the operator combined them in his head.
//!
//! This merges doctor, the fleet check (which already carries the open manual
//! checks) and the open incident bundles into one list, most severe first,
//! one line and one remedy per item, and one verdict. Pure: the host gathers,
//! this decides, and the command line and the TUI both render the same value.

use serde::{Deserialize, Serialize};

use crate::doctor::{Check, Health};
use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::HostState;

/// How an incident bundle for something the host does not track (a stack not
/// in state, a nightly-round label) stays on the list: one day, long enough
/// for the morning after.
pub const UNTRACKED_INCIDENT_WINDOW_S: u64 = 24 * 3600;

/// How long a step-counter fault (a plan violation) stays on the list: a
/// week, so one seen overnight is still there when someone next looks.
pub const PLAN_VIOLATION_WINDOW_S: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    /// Not doing its job right now.
    Broken,
    /// Working, but it needs a person before it bites.
    Attention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub level: Level,
    /// Which part said so: doctor, check or incident.
    pub source: String,
    /// What is wrong, in one line.
    pub what: String,
    /// The one command or key that addresses it.
    pub remedy: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Today {
    pub items: Vec<Item>,
    /// Parts that could not be read. Never folded into "nothing needs you":
    /// a question that was not asked is not a question answered.
    pub unread: Vec<String>,
}

impl Today {
    /// "Nothing needs you", or how many things do.
    pub fn verdict(&self) -> String {
        match (self.items.len(), self.unread.is_empty()) {
            (0, true) => "Nothing needs you".into(),
            (0, false) => format!(
                "Nothing found, but {} part(s) could not be read",
                self.unread.len()
            ),
            (1, _) => "1 thing needs you".into(),
            (n, _) => format!("{} things need you", n),
        }
    }

    pub fn needs_you(&self) -> bool {
        !self.items.is_empty() || !self.unread.is_empty()
    }
}

/// An incident bundle is open while nothing has succeeded on its stack since:
/// no deploy (`applied_at`) and no backup (`last_backup`) newer than the
/// bundle. A bundle whose stack the host does not track stays open for
/// [`UNTRACKED_INCIDENT_WINDOW_S`].
///
/// Bundle names are `<unix-seconds>-<operation label>`, and a stack's labels
/// end in `-<stack>` (`deploy-kyu`, `update-kp-soft`).
pub fn open_incidents(names: &[String], state: &HostState, now: u64) -> Vec<String> {
    let mut out = Vec::new();
    for name in names {
        let Some((ts, label)) = name.split_once('-') else {
            continue;
        };
        let Ok(ts) = ts.parse::<u64>() else {
            continue;
        };
        // The longest stack name the label ends in: `update-kp-soft` is
        // kp-soft's, not a stack called `soft`.
        let stack = state
            .stacks
            .iter()
            .filter(|(s, _)| label.ends_with(&format!("-{}", s)))
            .max_by_key(|(s, _)| s.len());
        let open = match stack {
            Some((_, st)) => ts > st.applied_at.max(st.last_backup),
            None => now.saturating_sub(ts) < UNTRACKED_INCIDENT_WINDOW_S,
        };
        if open {
            out.push(name.clone());
        }
    }
    out
}

/// fix-step-plan-nested: the plan violations the operation journal
/// (`journal.jsonl`, one JSON object per line) recorded within
/// [`PLAN_VIOLATION_WINDOW_S`] of `now`, oldest first. Lines that do not
/// parse are skipped: the journal is read, never judged, here.
pub fn plan_violations(journal: &str, now: u64) -> Vec<String> {
    journal
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["step"] == crate::runner::PLAN_VIOLATION_STEP)
        .filter(|v| now.saturating_sub(v["ts"].as_u64().unwrap_or(0)) < PLAN_VIOLATION_WINDOW_S)
        .filter_map(|v| v["status"].as_str().map(str::to_string))
        .collect()
}

/// Adds one attention item per plan violation. The op itself ran normally;
/// only the step counter it announced was wrong, so this is a code fault to
/// report, never a broken service.
pub fn add_plan_violations(today: &mut Today, violations: &[String]) {
    for v in violations {
        today.items.push(Item {
            level: Level::Attention,
            source: "step counter".into(),
            what: v.clone(),
            remedy: "the operation itself ran normally; its announced step plan was wrong — \
                     file it against the op named here"
                .into(),
        });
    }
    today.items.sort_by_key(|i| i.level);
}

/// Merge the three readings into one list, most severe first. Noted findings
/// and healthy doctor lines are left out: they need nobody.
pub fn assemble(
    doctor: &[Check],
    findings: &[Finding],
    incidents: &[String],
    state: &HostState,
    now: u64,
) -> Today {
    let mut items = Vec::new();
    for c in doctor {
        let level = match c.health {
            Health::Ok => continue,
            Health::Warn => Level::Attention,
            Health::Fail => Level::Broken,
        };
        items.push(Item {
            level,
            source: "doctor".into(),
            what: format!("{}: {}", c.name, c.detail),
            remedy: c
                .remedy
                .clone()
                .unwrap_or_else(|| "`homelab doctor` shows the detail".into()),
        });
    }
    for f in findings {
        let level = match f.severity {
            Severity::Noted => continue,
            Severity::Drift => Level::Attention,
            Severity::Broken => Level::Broken,
        };
        items.push(Item {
            level,
            source: "check".into(),
            what: format!("{}: {}", f.subject, f.what),
            remedy: f.remedy.clone(),
        });
    }
    for name in open_incidents(incidents, state, now) {
        items.push(Item {
            level: Level::Broken,
            source: "incident".into(),
            what: format!(
                "{} failed and nothing on its stack has succeeded since",
                name
            ),
            remedy: format!(
                "read /var/lib/homelab/incidents/{}/report.json, fix the cause and run the \
                 operation again",
                name
            ),
        });
    }
    // Stable: within a level the order doctor, check, incident is kept.
    items.sort_by_key(|i| i.level);
    Today {
        items,
        unread: Vec::new(),
    }
}

/// The plain-text form, for the command line and the TUI panel alike.
pub fn render(today: &Today) -> String {
    let mut s = String::new();
    for i in &today.items {
        let tag = match i.level {
            Level::Broken => "broken",
            Level::Attention => "attention",
        };
        s.push_str(&format!(
            "  [{}] {} ({})\n      → {}\n",
            tag, i.what, i.source, i.remedy
        ));
    }
    for u in &today.unread {
        s.push_str(&format!("  [unread] {}\n", u));
    }
    s.push_str(&today.verdict());
    s
}
