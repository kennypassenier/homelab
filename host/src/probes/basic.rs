//! The basic doctor probes: every managed container's backup age and sealed
//! secrets, the offsite remote, mirror lag, disk space and the host units.
//! I/O stays here; the verdict logic is in core.

use crate::*;
use homelab_core::doctor::{Probes, StackProbe};

type SnapshotCache = Arc<homelab_core::ops::snapshot_cache::SnapshotCache>;

/// Free percent of the filesystem `dir` is on, from `df` on the nearest
/// existing ancestor (fix-205: both the staging and scratch directories are
/// made on first use, so `df` on the directory itself fails on a host that
/// has not used them yet and the doctor reported "unknown" every night).
async fn free_pct(exec: &dyn Executor, dir: &str) -> Option<u64> {
    let quoted = dir.replace('\'', "'\\''");
    let script = format!(
        "d='{quoted}'; while [ ! -e \"$d\" ] && [ \"$d\" != / ]; do d=$(dirname \"$d\"); done; \
         df --output=pcent \"$d\""
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 20)).await.ok()?;
    used_to_free(&out.stdout)
}

/// Free percent of the state directory's own filesystem; the directory exists
/// by the time the daemon runs, so `df` asks it directly.
async fn state_dir_free_pct(exec: &dyn Executor, state_dir: &str) -> Option<u64> {
    let out = exec
        .run(&Cmd::new("df", &["--output=pcent", state_dir], 20))
        .await
        .ok()?;
    used_to_free(&out.stdout)
}

/// `df --output=pcent` prints a header and then `NN%`; this is 100 minus NN.
fn used_to_free(df_stdout: &str) -> Option<u64> {
    df_stdout
        .lines()
        .nth(1)
        .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
        .map(|used| 100u64.saturating_sub(used))
}

/// Per-stack backup freshness and container presence from state.json.
/// ops::pool: the stacks are asked a few at a time, in state order.
async fn stack_probes(
    exec: &dyn Executor,
    state_dir: &str,
    stacks: &[(String, homelab_core::state::StackState)],
    now_unix: u64,
    snapshot_cache: &SnapshotCache,
    backup_cfg: &homelab_core::ops::backup::BackupCfg,
) -> Vec<StackProbe> {
    homelab_core::ops::pool::bounded(
        stacks
            .iter()
            .map(|(name, st)| async move {
                // Present = Proxmox has a configuration for it, read off
                // pmxcfs without pct's 0.45 s of Perl start-up (2026-09-29).
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
    )
    .await
}

/// Is the gdrive remote configured, and does a cheap listing work?
async fn offsite(exec: &dyn Executor) -> (bool, bool) {
    let remotes = exec
        .run(&Cmd::new("rclone", &["listremotes"], 20))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let configured = remotes.lines().any(|l| l.trim() == "gdrive:");
    let token_valid = configured
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
    (configured, token_valid)
}

/// Mirror lag: commits not yet on the mirror remote.
async fn mirror_behind(
    exec: &dyn Executor,
    state_dir: &str,
    mirror_remote: Option<&str>,
) -> Option<u32> {
    mirror_remote?;
    let repo = format!("{state_dir}/repo");
    exec.run(&Cmd::new(
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
    .and_then(|o| o.stdout.trim().parse::<u32>().ok())
}

/// Operations the previous run left half done.
fn interrupted_ops(state_dir: &str) -> Vec<String> {
    std::fs::read_to_string(format!("{state_dir}/journal.jsonl"))
        .map(|j| {
            homelab_core::incidents::interrupted_ops(&j)
                .into_iter()
                .map(|(op, _)| op)
                .collect()
        })
        .unwrap_or_default()
}

/// The daemon's own units, held against the copies this binary carries.
async fn units_drift(exec: &dyn Executor) -> Vec<String> {
    let mut units = Vec::new();
    for u in homelab_core::hostunits::UNITS {
        units.push((u.path, exec.read_file(u.path).await.ok()));
    }
    homelab_core::hostunits::drift(&units)
}

/// Gather doctor probes (F6): backup freshness per stack from state.json,
/// offsite reachability via a quick rclone listing, mirror lag via the
/// unpushed-commit count. Generic over the executor so the healthy/broken
/// matrix is testable with MockExecutor. `progress` gets one line per phase
/// (fix-104: `homelab doctor` took 24 s with nothing on the screen).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn gather_probes(
    exec: &dyn Executor,
    state_dir: &str,
    mirror_remote: Option<&str>,
    staging_dir: Option<&str>,
    restore_scratch_dir: &str,
    now_unix: u64,
    // fix-218: the same `SnapshotCache` `homelab snapshots` and the dashboard
    // read. A cold entry is kicked for a background refresh (never blocks
    // doctor) via the `Arc` so the spawned task can outlive this call.
    snapshot_cache: &SnapshotCache,
    backup_cfg: &homelab_core::ops::backup::BackupCfg,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Probes {
    progress("doctor: every managed container, its backup age and its sealed secrets…");
    let state_raw = exec.read_file(&format!("{state_dir}/state.json")).await;
    let state_parses = state_raw
        .as_ref()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
        .unwrap_or(true);
    let stacks: Vec<(String, homelab_core::state::StackState)> = state_raw
        .as_ref()
        .ok()
        .and_then(|raw| serde_json::from_str::<homelab_core::state::HostState>(raw).ok())
        .map(|hs| hs.stacks.into_iter().collect())
        .unwrap_or_default();

    // Independent reads, overlapped (ops::pool).
    let offsite_fut = async {
        progress("doctor: the offsite remote (one listing on Google Drive)…");
        offsite(exec).await
    };
    let mirror_fut = async {
        progress("doctor: mirror, disk and the daemon's own units…");
        mirror_behind(exec, state_dir, mirror_remote).await
    };
    let (managed_stacks, (offsite_configured, offsite_token_valid), mirror_behind) = futures_util::join!(
        stack_probes(
            exec,
            state_dir,
            &stacks,
            now_unix,
            snapshot_cache,
            backup_cfg
        ),
        offsite_fut,
        mirror_fut
    );

    // fix-113 ADDENDUM: free % on the chassis backup staging directory — only
    // asked when one is configured; `None` outer = not configured, so
    // `diagnose` raises no finding over a feature nobody turned on.
    let staging_disk_free_pct = match staging_dir {
        Some(dir) => Some(free_pct(exec, dir).await),
        None => None,
    };
    Probes {
        host_units_drift: Some(units_drift(exec).await),
        host_disk_free_pct: state_dir_free_pct(exec, state_dir).await,
        staging_disk_free_pct,
        // fix-62: always asked, since a default applies when host.toml names none.
        restore_scratch_disk_free_pct: free_pct(exec, restore_scratch_dir).await,
        state_parses,
        managed_stacks,
        offsite_configured,
        offsite_token_valid,
        mirror_behind,
        interrupted_ops: interrupted_ops(state_dir),
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
        tools: None,
    }
}
