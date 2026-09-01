//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::{Level, PipelineEvent};

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// Where restic keeps its index cache. Without it every single operation
/// re-downloads the repository index from Google Drive first.
///
/// It was not missing by choice: restic derives the path from `$XDG_CACHE_HOME`
/// or `$HOME`, and a systemd service has neither, so every backup in this
/// fleet has run with `unable to open cache: neither $XDG_CACHE_HOME nor
/// $HOME are defined` in its output — a line that reads as noise and costs a
/// full index fetch per repository, of which the gateway alone has six.
pub const RESTIC_CACHE_DIR: &str = "/var/lib/homelab/restic-cache";

/// Build a Cmd that runs restic with the repo env inline (via `env`).
fn restic(base: &str, stack: &str, password_ref: &str, args: &[&str], timeout: u64) -> Cmd {
    // The host wraps this so RESTIC_PASSWORD comes from its secret store; here
    // we pass a reference the host resolves. In tests the MockExecutor just
    // records the argv. Path join uses "/" — everything lives under one
    // gdrive folder (homelab-backups), not loose dirs in the drive root.
    let repo = format!("{}/{}-config", base, stack);
    let mut full = vec![
        "env".to_string(),
        format!("RESTIC_REPOSITORY={}", repo),
        format!("RESTIC_PASSWORD_FILE={}", password_ref),
        format!("RESTIC_CACHE_DIR={}", RESTIC_CACHE_DIR),
        "restic".to_string(),
    ];
    full.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    Cmd::new(refs[0], &refs[1..], timeout)
}

#[derive(Clone)]
pub struct BackupCfg {
    pub restic_base: String,
    /// Path to the restic password file on the host (from the secret store).
    pub password_file: String,
    /// Tiered retention (G8) — computed by us, not restic's --keep-* flags.
    pub tiers: Vec<crate::retention::RetentionTier>,
    /// Snapshot timeout. Hardening H2: the old fixed 1800 s was too small
    /// for a first multi-GB upload over residential rclone/gdrive.
    pub snapshot_timeout_s: u64,
    /// Restore timeout. Was a hardcoded 1800 s while the backup side had
    /// already been raised to four hours for exactly the same reason — so a
    /// large restore over Google Drive died at thirty minutes, on the one
    /// operation you least want to find broken (deployment project, F38).
    pub restore_timeout_s: u64,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab-backups".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            tiers: crate::retention::default_tiers(),
            snapshot_timeout_s: 4 * 3600,
            restore_timeout_s: 4 * 3600,
        }
    }
}

/// Build a restic command from a BackupCfg (shared with deploy's E3
/// auto-restore step).
pub(crate) fn restic_cmd(cfg: &BackupCfg, stack: &str, args: &[&str], timeout: u64) -> Cmd {
    restic(&cfg.restic_base, stack, &cfg.password_file, args, timeout)
}

/// The newest snapshot across a stack's per-app repositories, or None when
/// nothing answered. The repository is the truth about when a stack was last
/// backed up; `StackState::last_backup` is only a cache of it, and a C4
/// replacement throws that cache away with the container it destroys.
///
/// Found by the M7 drill (2026-08-31): CT 115 was backed up twelve minutes
/// before it was replaced, came back reporting it had never been backed up,
/// and the fleet check dutifully called it broken while the snapshot sat in
/// the repository untouched.
pub(crate) async fn newest_snapshot_unix(
    exec: &dyn Executor,
    m: &StackManifest,
    cfg: &BackupCfg,
) -> Option<u64> {
    let mut newest: Option<u64> = None;
    for (owner, _paths) in owner_groups(m) {
        // A repository that does not exist yet is the normal case for a new
        // stack, so a failure here is silence, not an error.
        let Ok(out) = exec
            .run(&restic_cmd(
                cfg,
                &owner,
                &["snapshots", "--latest", "1", "--json"],
                120,
            ))
            .await
        else {
            continue;
        };
        if !out.success() {
            continue;
        }
        if let Some(t) = parse_snapshots_json(&out.stdout)
            .into_iter()
            .map(|(_, t)| t)
            .max()
        {
            newest = Some(newest.map_or(t, |n: u64| n.max(t)));
        }
    }
    newest
}

/// D25: group the manifest's storage paths by the app that owns them, in
/// manifest order. A path with no declared owner belongs to the stack, which
/// keeps host-level paths (and every manifest written before the field
/// existed) working exactly as they did.
/// Public because the disaster-recovery runbook must name the SAME
/// repositories the backup actually writes to. It used to derive them itself,
/// from the stack name, and so printed `media-config` for a stack whose
/// repositories are `jellyfin-config`, `sonarr-config`, `radarr-config` and
/// three more. That document is read exactly once — when everything else is
/// gone — and it would have said the backups were not there.
pub fn owner_groups(m: &StackManifest) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for mount in &m.storage {
        let owner = mount.owner(&m.stack_name).to_string();
        match groups.iter_mut().find(|(o, _)| *o == owner) {
            Some((_, paths)) => paths.push(mount.host_path.clone()),
            None => groups.push((owner, vec![mount.host_path.clone()])),
        }
    }
    groups
}

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
pub async fn backup(ctx: &OpCtx<'_>, m: &StackManifest, cfg: &BackupCfg) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    // D25: one repository per owning app, so an app that moves to another
    // stack keeps its history. Order is the manifest's, so the log reads the
    // way the file does.
    let groups = owner_groups(m);

    // A1/A2: same gate as every mutating op (quiesce/resume reach into the
    // container).
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "init repos", {
        // Idempotent: init fails harmlessly if the repo already exists.
        for (owner, _) in &groups {
            let _ = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["init"],
                    120,
                ))
                .await?;
        }
        Ok(StepOutcome::Unchanged)
    });

    // H2 hardening: a previous run killed mid-snapshot can leave a stale
    // repo lock; restic unlock only removes locks from dead processes, so
    // this is always safe. Best-effort (repo may not exist yet).
    step!(runner, "clear stale locks", {
        for (owner, _) in &groups {
            let _ = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["unlock"],
                    120,
                ))
                .await;
        }
        Ok(StepOutcome::Unchanged)
    });

    // Quiesce: stop containers labeled com.homelab.backup.pause=true, and
    // REMEMBER WHICH ONES.
    //
    // This used to stop by label and resume by the manifest's `apps` list,
    // and the two are not the same set. On 2026-08-31 the metrics stack's
    // nightly backup stopped prometheus and alertmanager — both labelled —
    // and resumed prometheus, promtail and pve-exporter, because host state
    // still held the app list from before alertmanager was added. The
    // snapshot then failed on a stale path, so nothing else touched the
    // stack, and Alertmanager stayed down for six hours. Nothing reported
    // it; Kenny saw it in Uptime Kuma, which had been watching it for two
    // hours by then.
    //
    // A backup that can leave a service off is worse than a backup that
    // fails, so what is paused is now what is resumed, by name.
    let paused: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let paused_w = paused.clone();
    step!(runner, "quiesce", {
        let script = "docker ps --filter label=com.homelab.backup.pause=true --format '{{.Names}}'";
        let out = super::util_pct_sh(exec, m.vmid, script, 60).await?;
        let names: Vec<String> = out
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if names.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let stop = format!("docker stop {}; true", names.join(" "));
        let _ = super::util_pct_sh(exec, m.vmid, &stop, 120).await?;
        if let Ok(mut g) = paused_w.lock() {
            *g = names;
        }
        Ok(StepOutcome::Changed)
    });

    // H2 hardening: the snapshot may fail, but RESUME MUST ALWAYS RUN — a
    // fail-closed abort here would leave the quiesced databases down until
    // a human noticed. So the snapshot error is captured, resume runs
    // unconditionally, and only then does the operation fail.
    let snapshot_result = runner
        .step("snapshot", || async {
            if groups.is_empty() {
                return Ok(StepOutcome::Unchanged);
            }
            for (owner, paths) in &groups {
                // --quiet as well as --json: without it restic emits a status line per
                // update and the operation log becomes a wall of progress json.
                // Quiet keeps the summary, which is the only line this needs.
                let mut args = vec!["backup", "--quiet", "--json"];
                for p in paths {
                    args.push(p.as_str());
                }
                let out = run_ok(
                    exec,
                    &restic(
                        &cfg.restic_base,
                        owner,
                        &cfg.password_file,
                        &args,
                        cfg.snapshot_timeout_s,
                    ),
                )
                .await?;
                // A restic run over a directory that exists and is empty
                // succeeds, writes a snapshot containing nothing, and reports
                // success. The record then says the stack is backed up and
                // the restore has nothing to give back — the same shape as
                // every other finding here: a green result that proves the
                // wrong thing.
                //
                // A path that does not exist already fails loudly (rc=1), and
                // that is how the metrics stack's stale path was caught on
                // 2026-08-31. An empty one is the case nothing catches.
                if snapshot_is_empty(&out.stdout) {
                    return Err(CoreError::Command {
                        rendered: format!("restic backup {}", owner),
                        detail: format!(
                            "the snapshot for '{}' contains no files :: it covered {} — check the path holds what you think it does, because a restore from this gives back nothing",
                            owner,
                            paths.join(", ")
                        ),
                    });
                }
            }
            Ok(StepOutcome::Changed)
        })
        .await;

    // Resume the paused containers — unconditionally.
    let paused_r = paused.clone();
    step!(runner, "resume", {
        // Exactly what quiesce stopped, by name — this is the half that must
        // not depend on any list that can go stale.
        let names = paused_r.lock().map(|g| g.clone()).unwrap_or_default();
        if !names.is_empty() {
            let start = format!("docker start {}; true", names.join(" "));
            let _ = super::util_pct_sh(exec, m.vmid, &start, 300).await?;
        }
        // Then the declared apps, which also brings back anything that was
        // down for an unrelated reason. Belt and braces: this is the step
        // that runs even when the snapshot failed.
        let dir_cmds = m
            .apps
            .iter()
            .map(|a| format!("cd '/opt/{}/{}' && docker compose up -d", m.stack_name, a))
            .collect::<Vec<_>>()
            .join("; ");
        if !dir_cmds.is_empty() {
            let _ = super::util_pct_sh(exec, m.vmid, &format!("{}; true", dir_cmds), 300).await?;
        }
        Ok(StepOutcome::Changed)
    });

    if let Err(e) = snapshot_result {
        return runner.finish_err("snapshot", &e);
    }

    step!(runner, "retention", {
        // G8 tiered retention: list snapshots, compute the forget-set with
        // our own engine, forget by explicit id. Per repository, since D25
        // gave every app its own.
        //
        // W2: the stack file's own policy wins over the fleet-wide one when
        // it states one. Resolved here rather than where the config is built,
        // so every caller — a manual backup, the nightly run, a future one —
        // gets it without being told to.
        let tiers = m.retention.as_ref().unwrap_or(&cfg.tiers);
        if m.retention.is_some() {
            ctx.sink.emit(PipelineEvent::Line {
                level: Level::Info,
                source: "HOST".into(),
                msg: format!(
                    "[w2] {} keeps snapshots by its own policy ({} tier(s)), not the fleet-wide one",
                    m.stack_name,
                    tiers.len()
                ),
            });
        }
        let mut changed = false;
        for (owner, _) in &groups {
            let out = run_ok(
                exec,
                &restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["snapshots", "--json"],
                    300,
                ),
            )
            .await?;
            let snapshots = parse_snapshots_json(&out.stdout);
            let doomed = crate::retention::forget_list(&snapshots, tiers, ctx.now_unix);
            if doomed.is_empty() {
                continue;
            }
            let mut args: Vec<&str> = vec!["forget"];
            args.extend(doomed.iter().map(|s| s.as_str()));
            args.push("--prune");
            run_ok(
                exec,
                &restic(&cfg.restic_base, owner, &cfg.password_file, &args, 900),
            )
            .await?;
            changed = true;
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    runner.log(
        Level::Info,
        format!("[backup] {} snapshot complete", m.stack_name),
    );
    runner.finish_ok()
}

/// Parse `restic snapshots --json` into `(short_id, unix_time)` pairs.
/// Tolerant of extra fields; returns empty on malformed input (retention
/// then keeps everything — fail-safe direction).
pub(crate) fn parse_snapshots_json(raw: &str) -> Vec<(String, u64)> {
    #[derive(serde::Deserialize)]
    struct Snap {
        short_id: String,
        time: String,
    }
    let Ok(snaps) = serde_json::from_str::<Vec<Snap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps
        .into_iter()
        .filter_map(|s| {
            // RFC3339 → unix without pulling in chrono: date parsing via the
            // subset restic emits (e.g. 2026-08-11T04:00:12.123+02:00).
            humantime_to_unix(&s.time).map(|t| (s.short_id, t))
        })
        .collect()
}

/// Minimal RFC3339 → unix seconds (UTC), no external crates. Handles the
/// forms restic emits; returns None on anything unexpected.
fn humantime_to_unix(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Timezone offset: trailing Z or ±HH:MM after the (optional) fraction.
    let rest = &s[19..];
    let offset_secs: i64 = if rest.ends_with('Z') || rest.is_empty() {
        0
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let sign = if rest.as_bytes()[pos] == b'+' { 1 } else { -1 };
        let tz = &rest[pos + 1..];
        let th = tz.get(0..2)?.parse::<i64>().ok()?;
        let tm = tz.get(3..5)?.parse::<i64>().ok()?;
        sign * (th * 3600 + tm * 60)
    } else {
        0
    };
    // Days since epoch (civil-from-days algorithm, Howard Hinnant).
    let (y, mo) = if mo <= 2 { (y - 1, mo + 12) } else { (y, mo) };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (mo - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let unix = days * 86_400 + h * 3600 + mi * 60 + sec - offset_secs;
    u64::try_from(unix).ok()
}

/// E2: restore a stack's /appdata from a snapshot (default: latest).
/// validate → quiesce → restore → resume → verify.
pub async fn restore(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> OperationReport {
    let op = format!("restore-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    runner.log(
        Level::Warn,
        format!("[restore] {} from snapshot '{}'", m.stack_name, snapshot),
    );

    // A1/A2: restore composes down and writes over the target — full gate.
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // D25: a stack's data lives in one repository per owning app, so a
    // restore walks all of them. Order is the manifest's.
    let groups = owner_groups(m);

    step!(runner, "validate snapshot", {
        for (owner, _) in &groups {
            let out = exec
                .run(&restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["snapshots", "--last"],
                    120,
                ))
                .await?;
            if !out.success() {
                return Err(CoreError::Other(format!(
                    "restic repo for '{}' unreachable",
                    owner
                )));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    // Stop the whole stack for a consistent restore.
    step!(runner, "quiesce stack", {
        for a in &m.apps {
            let _ = super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd '/opt/{}/{}' && docker compose down", m.stack_name, a),
                120,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "restore data", {
        for (owner, _) in &groups {
            run_ok(
                exec,
                &restic(
                    &cfg.restic_base,
                    owner,
                    &cfg.password_file,
                    &["restore", snapshot, "--target", "/"],
                    cfg.restore_timeout_s,
                ),
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "resume stack", {
        for a in &m.apps {
            super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd '/opt/{}/{}' && docker compose up -d", m.stack_name, a),
                300,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "verify health", {
        for a in &m.apps {
            let out = super::util_pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd '/opt/{}/{}' && docker compose ps --status running --services",
                    m.stack_name, a
                ),
                60,
            )
            .await?;
            if out.stdout.trim().is_empty() {
                return Err(CoreError::Other(format!("{} not running after restore", a)));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    runner.log(
        Level::Info,
        format!("[restore] {} restored and verified", m.stack_name),
    );
    runner.finish_ok()
}

/// H10 hardening: snapshot the host's own critical metadata — the secrets
/// vault, state.json, and TLS material — into a dedicated `host-meta` repo.
/// Without this, losing the host disk loses the keys needed for recovery.
pub async fn backup_host_meta(ctx: &OpCtx<'_>, cfg: &BackupCfg) -> OperationReport {
    let mut runner = Runner::new("host-meta-backup", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let secrets = format!("{}/secrets", ctx.state_dir);
    let state_file = format!("{}/state.json", ctx.state_dir);
    let tls_cert = format!("{}/tls-cert.pem", ctx.state_dir);
    let tls_key = format!("{}/tls-key.pem", ctx.state_dir);
    // The intent repo carries every applied compose file plus its git
    // history — cheap to include, and it turns "restore the host" into
    // "restore the host AND know what ran on it".
    let repo = format!("{}/repo", ctx.state_dir);

    step!(runner, "init repo", {
        let _ = exec
            .run(&restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &["init"],
                120,
            ))
            .await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "snapshot", {
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                "host-meta",
                &cfg.password_file,
                &["backup", &secrets, &state_file, &tls_cert, &tls_key, &repo],
                600,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        "[host-meta] vault/state/tls snapshot complete".to_string(),
    );
    runner.finish_ok()
}

/// Did the run that produced this output actually store anything?
///
/// restic's `--json` stream ends with a `summary` message carrying the
/// counts. Absence of a summary is NOT treated as empty: a version that
/// changes its output should not turn every backup into a failure — a check
/// that fires on something it merely does not recognise is worse than no
/// check, because it teaches people to ignore it.
pub fn snapshot_is_empty(stdout: &str) -> bool {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') || !line.contains("\"message_type\":\"summary\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let files = v
            .get("total_files_processed")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        let bytes = v
            .get("total_bytes_processed")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        return files == 0 && bytes == 0;
    }
    false
}
