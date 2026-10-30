//! Gathering what needs a person today and the live facts the doctor and the dashboard show.

use crate::*;

/// Y4: read off the machine what the pure comparison needs. Kept separate so
/// the judgement stays testable without a fleet.
///
/// G6 (T79): the gathering itself now lives in `homelab_core::ops::facts`,
/// behind an executor, with tests; this is the host's thin adapter that
/// hands it the configuration and logs what it measured.
/// fix-68: the three readings `homelab today` merges — doctor, the fleet
/// check with its manual checks, and the incident bundles — gathered the
/// same way their own verbs gather them.
pub(crate) async fn gather_today(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
    digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
    host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
) -> homelab_core::ops::today::Today {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // The doctor's readings and the fleet check's are independent, so they
    // are taken side by side (ops::pool) rather than one after the other:
    // 27 s + 67 s on pve on 2026-09-29, before either was made concurrent.
    // They share no round: doctor asks each recorded stack whether its
    // container exists and its secret files are sealed; the check asks each
    // container about disk, memory, logs, configuration and updates.
    let doctor_fut = async {
        let mut probes = gather_probes(
            exec,
            &state.config.state_dir,
            state.config.mirror_remote.as_deref(),
            state.config.backup.staging_dir.as_deref(),
            &state.config.restore_drill_scratch_dir,
            now,
            &state.snapshot_cache,
            &state.config.backup,
            &|line: &str| progress_line(state, line),
        )
        .await;
        probes.failed_auth = Some(state.auth_failures.snapshot());
        gather_security_probes(exec, &ProbeContext::of(&state.config), now, &mut probes).await;
        homelab_core::doctor::diagnose(&probes)
    };
    let (checks, mut live) = futures_util::join!(
        doctor_fut,
        gather_live_facts(exec, state, stack_files, false)
    );
    live.digests = digests;
    // fix-110: as `Rpc::FleetCheck`, for the `homelab today` path.
    live.declared_host_config = host_config;
    live.live_host_config = live_host_config_table(
        &std::fs::read_to_string(&state.config.config_path).unwrap_or_default(),
    );
    let incidents: Vec<String> = std::fs::read_dir(format!("{}/incidents", state.config.state_dir))
        .map(|rd| {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    match homelab_core::state::StateStore::new(exec, &state.config.state_dir)
        .load()
        .await
    {
        Ok(snapshot) => {
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
            let mut t =
                homelab_core::ops::today::assemble(&checks, &findings, &incidents, &snapshot, now);
            // fix-step-plan-nested: step-counter faults, journalled by the
            // runner instead of failing the op.
            let journal =
                std::fs::read_to_string(format!("{}/journal.jsonl", state.config.state_dir))
                    .unwrap_or_default();
            homelab_core::ops::today::add_plan_violations(
                &mut t,
                &homelab_core::ops::today::plan_violations(&journal, now),
            );
            t
        }
        Err(e) => {
            let mut t =
                homelab_core::ops::today::assemble(&checks, &[], &[], &Default::default(), now);
            t.unread.push(format!(
                "state unreadable, so the fleet check and the incidents were not read: {}",
                e
            ));
            t
        }
    }
}

/// `watched_fresh`: true lists every watched backup on its remote and records
/// the answer (the nightly round); false reads what the host recorded
/// (`homelab check`, `homelab today`; decision "Fleet check speed").
pub(crate) async fn gather_live_facts(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
    watched_fresh: bool,
) -> homelab_core::ops::fleetcheck::LiveFacts {
    use homelab_core::ops::facts::{FactsInputs, WatchedBackupSpec};
    let inp = FactsInputs {
        watched_backups: state
            .config
            .watched_backups
            .iter()
            .map(|w| WatchedBackupSpec {
                name: w.name.clone(),
                rclone_path: w.rclone_path.clone(),
                max_age_hours: w.max_age_hours,
            })
            .collect(),
        state_dir: state.config.state_dir.clone(),
        gateway_vmid: state.config.safety.gateway_vmid,
        gateway_routes_dir: state.config.safety.gateway_routes_dir.clone(),
        no_touch: state.config.safety.no_touch.to_vec(),
        prometheus_url: state.config.prometheus_url.clone(),
        loki_url: state.config.loki_url.clone(),
        loki_vmid: state.config.loki_vmid,
        logs_window: sane_window(&state.config.logs_window),
        now_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        watched_fresh,
    };
    // fix-104: each phase goes to whoever is watching; the check took 41 s
    // with nothing on the screen after "link up".
    let progress = |line: &str| progress_line(state, line);
    let (mut facts, notes) =
        homelab_core::ops::facts::gather_live_facts_with(exec, &inp, stack_files, &progress).await;
    for n in notes {
        info!("{}", n);
    }
    // fix-96: so the check can say when a configured second copy stops.
    facts.second_copy_dataset = state.config.second_copy_dataset.clone();
    // fix-218: the same real-snapshot reading `homelab doctor` uses, so the
    // fleet check (and `homelab today`, and the dashboard's Health) never
    // disagree with doctor about whether a stack has actually been backed up.
    facts.backup_snapshots = gather_backup_snapshot_facts(
        &state.config.state_dir,
        &state.snapshot_cache,
        &state.config.backup,
    )
    .await;
    // rule-20: pve's own capacity — not a managed container's rootfs
    // (`growth`) and not a stack's declared data pool (`pools`).
    facts.host_capacity = gather_host_capacity(
        exec,
        state.config.prometheus_url.as_deref(),
        state.config.tsdb_retention_size_mib,
        state.config.backup.staging_dir.as_deref(),
        state.config.backup.staging_cap_mib,
    )
    .await;
    facts
}
