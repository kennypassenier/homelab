//! The scheduler: the status loop and the main scheduler loop that runs the night, the watches and the periodic rounds.

use crate::*;

/// feat-platform-2: one reading every `status_interval_s`, the managed
/// stacks taken from state.json each time so a new stack is read at once.
pub(crate) async fn status_loop(state: AppState) {
    let exec = RealExecutor;
    let mut sealed_at: Option<u64> = None;
    loop {
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let hs = store.load().await.unwrap_or_default();
        let targets: Vec<homelab_core::ops::livestatus::Target> = hs
            .stacks
            .iter()
            .map(|(name, s)| homelab_core::ops::livestatus::Target {
                vmid: s.vmid,
                stack: name.clone(),
                units: s.natives.iter().map(|n| n.unit.clone()).collect(),
            })
            .collect();
        let reading = homelab_core::ops::livestatus::read(&exec, &targets, unix_now()).await;
        for (vmid, why) in &reading.probe_errors {
            tracing::debug!(vmid, "status reading :: {}", why);
        }
        if let Ok(mut slot) = state.live_status.write() {
            *slot = Some(reading);
        }
        // redesign-stackhub-1: after the status reading, so the first one is
        // not held up by asking every container about its secret files.
        if sealed_at.is_none_or(|t| unix_now().saturating_sub(t) >= ENV_SEALED_INTERVAL_S) {
            refresh_env_sealed(&state, &exec).await;
            sealed_at = Some(unix_now());
        }
        // fix-175: the host's own CPU, same cadence as the guest reading —
        // busy share since the previous poll, from two /proc/stat samples.
        // The first poll of this daemon's life has no previous sample, so
        // it stores one and reports unknown rather than a fabricated 0.
        if let Ok(stat) = exec.read_file("/proc/stat").await
            && let Some(curr) = homelab_core::ops::hostcpu::parse_proc_stat(&stat)
        {
            let prev = state.host_cpu_prev.read().ok().and_then(|p| *p);
            let pct = prev.and_then(|p| homelab_core::ops::hostcpu::cpu_pct(&p, &curr));
            if let Ok(mut slot) = state.host_cpu_pct.write() {
                *slot = pct.map(u64::from);
            }
            if let Ok(mut slot) = state.host_cpu_prev.write() {
                *slot = Some(curr);
            }
        }
        tokio::time::sleep(Duration::from_secs(state.config.status_interval_s)).await;
    }
}

pub(crate) async fn scheduler_loop(state: AppState) {
    // fix-122: the nightly updates run inside a span naming their stack.
    use tracing::Instrument as _;
    let exec = RealExecutor;
    // fix-129: the first look soon after start, then every 20 minutes.
    let mut wait = Duration::from_secs(SCHEDULER_FIRST_CHECK_S);
    loop {
        tokio::time::sleep(wait).await;
        wait = Duration::from_secs(20 * 60);
        // fix-52 residual: the loop woke up on its own, which is the one
        // thing an `await` stuck forever cannot do.
        touch_scheduler_heartbeat(&state);
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
        // password-chain-bus-factor: the standing checks exist whether or
        // not nightly backups are scheduled — a disabled scheduler must not
        // also silence the one question that is on a clock rather than a
        // deploy.
        {
            let tick_now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            record_state(&store, "ensure standing checks", |s| {
                homelab_core::ops::manualchecks::ensure_standing_checks(s, tick_now)
            })
            .await;
        }
        let (hour, tiers) = {
            let s = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            match s.backup_hour {
                Some(h) => (h, s.retention.clone()),
                None => continue, // scheduler disabled
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Host-local hour without pulling in chrono: read from `date`.
        let local_hour = match exec.run(&Cmd::new("date", &["+%H"], 10)).await {
            Ok(out) => parse_local_hour(&out.stdout),
            Err(_) => None,
        };
        let Some(local_hour) = local_hour else {
            tracing::error!(
                "scheduler: cannot determine local hour ('date' failed) — nightly run skipped THIS TICK; investigate"
            );
            continue;
        };
        if !in_night_window(hour, local_hour) {
            continue;
        }
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let snapshot = match store.load().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("scheduler: state unreadable — skipping this tick: {}", e);
                continue;
            }
        };
        // H10: one plan per tick — stacks that are due, then the host's own
        // crown jewels. Decided by a pure function so it is unit-tested.
        let stack_inputs = nightly_stack_inputs(&snapshot);
        // G14: taken before the loop below consumes `snapshot.stacks`.
        let drill_repos: Vec<String> = homelab_core::ops::restoredrill::all_drill_repos(
            &snapshot
                .stacks
                .iter()
                .map(|(name, st)| {
                    (
                        st.manifest
                            .as_ref()
                            .map(|m| m.storage.clone())
                            .unwrap_or_default(),
                        name.clone(),
                        // fix-115: only units that have a repository.
                        homelab_core::ops::restoredrill::backed_up_units(&st.natives),
                    )
                })
                .collect::<Vec<_>>(),
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
        );
        // fix-62: whose turn it is, read before the loop below consumes the
        // snapshot's stacks.
        let drill_pick = homelab_core::ops::restoredrill::pick(&snapshot, &drill_repos);
        // fix-62: every stack's vmid and mounts, for `postgres_check` to
        // find whether tonight's repository is a Postgres data directory —
        // also read before the loop below consumes `snapshot.stacks`.
        let pg_stacks: Vec<(u16, Vec<homelab_core::manifest::MountSpec>, String)> = snapshot
            .stacks
            .iter()
            .map(|(name, st)| {
                (
                    st.vmid,
                    st.manifest
                        .as_ref()
                        .map(|m| m.storage.clone())
                        .unwrap_or_default(),
                    name.clone(),
                )
            })
            .collect();
        // fix-96: every repository with the retention its source keeps, for
        // the second copy and the rotating restic check.
        let copy_policies = homelab_core::ops::secondcopy::repo_policies(
            &snapshot,
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
            &tiers,
            state.config.backup.local_dataset.is_some(),
        );
        let plan = nightly_plan(
            hour,
            local_hour,
            now,
            &stack_inputs,
            &NightlyState {
                last_host_meta: snapshot.last_host_meta,
                last_zfs: snapshot.last_zfs,
                last_restore_drill: snapshot.last_restore_drill,
                restore_drill_interval_s: state.config.restore_drill_interval_s,
                zfs_configured: !state.config.zfs_jobs.is_empty(),
                devices_configured: !state.config.device_backups.is_empty(),
                second_copy_configured: state.config.second_copy_dataset.is_some(),
                last_second_copy: snapshot.last_second_copy,
                last_integrity_check: snapshot.last_integrity_check,
            },
        );
        // Y1: every due backup runs first, several at a time, under one
        // hold of the global lock. Then the loop below does the updates one
        // by one — an update replaces running containers, which is a very
        // different risk from reading their data, so it stays serial.
        //
        // Backups before updates also orders the night sensibly on its own:
        // everything is safe on disk before anything is replaced.
        let backup_jobs: Vec<BackupJob> = snapshot
            .stacks
            .iter()
            .filter(|(name, _)| plan.contains(&NightlyTask::Stack((*name).clone())))
            .filter_map(|(name, st)| {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                // fix-138: the same decision the night tests reach.
                let what = match homelab_core::ops::night::backup_work(st)? {
                    homelab_core::ops::night::BackupWork::Native(n) => BackupWhat::Native(n),
                    homelab_core::ops::night::BackupWork::Compose(m) => BackupWhat::Compose(m),
                };
                Some(BackupJob {
                    stack: name.clone(),
                    what,
                    cfg,
                })
            })
            .collect();
        let backup_done =
            run_backup_batch(&state, &exec, backup_jobs, state.config.backup_concurrency).await;
        // fix-52 residual: the batch — which can run for hours — returned.
        touch_scheduler_heartbeat(&state);

        // fix-138 (expert panel, host-monolith-untested, 2026-09-27): what
        // each stack's night does is decided by `ops::night::stack_night`,
        // which the night tests reach; this loop only runs the operations,
        // writes state and logs.
        let updates_parked = snapshot.updates_parked.clone();
        for (name, st) in snapshot.stacks {
            let due = plan.contains(&NightlyTask::Stack(name.clone()));
            let backup = backup_done
                .get(&name)
                .cloned()
                .unwrap_or(NightBackup::Failed);
            let night = homelab_core::ops::night::stack_night(
                &name,
                &st,
                due,
                updates_parked.contains_key(&name),
                &backup,
            );
            for (level, line) in &night.lines {
                match level {
                    homelab_core::sink::Level::Warn | homelab_core::sink::Level::Error => {
                        tracing::warn!("{}", line)
                    }
                    _ => info!("{}", line),
                }
            }
            if night.record_last_backup {
                // Record last_backup so tomorrow's check is accurate.
                record_state(&store, "last_backup", |s| {
                    if let Some(rec) = s.stacks.get_mut(&name) {
                        rec.last_backup = now;
                    }
                    // fix-nightly-1: only the night's own backup counts as
                    // tonight's.
                    s.last_nightly_backup.insert(name.clone(), now);
                })
                .await;
            }
            let mut update_ok = true;
            match night.updates {
                homelab_core::ops::night::UpdateWork::None => {}
                homelab_core::ops::night::UpdateWork::Compose(m) => {
                    let m2 = *m;
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-update", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::update::update(ctx, &m2, None, true).await
                        })
                    })
                    .instrument(stack_span(&name))
                    .await;
                    update_ok = r.ok;
                }
                homelab_core::ops::night::UpdateWork::Native { release, own_cmd } => {
                    let applied = Some(st.applied_at);
                    // B1: the orchestrator's own release update.
                    for native in release {
                        let r =
                            run_mutating_op(&state, &exec, 0, "scheduled-release-update", |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::release_update(ctx, &native).await
                                })
                            })
                            .instrument(stack_span(&name))
                            .await;
                        update_ok &= r.ok;
                    }
                    for native in own_cmd {
                        let r =
                            run_mutating_op(&state, &exec, 0, "scheduled-update-native", |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::update_native(ctx, &native, applied)
                                        .await
                                })
                            })
                            .instrument(stack_span(&name))
                            .await;
                        update_ok &= r.ok;
                    }
                }
            }
            // app-settings-enforced-1: the values a fix depends on, put back
            // in the apps' own files. After the backup batch and the update
            // (both done with this stack), under the same operation lock as
            // every mutation, so never while a backup of the stack runs. A
            // stack whose update just failed is left alone.
            if update_ok
                && st.enabled
                && !st.is_native()
                && let Some(m) = st.manifest.clone()
                && store
                    .load()
                    .await
                    .is_ok_and(|s| homelab_core::ops::settings::stack_has_settings(&s, &name))
            {
                let _ = run_mutating_op(&state, &exec, 0, "scheduled-settings", |ctx| {
                    Box::pin(async move { homelab_core::ops::settings::enforce(ctx, &m).await })
                })
                .instrument(stack_span(&name))
                .await;
            }
            // H8 / fix-59: only a failed update parks, and only the updates;
            // a failed or deferred backup is simply tried again tomorrow.
            if night.settle_park {
                park_after_night(&state, &exec, &store, &name, update_ok, now).await;
            }
            // fix-52 residual: this stack's update work (which can itself
            // run long) finished without the loop ever stalling on it.
            touch_scheduler_heartbeat(&state);
        }

        // H10: the host's own crown jewels — the secrets vault (holding the
        // only copy of the restic password), state.json, TLS material and the
        // intent repo. Runs even when no stack was due; without it, losing the
        // host disk loses the keys to every backup we ever made.
        if plan.contains(&NightlyTask::HostMeta) {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            let report = run_mutating_op(&state, &exec, 0, "host-meta-backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await },
                )
            })
            .await;
            if report.ok {
                record_state(&store, "last_host_meta", |s| s.last_host_meta = now).await;
            } else {
                tracing::error!(
                    "scheduler: host-meta backup FAILED — the vault/state/TLS snapshot is the recovery path for a lost host disk; investigate now"
                );
            }
        }

        // G14: one restore, rehearsed for real, in turn.
        //
        // Restores into a temporary directory and judges what came back by
        // its LARGEST file rather than its first. That is not fussiness: on
        // 2026-09-02 a hand-run drill declared a restore identical to live by
        // comparing two md5 sums that both belonged to a zero-byte file, and
        // a drill that can be satisfied by empty files rehearses nothing.
        if plan.contains(&NightlyTask::RestoreDrill)
            && let Some(repo) = drill_pick.clone()
        {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            // fix-62 (see REGISTER.md): a data pool, not the state dir on
            // pve-root.
            let target = format!(
                "{}/restore-drill",
                state.config.restore_drill_scratch_dir.trim_end_matches('/')
            );
            let pg = homelab_core::ops::restoredrill::postgres_check(&pg_stacks, &repo);
            let outcome = run_restore_drill(&exec, &cfg, &repo, &target, pg.as_ref()).await;
            match &outcome {
                homelab_core::ops::restoredrill::Outcome::Passed {
                    files,
                    largest_bytes,
                } => info!(
                    "restore drill: {} came back with {} file(s), largest {} bytes",
                    repo, files, largest_bytes
                ),
                homelab_core::ops::restoredrill::Outcome::Failed(why) => {
                    tracing::error!("restore drill: {} proved nothing :: {}", repo, why)
                }
            }
            record_state(&store, "restore drill", |sn| {
                homelab_core::ops::restoredrill::record(sn, &drill_repos, &repo, &outcome, now)
            })
            .await;
        }

        // Route A: the devices this suite may not touch, asked for their own
        // configuration. Best-effort per device — one router refusing does
        // not fail the night for the others, and the failure is a finding
        // rather than a silence.
        if plan.contains(&NightlyTask::DeviceConfig) {
            for dev in state.config.device_backups.clone() {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let name = dev.name.clone();
                let report = run_mutating_op(&state, &exec, 0, "device-backup", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
                    })
                })
                .await;
                if !report.ok {
                    tracing::error!(
                        "scheduler: device backup for {} FAILED — its configuration is not \
                         being kept",
                        name
                    );
                } else {
                    record_device_backup(&state, &name).await;
                }
            }
        }

        // fix-94: the house's public address is dynamic; keep CrowdSec's
        // whitelist on the gateway equal to it. Cheap when nothing changed:
        // one read of the file, one GET against the router, no write.
        // fix-156: every night, whatever else the night does.
        run_mutating_op(&state, &exec, 0, "home-address-whitelist", |ctx| {
            Box::pin(async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await })
        })
        .await;

        // fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): every
        // repository copied into the second repository set, after all of
        // tonight's backups and before the ZFS replication below, so the
        // replica on HDD18TB carries tonight's copy too.
        if plan.contains(&NightlyTask::SecondCopy)
            && let Some(ds) = state.config.second_copy_dataset.clone()
        {
            let cfg = state.config.backup.clone();
            let repos = copy_policies.clone();
            let report = run_mutating_op(&state, &exec, 0, "second-copy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::secondcopy::copy_all(ctx, &cfg, &ds, &repos).await
                })
            })
            .await;
            if !report.ok {
                tracing::error!(
                    "scheduler: the second copy did not complete — the repositories it names \
                         exist on Google Drive only tonight"
                );
            }
        }

        // fix-96: one repository checked per night, both copies; once a month
        // per repository the check also reads a slice of the data. Read after
        // the copy above, which records which repositories have a copy yet.
        if plan.contains(&NightlyTask::IntegrityCheck)
            && let Ok(fresh) = store.load().await
        {
            let names: Vec<String> = copy_policies.iter().map(|p| p.repo.clone()).collect();
            if let Some(repo) = homelab_core::ops::secondcopy::pick(&fresh, &names) {
                let subset = homelab_core::ops::secondcopy::data_subset(
                    &fresh,
                    &repo,
                    now,
                    state.config.integrity_data_read_interval_s,
                );
                let local = state
                    .config
                    .second_copy_dataset
                    .clone()
                    .filter(|_| homelab_core::ops::secondcopy::check_local(&fresh, &repo));
                let cfg = state.config.backup.clone();
                let report = run_mutating_op(&state, &exec, 0, "restic-check", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::secondcopy::check_repo(
                            ctx,
                            &cfg,
                            &repo,
                            local.as_deref(),
                            subset,
                        )
                        .await
                    })
                })
                .await;
                if !report.ok {
                    tracing::error!(
                        "scheduler: restic check found a problem — {}",
                        report.message
                    );
                }
            }
        }

        // E8: ZFS snapshots + replication of the big pools.
        if plan.contains(&NightlyTask::Zfs) {
            let jobs = state.config.zfs_jobs.clone();
            let report = run_mutating_op(&state, &exec, 0, "zfs-replicate", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::zfs::replicate(
                        ctx,
                        &jobs,
                        &tiers,
                        homelab_core::ops::backup::BackupTrigger::Nightly,
                    )
                    .await
                })
            })
            .await;
            if report.ok {
                record_state(&store, "last_zfs", |s| s.last_zfs = now).await;
            } else {
                tracing::error!(
                    "scheduler: ZFS replication FAILED — investigate; the old cron script used to fail silently, this one does not"
                );
            }
        }

        // fix-83 (manual-images-latest-unpinned, 2026-09-27): record the
        // digest every `manual` container runs, and ask each declared
        // upstream for its latest release, so the fleet check below can say
        // when a pinned app has fallen behind. Read-only on the containers;
        // GitHub is asked at most once a night (the answer is cached in
        // state), every call is bounded, and state is loaded again after the
        // reading so a deploy that saved meanwhile is not overwritten.
        if let Ok(snapshot) = store.load().await {
            let (facts, notes) = homelab_core::ops::pins::gather(
                &exec,
                &snapshot,
                &state.config.safety.no_touch,
                now,
                state.config.upstream_max_age_s,
            )
            .await;
            for n in notes {
                info!("{}", n);
            }
            if let Ok(mut s) = store.load().await {
                homelab_core::ops::pins::apply(&mut s, facts);
                let _ = store.save(s.clone()).await;
                // fix-registry-1: a newer pinned version was learned; warm the
                // cache in the background, never in this round's way.
                spawn_cache_prefetch(&state, &s);
                // pinned-digest-pruned-1: keep a copy of every pinned digest.
                spawn_keep_round(&state, &s);
            }
        }

        // fix-143 (Cloudflare nightly comparison, owner decision
        // 2026-10-01): once a night, like the integrity check — this used
        // to run only from the workstation (`homelab check`), so a night
        // nobody ran it went unwatched.
        if let Ok(fresh) = store.load().await
            && homelab_core::ops::restoredrill::due(fresh.last_edge_check, now, 24 * 3600)
        {
            run_nightly_edge_check(&state, &exec, now).await;
        }

        // Y4: after the night's work, hold the record against the machine.
        // Unconditional, because the findings this exists for are precisely
        // the ones that produce no failure of their own: a stack whose
        // hostname drifted, a backup that quietly stopped, a route that leads
        // nowhere. Stack-file facts are the client's to supply, so the
        // nightly pass runs without them.
        // G15 of the Phase-7 gate: this used to run only when the night had
        // work to do. Backwards — the findings this check exists for are
        // precisely the ones that produce no failure of their own, so a night
        // where nothing was due is a night where nothing was watched either.
        // A hostname that drifted, a route that leads nowhere and a template
        // that no longer exists do not wait for a backup to be due.
        {
            // The stack-file half is deliberately empty here and it is not an
            // oversight: those files live in the CLIENT's repository, not on
            // the host, so "does this file target a vmid somebody else owns"
            // is a question `homelab check` asks from the workstation. The
            // host answers everything it can actually see.
            let mut live = gather_live_facts(&exec, &state, &[], true).await;
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
                    homelab_core::ops::facts::container_file_hashes(&exec, &stacks_with_hashes)
                        .await;
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
                    record_state(&store, "fleet report fingerprint", |sn| {
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
                        &state,
                        &exec,
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

        // fix-131: the daemon's own records, bounded every night. Under the
        // operation lock, so no operation appends to the journal while it
        // is cut; off the async workers, since both are plain file work.
        {
            let _guard = state.op_lock.lock().await;
            let dir = state.config.state_dir.clone();
            let max_age_days = state.config.incident_bundle_max_age_days;
            let max_count = state.config.incident_bundle_max_count;
            let journal_max_bytes = state.config.journal_max_bytes;
            let _ = tokio::task::spawn_blocking(move || {
                compact_journal_file(&dir, journal_max_bytes);
                prune_incidents(&dir, now, max_age_days, max_count);
            })
            .await;
        }
    }
}
