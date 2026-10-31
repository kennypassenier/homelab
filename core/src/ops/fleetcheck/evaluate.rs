//! The whole comparison, as one pure function, in the order the report reads.

use super::stack_record::stack_findings;
use super::{
    Finding, GrowthLimits, HostCapacityThresholds, LiveFacts, dead_route_findings,
    evaluate_big_logs, evaluate_boot, evaluate_container_drift, evaluate_coverage,
    evaluate_firewalls, evaluate_growth, evaluate_host_capacity, evaluate_host_config_drift,
    evaluate_host_meta, evaluate_incomplete, evaluate_notify, evaluate_owners,
    evaluate_patch_state, evaluate_pools, evaluate_repo_drift, evaluate_restore_checks,
    evaluate_route_owners, evaluate_watched_backups,
};
use crate::state::HostState;

/// The whole comparison, as one pure function.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
    growth_limits: GrowthLimits,
    tile_watch_source: Option<&str>,
    trusted_sources: &[String],
    patch_threshold_s: u64,
    host_meta_max_age_s: u64,
    host_capacity_limits: HostCapacityThresholds,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // fix-150: updates standing still for longer than a week.
    out.extend(evaluate_patch_state(&live.patch, patch_threshold_s));

    // rule-20: pve's own capacity — root fs, thin pool, journald, every ZFS
    // pool, Prometheus' TSDB.
    out.extend(evaluate_host_capacity(
        &live.host_capacity,
        host_capacity_limits,
    ));

    out.extend(stack_findings(state, live, now_unix, backup_max_age_s));

    // fix-142: the files against what the host applied.
    out.extend(repo_findings(state, live));

    out.extend(resource_findings(live, growth_limits));
    out.extend(declaration_findings(
        state,
        live,
        tile_watch_source,
        trusted_sources,
    ));
    out.extend(recorded_findings(
        state,
        live,
        now_unix,
        host_meta_max_age_s,
    ));
    out.extend(dead_route_findings(&live.routes));
    out
}

/// F184: why the HOST itself cannot help, when it is oversubscribed or
/// already swapping hard. Read once, so every per-container remedy can say
/// something the machine can actually do.
fn host_short_reason(live: &LiveFacts) -> Option<String> {
    live.host_memory.and_then(|(total, committed, su, st)| {
        let oversubscribed = committed > total;
        let swap_pressed = st > 0 && su * 100 / st.max(1) >= 50;
        (oversubscribed || swap_pressed).then(|| {
            format!(
                "the host has {} MB of RAM with {} MB promised to guests, and {} of its {} MB \
                 of swap in use",
                total, committed, su, st
            )
        })
    })
}

/// Container growth, data pools, oversized logs and storage ownership.
fn resource_findings(live: &LiveFacts, growth_limits: GrowthLimits) -> Vec<Finding> {
    let host_short = host_short_reason(live);
    let mut out = Vec::new();
    out.extend(evaluate_growth(
        &live.growth,
        growth_limits,
        host_short.as_deref(),
    ));
    out.extend(evaluate_pools(&live.pools, growth_limits));
    out.extend(evaluate_big_logs(&live.big_logs));
    out.extend(evaluate_owners(&live.owners));
    out
}

/// What the stack files declare, held against what exists.
fn declaration_findings(
    state: &HostState,
    live: &LiveFacts,
    tile_watch_source: Option<&str>,
    trusted_sources: &[String],
) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(evaluate_route_owners(state, &live.route_files));
    out.extend(evaluate_firewalls(
        state,
        &live.firewalls,
        &live.boot,
        tile_watch_source,
        trusted_sources,
    ));
    out.extend(evaluate_coverage(&live.coverage));
    out.extend(evaluate_boot(state, &live.boot));
    out.extend(evaluate_watched_backups(&live.watched_backups));
    out.extend(evaluate_incomplete(state));
    out
}

/// Everything the host's own records and the sibling ops can say.
fn recorded_findings(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    host_meta_max_age_s: u64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(crate::ops::retired::evaluate_retired(state));
    out.extend(evaluate_notify(state, now_unix));
    // fix-94: a CrowdSec whitelist running on a last known home address.
    out.extend(crate::ops::homeaddress::evaluate_home_address(state));
    // fix-83: a pinned manual app whose upstream released something newer.
    out.extend(crate::ops::pins::evaluate_pins(state));
    out.extend(crate::ops::restoredrill::evaluate_drill(
        state,
        now_unix,
        crate::ops::restoredrill::DEFAULT_DRILL_INTERVAL_S,
    ));
    out.extend(evaluate_host_meta(state, now_unix, host_meta_max_age_s));
    out.extend(evaluate_restore_checks(state));
    // fix-96: the second copy and the rotating restic check.
    out.extend(crate::ops::secondcopy::evaluate_copies(
        state,
        now_unix,
        live.second_copy_dataset.as_deref(),
        crate::ops::secondcopy::DEFAULT_CHECK_INTERVAL_S,
    ));
    out.extend(crate::ops::manualchecks::evaluate_manual(state, now_unix));
    out.extend(crate::ops::probes::evaluate(state, &live.probe_readings));
    out
}

/// The stack files and host.toml against what the host applied (fix-142).
fn repo_findings(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(evaluate_repo_drift(state, live));
    out.extend(evaluate_container_drift(state, live));
    out.extend(evaluate_host_config_drift(live));
    out
}
