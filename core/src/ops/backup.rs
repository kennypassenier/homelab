//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

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
        "restic".to_string(),
    ];
    full.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    Cmd::new(refs[0], &refs[1..], timeout)
}

pub struct BackupCfg {
    pub restic_base: String,
    /// Path to the restic password file on the host (from the secret store).
    pub password_file: String,
    /// Tiered retention (G8) — computed by us, not restic's --keep-* flags.
    pub tiers: Vec<crate::retention::RetentionTier>,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab-backups".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            tiers: crate::retention::default_tiers(),
        }
    }
}

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
pub async fn backup(ctx: &OpCtx<'_>, m: &StackManifest, cfg: &BackupCfg) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let paths: Vec<String> = m.storage.iter().map(|s| s.host_path.clone()).collect();

    // A1/A2: same gate as every mutating op (quiesce/resume reach into the
    // container).
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "init repo", {
        // Idempotent: init fails harmlessly if the repo already exists.
        let _ = exec
            .run(&restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["init"],
                120,
            ))
            .await?;
        Ok(StepOutcome::Unchanged)
    });

    // Quiesce: stop containers labeled com.homelab.backup.pause=true.
    step!(runner, "quiesce", {
        let script = "for c in $(docker ps -q --filter label=com.homelab.backup.pause=true); do docker stop $c; done; true";
        let _ = super::util_pct_sh(exec, m.vmid, script, 120).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "snapshot", {
        if paths.is_empty() {
            // No /appdata paths — nothing to snapshot (surfaced as a no-change
            // step; the transcript makes it visible).
            return Ok(StepOutcome::Unchanged);
        }
        let mut args = vec!["backup"];
        for p in &paths {
            args.push(p.as_str());
        }
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &args,
                1800,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // Resume the paused containers.
    step!(runner, "resume", {
        let dir_cmds = m
            .apps
            .iter()
            .map(|a| format!("cd '/opt/{}/{}' && docker compose up -d", m.stack_name, a))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = super::util_pct_sh(exec, m.vmid, &format!("{}; true", dir_cmds), 300).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "retention", {
        // G8 tiered retention: list snapshots, compute the forget-set with
        // our own engine, forget by explicit id.
        let out = run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["snapshots", "--json"],
                300,
            ),
        )
        .await?;
        let snapshots = parse_snapshots_json(&out.stdout);
        let doomed = crate::retention::forget_list(&snapshots, &cfg.tiers, ctx.now_unix);
        if doomed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &args,
                900,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
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
fn parse_snapshots_json(raw: &str) -> Vec<(String, u64)> {
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

    step!(runner, "validate snapshot", {
        let out = exec
            .run(&restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["snapshots", "--last"],
                120,
            ))
            .await?;
        if !out.success() {
            return Err(CoreError::Other("restic repo unreachable".into()));
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
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["restore", snapshot, "--target", "/"],
                1800,
            ),
        )
        .await?;
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
