//! checks-automate (Kenny, 2026-09-30: "Alles wat kan"): the manual
//! questions a machine can answer, measured every night.
//!
//! A manual question ("open qBittorrent and look whether any torrent is on
//! missingFiles") was answered once a quarter at best. A probe asks the app
//! itself, inside its container, in every fleet check: the deploy registers
//! the stack's probes, the fleet check reads each one, and a reading outside
//! `healthy` is a finding that names the app and links to it.

use std::collections::BTreeMap;

use crate::checks::ServiceChecks;
use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::{HostState, ProbeRecord};

/// Stable while the stack, app and name are; a renamed probe is a new one.
pub fn id_for(stack: &str, app: &str, name: &str) -> String {
    crate::ops::manualchecks::id_for(stack, app, &format!("probe:{}", name))
}

/// Replace the stack's probes with what its deploy declared. A probe that
/// left the stack files leaves the state; other stacks are untouched.
pub fn register(
    state: &mut HostState,
    stack: &str,
    vmid: u16,
    checks: &BTreeMap<String, ServiceChecks>,
) {
    state.probes.retain(|_, r| r.stack != stack);
    for (app, sc) in checks {
        for p in &sc.probes {
            // fix-182: an explicit `id:` is a stable reference the same way
            // a manual check's is, but probes carry no answer of their own
            // (the whole stack's probe set is replaced on every deploy,
            // right above) — so there is nothing to migrate, only a nicer
            // name to register under.
            let id = p.id.clone().unwrap_or_else(|| id_for(stack, app, &p.name));
            state.probes.insert(
                id,
                ProbeRecord {
                    stack: stack.to_string(),
                    app: app.clone(),
                    vmid,
                    probe: p.clone(),
                    url: sc.url.clone(),
                },
            );
        }
    }
}

/// One probe's reading from a fleet check: the trimmed stdout, or why the
/// command gave none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReading {
    pub id: String,
    pub reading: Result<String, String>,
}

/// Findings for every probe that was read: Broken when the reading is not
/// healthy, Drift when it could not be read. A probe with no reading (its
/// container was not asked) is silent: an unasked question is never a
/// finding.
pub fn evaluate(state: &HostState, readings: &[ProbeReading]) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in readings {
        let Some(rec) = state.probes.get(&r.id) else {
            continue;
        };
        let link = rec
            .url
            .as_ref()
            .map(|u| format!(" ({})", u))
            .unwrap_or_default();
        match &r.reading {
            Ok(v) if rec.probe.healthy.judge(v) => {}
            Ok(v) => out.push(Finding {
                severity: Severity::Broken,
                subject: format!("{}/{}", rec.stack, rec.app),
                what: format!(
                    "{}: reads \"{}\", healthy is {}{}",
                    rec.probe.name,
                    v.trim(),
                    rec.probe.healthy.describe(),
                    link
                ),
                remedy: format!(
                    "open {} and look at it",
                    rec.url.as_deref().unwrap_or(&rec.app)
                ),
            }),
            Err(why) => out.push(Finding {
                severity: Severity::Drift,
                subject: format!("{}/{}", rec.stack, rec.app),
                what: format!("{}: could not be read ({}){}", rec.probe.name, why, link),
                remedy: "the probe's command in checks.yml no longer answers; run it in the \
                         container to see why"
                    .into(),
            }),
        }
    }
    out.sort_by(|a, b| a.subject.cmp(&b.subject).then(a.what.cmp(&b.what)));
    out
}
