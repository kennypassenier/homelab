//! The nightly fleet check: hold the record against the machine and report what is alarming.

use super::Night;
use crate::*;

/// Y4: after the night's work, hold the record against the machine.
pub(super) async fn fleet_check(n: &Night<'_>) {
    let (state, exec, store) = (n.state, n.exec, &n.store);
    let now = n.now;
    // The stack-file half is deliberately empty here and it is not an
    // oversight: those files live in the CLIENT's repository, not on
    // the host, so "does this file target a vmid somebody else owns"
    // is a question `homelab check` asks from the workstation. The
    // host answers everything it can actually see.
    let mut live = gather_live_facts(exec, state, &[], true).await;
    if let Ok(snapshot) = store.load().await {
        // fix-142: the in-container half of the nightly hash
        // comparison — only the host can ask this, since it needs
        // `pct exec`, so it runs here and nowhere else. Only stacks
        // with a recorded push (deployed since this was built) are
        // asked.
        let stacks_with_hashes: Vec<(String, u16)> = snapshot
            .stacks
            .iter()
            .filter(|(_, st)| !st.pushed_file_hashes.is_empty())
            .map(|(name, st)| (name.clone(), st.vmid))
            .collect();
        live.container_file_hashes =
            homelab_core::ops::facts::container_file_hashes(exec, &stacks_with_hashes).await;
        let findings = homelab_core::ops::fleetcheck::evaluate(
            &snapshot,
            &live,
            now,
            state.config.backup_max_age_s,
            homelab_core::ops::fleetcheck::GrowthLimits::default(),
            state.config.tile_watch_source.as_deref(),
            &state.config.trusted_sources,
            state.config.patch_threshold_s,
            state.config.host_meta_max_age_s,
            state.config.capacity_thresholds,
        );
        // Z3, now a tested function in core rather than a filter
        // buried in this loop (G15).
        let problems = homelab_core::ops::fleetcheck::alarming(&findings);
        // fix-65: the same alarming set is sent once, then weekly
        // while it stands; a new or changed set goes out that night.
        let fingerprint = homelab_core::ops::fleetcheck::report_fingerprint(&findings);
        let send = !problems.is_empty()
            && homelab_core::ops::fleetcheck::nightly_report_due(
                &fingerprint,
                &snapshot.last_fleet_report_fp,
                snapshot.last_fleet_report_at,
                now,
                state.config.nightly_report_repeat_s,
            );
        if send || (problems.is_empty() && !snapshot.last_fleet_report_fp.is_empty()) {
            // Remember what went out; forget it once nothing is
            // alarming, so a problem that comes back is sent again.
            let fp = fingerprint.clone();
            record_state(store, "fleet report fingerprint", |sn| {
                if send {
                    sn.last_fleet_report_fp = fp;
                    sn.last_fleet_report_at = now;
                } else {
                    sn.last_fleet_report_fp.clear();
                }
            })
            .await;
        }
        if problems.is_empty() {
            info!(
                "fleet check: repo and reality agree{}",
                if findings.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", render_findings(&findings))
                }
            );
        } else if !send {
            tracing::warn!(
                "fleet check: {} finding(s), the same alarming set as reported on {} — \
                         not sent again until it changes or a week has passed\n{}",
                findings.len(),
                homelab_core::state::ymd(snapshot.last_fleet_report_at),
                render_findings(&findings)
            );
        } else {
            tracing::warn!("{}", render_findings(&findings));
            // The finding text is already in the log above; the
            // notice exists so it leaves the machine. Decision
            // notify-routing (2026-09-30): the whole report goes to
            // the dashboard's centre; the phone only when something
            // is broken (F86: one payload shape for every event).
            let broken = findings
                .iter()
                .filter(|f| f.severity == homelab_core::ops::fleetcheck::Severity::Broken)
                .count();
            publish_notice(
                state,
                exec,
                NoticeFacts {
                    op: "fleet-check".into(),
                    label: "nightly".into(),
                    ok: false,
                    deferred: false,
                    since: now,
                    ex: homelab_core::notify::explain_fleet_check(&findings),
                    urgency: homelab_core::notify::urgency(
                        &homelab_core::notify::Event::FleetCheck { broken },
                    ),
                    incident: None,
                    req: None,
                    by: None,
                    findings: findings.clone(),
                },
            )
            .await;
        }
    }
}
