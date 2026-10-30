//! The security probes behind the doctor: the probe context and gathering each probe.

use crate::*;

/// fix-130: what the new doctor probes need from the configuration.
pub(crate) struct ProbeContext {
    pub(crate) listen: String,
    pub(crate) exec_enabled: bool,
    pub(crate) config_path: String,
    pub(crate) password_file: String,
    pub(crate) privileged_vmids: Vec<u16>,
    pub(crate) no_touch: Vec<u16>,
    pub(crate) drill_interval_s: u64,
    pub(crate) state_dir: String,
    /// fix-130 (second half): where to read the gateway's routes directory,
    /// to name any file in it that no stack declares (the same judgement the
    /// fleet check carries, `fleetcheck::unowned_route_files`).
    pub(crate) gateway_vmid: u16,
    pub(crate) gateway_routes_dir: String,
}

impl ProbeContext {
    pub(crate) fn of(config: &Config) -> Self {
        ProbeContext {
            listen: config.listen.to_string(),
            exec_enabled: config.exec_enabled,
            config_path: config.config_path.clone(),
            password_file: config.backup.password_file.clone(),
            privileged_vmids: config.safety.privileged_vmids.clone().unwrap_or_default(),
            no_touch: config.safety.no_touch.clone(),
            drill_interval_s: config.restore_drill_interval_s,
            state_dir: config.state_dir.clone(),
            gateway_vmid: config.safety.gateway_vmid,
            gateway_routes_dir: config.safety.gateway_routes_dir.clone(),
        }
    }
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the probes
/// doctor lacked. Cheap reads only: `stat`, one loop over the container
/// configurations, state, `test -s` and one `rclone about`. A probe that
/// cannot be read stays `None`, so doctor leaves its line out rather than
/// guess.
pub(crate) async fn gather_security_probes(
    exec: &dyn Executor,
    pc: &ProbeContext,
    now_unix: u64,
    probes: &mut homelab_core::doctor::Probes,
) {
    use homelab_core::doctor::{DrillProbe, DriveSpace, Exposure, Freshness, Privileged};
    probes.exposure = Some(Exposure {
        listen: pc.listen.clone(),
        exec_enabled: pc.exec_enabled,
    });

    // Private files: group or world bits set is a finding. A path that does
    // not exist prints nothing on stdout, which is right: absent is not loose.
    let private = [
        pc.config_path.clone(),
        pc.password_file.clone(),
        format!("{}/secrets", pc.state_dir),
        format!("{}/tls-key.pem", pc.state_dir),
        format!("{}/audit.log", pc.state_dir),
        format!("{}/incidents", pc.state_dir),
    ];
    let mut args: Vec<&str> = vec!["-c", "%a %n"];
    args.extend(private.iter().map(String::as_str));
    let stat_cmd = Cmd::new("stat", &args, 20);

    // Privileged containers. Templates are left out (the golden -priv
    // template is privileged on purpose and never runs), and the no-touch
    // guests' configurations are not even read: the list is law.
    let skip = pc
        .no_touch
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("|");
    let script = format!(
        "for f in /etc/pve/lxc/*.conf; do v=$(basename \"$f\" .conf); \
         case \"$v\" in {}) continue;; esac; \
         grep -q '^template: 1' \"$f\" && continue; \
         grep -q '^unprivileged: 1' \"$f\" || echo \"$v\"; done",
        if skip.is_empty() { "-".into() } else { skip }
    );
    let privileged_cmd = Cmd::new("sh", &["-c", &script], 20);
    let state_path = format!("{}/state.json", pc.state_dir);
    // Present and not empty; the content never leaves the file.
    let password_cmd = Cmd::new("test", &["-s", &pc.password_file], 10);
    // Drive's space, trash included: pruned packs stay in the trash and
    // count against the quota until it is emptied.
    let offsite_configured = probes.offsite_configured;
    let about_cmd = Cmd::new("rclone", &["about", "gdrive:", "--json"], 60);
    let about_fut = async {
        if offsite_configured {
            Some(exec.run(&about_cmd).await)
        } else {
            None
        }
    };
    // fix-130 (second half): the same `ls -1A` the fleet check runs, so
    // doctor can name an unowned route file without waiting for a nightly
    // round. Any extension counts — a `.bak` is still a file nobody owns.
    let routes_cmd = homelab_core::executor::attach_sh(
        pc.gateway_vmid,
        &format!("ls -1A '{}'", pc.gateway_routes_dir),
        30,
    );
    // ops::pool: six independent reads, overlapped; polled in the order
    // they used to run.
    let (stat_out, privileged_out, state_raw, password_out, about_out, routes_out) = futures_util::join!(
        exec.run(&stat_cmd),
        exec.run(&privileged_cmd),
        exec.read_file(&state_path),
        exec.run(&password_cmd),
        about_fut,
        exec.run(&routes_cmd)
    );

    if let Ok(out) = stat_out {
        probes.loose_files = Some(
            out.stdout
                .lines()
                .filter_map(|l| l.split_once(' '))
                .filter(|(mode, _)| u32::from_str_radix(mode, 8).is_ok_and(|m| m & 0o077 != 0))
                .map(|(mode, path)| format!("{} ({})", path, mode))
                .collect(),
        );
    }

    if let Ok(out) = privileged_out {
        let mut vmids: Vec<u16> = out
            .stdout
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        vmids.sort_unstable();
        let outside_policy = vmids
            .iter()
            .copied()
            .filter(|v| !pc.privileged_vmids.contains(v))
            .collect();
        probes.privileged = Some(Privileged {
            vmids,
            outside_policy,
        });
    }

    // Host-meta and the restore drill, from the record.
    let hs: Option<homelab_core::state::HostState> = state_raw
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    if let Some(hs) = &hs {
        let age = |t: u64| (t > 0).then(|| now_unix.saturating_sub(t) / 3600);
        probes.host_meta = Some(Freshness {
            age_h: age(hs.last_host_meta),
        });
        probes.restore_drill = Some(DrillProbe {
            age_h: age(hs.last_restore_drill),
            interval_h: pc.drill_interval_s / 3600,
            failing: hs
                .restore_drills
                .iter()
                .filter_map(|(repo, r)| r.last_error.as_ref().map(|e| format!("{}: {}", repo, e)))
                .collect(),
        });
    }

    // fix-130 (second half): judged the same way the fleet check judges it
    // (`fleetcheck::unowned_route_files`) — only when both the listing and
    // the state parsed; an unreadable gateway is no fact, not an empty one.
    if let (Some(hs), Ok(out)) = (&hs, &routes_out)
        && out.success()
    {
        let on_disk: Vec<String> = out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        probes.unowned_route_files = Some(homelab_core::ops::fleetcheck::unowned_route_files(
            hs, &on_disk,
        ));
    }

    probes.password_file_ok = password_out.ok().map(|o| o.success());

    if let Some(Ok(out)) = about_out
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&out.stdout)
    {
        let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        if n("total") > 0 {
            probes.drive = Some(DriveSpace {
                total: n("total"),
                free: n("free"),
                trashed: n("trashed"),
            });
        }
    }
}

/// Gather doctor probes (F6). I/O stays here; the verdict logic is in core.
/// H8 hardening: the probe layer now feeds REAL data — backup freshness per
/// stack from state.json, offsite reachability via a quick rclone listing,
/// mirror lag via unpushed-commit count. Generic over the executor so the
/// healthy/broken matrix is testable with MockExecutor.
/// `progress` gets one line per phase (fix-104, long-silences, 2026-09-27:
/// `homelab doctor` took 24 s with nothing on the screen after "link up").
#[allow(clippy::too_many_arguments)]
pub(crate) async fn gather_probes(
    exec: &dyn Executor,
    state_dir: &str,
    mirror_remote: Option<&str>,
    staging_dir: Option<&str>,
    restore_scratch_dir: &str,
    now_unix: u64,
    // fix-218: the same `SnapshotCache` `homelab snapshots` and the dashboard
    // read, so the backup-age line reflects a real repository instead of
    // `StackState::last_backup` (which a vacuous "no services" success can
    // advance with nothing ever archived). A cold entry is kicked for a
    // background refresh (never blocks doctor) via the `Arc` so the spawned
    // task can outlive this call.
    snapshot_cache: &Arc<homelab_core::ops::snapshot_cache::SnapshotCache>,
    backup_cfg: &homelab_core::ops::backup::BackupCfg,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> homelab_core::doctor::Probes {
    use homelab_core::doctor::{Probes, StackProbe};
    progress("doctor: every managed container, its backup age and its sealed secrets…");
    let state_raw = exec.read_file(&format!("{}/state.json", state_dir)).await;
    let state_parses = state_raw
        .as_ref()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
        .unwrap_or(true);

    // Per-stack backup freshness + container presence from state.json.
    // ops::pool: the stacks are asked a few at a time, in state order.
    let stacks: Vec<(String, homelab_core::state::StackState)> = state_raw
        .as_ref()
        .ok()
        .and_then(|raw| serde_json::from_str::<homelab_core::state::HostState>(raw).ok())
        .map(|hs| hs.stacks.into_iter().collect())
        .unwrap_or_default();
    let stacks_fut = homelab_core::ops::pool::bounded(
        stacks
            .iter()
            .map(|(name, st)| async move {
                // Present = Proxmox has a configuration for it, which is what
                // `pct status` answered; read off pmxcfs without pct's 0.45 s of
                // Perl start-up (measured 2026-09-29).
                let present = exec
                    .read_file(&homelab_core::executor::lxc_conf_path(st.vmid))
                    .await
                    .is_ok();
                // gap-27: sealed = every secret file on the container has a
                // vault copy. It was hard-coded true, so the check never fired.
                let env_sealed = !present
                    || homelab_core::ops::facts::unsealed_secret_files(exec, state_dir, name, st)
                        .await
                        .is_empty();
                // fix-218: the real newest snapshot across every repository
                // this stack owns, off the same `SnapshotCache` `homelab
                // snapshots` reads — not `StackState::last_backup`.
                let owners = stack_snapshot_owners(st);
                let (newest_snapshot, snapshot_read) =
                    cached_newest_snapshot_unix(snapshot_cache, backup_cfg, &owners);
                StackProbe {
                    name: name.clone(),
                    snapshot_age_h: newest_snapshot.map(|t| now_unix.saturating_sub(t) / 3600),
                    snapshot_read,
                    container_present: present,
                    env_sealed,
                    // fix-130: a native stack's services are backed up whole.
                    nothing_to_back_up: !st.is_native()
                        && st.manifest.as_ref().is_some_and(|m| m.backs_up_nothing()),
                    is_native: st.is_native(),
                }
            })
            .collect(),
        homelab_core::ops::pool::READ_CONCURRENCY,
    );

    // Offsite: is the gdrive remote configured, and does a cheap listing work?
    let offsite_fut = async {
        progress("doctor: the offsite remote (one listing on Google Drive)…");
        let remotes = exec
            .run(&Cmd::new("rclone", &["listremotes"], 20))
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
        let offsite_configured = remotes.lines().any(|l| l.trim() == "gdrive:");
        let offsite_token_valid = offsite_configured
            && exec
                .run(&Cmd::new(
                    "rclone",
                    &[
                        "lsd",
                        "gdrive:homelab-backups",
                        "--max-depth",
                        "1",
                        "--contimeout",
                        "10s",
                    ],
                    30,
                ))
                .await
                .map(|o| o.success())
                .unwrap_or(false);
        (offsite_configured, offsite_token_valid)
    };

    // Mirror lag: commits not yet on the mirror remote.
    let repo = format!("{}/repo", state_dir);
    let mirror_fut = async {
        progress("doctor: mirror, disk and the daemon's own units…");
        match mirror_remote {
            None => None,
            Some(_) => exec
                .run(&Cmd::new(
                    "git",
                    &[
                        "-C",
                        &repo,
                        "rev-list",
                        "--count",
                        "--branches",
                        "--not",
                        "--remotes=mirror",
                    ],
                    30,
                ))
                .await
                .ok()
                .and_then(|o| o.stdout.trim().parse::<u32>().ok()),
        }
    };
    // Independent reads, overlapped (ops::pool); polled in the order they
    // used to run.
    let (managed_stacks, (offsite_configured, offsite_token_valid), mirror_behind) =
        futures_util::join!(stacks_fut, offsite_fut, mirror_fut);
    let interrupted = std::fs::read_to_string(format!("{}/journal.jsonl", state_dir))
        .map(|j| {
            homelab_core::incidents::interrupted_ops(&j)
                .into_iter()
                .map(|(op, _)| op)
                .collect()
        })
        .unwrap_or_default();
    // Host disk free % via df on the state dir.
    let disk = exec
        .run(&Cmd::new("df", &["--output=pcent", state_dir], 20))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                .map(|used| 100u64.saturating_sub(used))
        });
    // fix-205: both directories are made on first use, so on a host that
    // has not used them yet `df` on the directory itself fails and the
    // doctor reported "unknown" every night. The free space that matters is
    // the filesystem the directory WILL be made on: `df` the nearest
    // existing ancestor.
    fn df_nearest(dir: &str) -> Cmd {
        let quoted = dir.replace('\'', "'\\''");
        let script = format!(
            "d='{quoted}'; while [ ! -e \"$d\" ] && [ \"$d\" != / ]; do d=$(dirname \"$d\"); done; \
             df --output=pcent \"$d\""
        );
        Cmd::new("sh", &["-c", &script], 20)
    }
    // fix-113 ADDENDUM: free % on the chassis backup staging directory —
    // only asked when one is configured; `None` outer = not configured, so
    // `diagnose` raises no finding over a feature nobody turned on.
    let staging_disk_free_pct = match staging_dir {
        Some(dir) => Some(exec.run(&df_nearest(dir)).await.ok().and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                .map(|used| 100u64.saturating_sub(used))
        })),
        None => None,
    };
    // fix-62: free % on the restore drill's scratch directory — always
    // asked, since a default applies when host.toml names none.
    let restore_scratch_disk_free_pct = exec
        .run(&df_nearest(restore_scratch_dir))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                .map(|used| 100u64.saturating_sub(used))
        });
    // The daemon's own units, held against the copies this binary carries.
    let mut units = Vec::new();
    for u in homelab_core::hostunits::UNITS {
        units.push((u.path, exec.read_file(u.path).await.ok()));
    }
    let host_units_drift = Some(homelab_core::hostunits::drift(&units));
    Probes {
        host_units_drift,
        host_disk_free_pct: disk,
        staging_disk_free_pct,
        restore_scratch_disk_free_pct,
        state_parses,
        managed_stacks,
        offsite_configured,
        offsite_token_valid,
        mirror_behind,
        interrupted_ops: interrupted,
        // fix-120: filled in by the caller, which holds the counter.
        failed_auth: None,
        exposure: None,
        loose_files: None,
        privileged: None,
        host_meta: None,
        restore_drill: None,
        password_file_ok: None,
        drive: None,
        unowned_route_files: None,
    }
}

/// redesign-host-4 (3.71.0): the disk facts the Host page shows beside the
/// root volume — how full the thin pool is, what its volumes are promised,
/// and the root disk's kind — gathered with the fix-222 reading.
#[cfg(test)]
mod redesign_host_4_tests {
    use super::*;
    use homelab_core::executor::{CmdOutput, MockExecutor};

    const LVS_JSON: &str = r#"{"report":[{"lv":[
        {"lv_name":"data","lv_size":"794.30","pool_lv":"","data_percent":"41.20","metadata_percent":"2.53"},
        {"lv_name":"root","lv_size":"96.00","pool_lv":"","data_percent":"","metadata_percent":""},
        {"lv_name":"vm-104-disk-0","lv_size":"8.00","pool_lv":"data","data_percent":"55.10","metadata_percent":""},
        {"lv_name":"vm-113-disk-0","lv_size":"500.00","pool_lv":"data","data_percent":"60.00","metadata_percent":""}
    ]}]}"#;

    #[tokio::test]
    async fn redesign_host_4_the_disk_reading_carries_the_pools_use_and_the_disks_kind() {
        let exec = MockExecutor::new();
        exec.respond_always("lv_size pve/root", CmdOutput::ok("  96.00g\n"));
        exec.respond_always("lv_size pve/data", CmdOutput::ok("  794.30g\n"));
        exec.respond_always("--reportformat json", CmdOutput::ok(LVS_JSON));
        exec.respond_always(
            "pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/sda3\n"),
        );
        exec.respond_always("--getsize64", CmdOutput::ok("1000204886016\n"));
        exec.respond_always("ROTA,TRAN /dev/sda", CmdOutput::ok("   0 sata\n"));
        exec.respond_always("--max-depth=2", CmdOutput::ok("1024\t/var\n"));
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.root_disk_device, "/dev/sda");
        assert_eq!(d.root_disk_kind.as_deref(), Some("SSD"));
        let pool = d.thin_pool.expect("the pool's use is read");
        assert_eq!(pool.data_pct, 41.2);
        assert_eq!(pool.metadata_pct, 2.53);
        assert_eq!(pool.promised_gb, 508.0);
        assert_eq!(pool.volumes, 2);
        assert!(exec.unused_rules().is_empty(), "{:?}", exec.unused_rules());
    }

    #[tokio::test]
    async fn redesign_host_4_an_unreadable_pool_or_disk_is_absent_never_invented() {
        let exec = MockExecutor::new();
        exec.respond_always("--reportformat json", CmdOutput::failed(5, "no such vg"));
        exec.respond_always(
            "pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/nvme0n1p3\n"),
        );
        exec.respond_always("--getsize64", CmdOutput::ok("1000204886016\n"));
        exec.respond_always("ROTA,TRAN", CmdOutput::failed(32, "not a block device"));
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.thin_pool, None);
        assert_eq!(d.root_disk_kind, None);
    }
}

/// fix-222 round 2 (live, host 3.70.7, 2026-10-03): `status --json` carried
/// the root volume and the thin pool but `root_disk_device` was empty and
/// `root_disk_total_gb` 0. `pvs` takes physical volumes (or tags) as its
/// positional arguments, never a volume group: `pvs ... pve` answers
/// `Failed to find physical volume "pve".` with exit 5 on a real host, so
/// the whole physical-disk half was skipped. The earlier tests matched the
/// mock on the substring `pv_name pve` and so accepted the wrong call. The
/// outputs below have the shape lvm2, util-linux and coreutils print on a
/// Proxmox VE host installed on one NVMe disk.
#[cfg(test)]
mod fix_222_real_host_tests {
    use super::*;
    use homelab_core::executor::{CmdOutput, MockExecutor};

    #[tokio::test]
    async fn fix_222_the_physical_disk_is_found_with_the_call_lvm_really_answers() {
        let exec = MockExecutor::new();
        exec.respond_always("lv_size pve/root", CmdOutput::ok("  96.00g\n"));
        exec.respond_always("lv_size pve/data", CmdOutput::ok("  794.30g\n"));
        // What lvm2 answers when handed a volume group where a PV belongs.
        exec.respond_always(
            "pvs --noheadings -o pv_name pve",
            CmdOutput::failed(5, "  Failed to find physical volume \"pve\".\n"),
        );
        // The volume group selected the way pvs accepts it.
        exec.respond_always(
            "pvs --noheadings -o pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/nvme0n1p3\n"),
        );
        exec.respond_always(
            "blockdev --getsize64 /dev/nvme0n1",
            CmdOutput::ok("1000204886016\n"),
        );
        exec.respond_always(
            "lsblk -d -n -o ROTA,TRAN /dev/nvme0n1",
            CmdOutput::ok("   0 nvme\n"),
        );
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.root_disk_device, "/dev/nvme0n1");
        assert!(
            (d.root_disk_total_gb - 931.5).abs() < 0.1,
            "{}",
            d.root_disk_total_gb
        );
        assert_eq!(d.root_disk_kind.as_deref(), Some("NVMe SSD"));
    }
}
