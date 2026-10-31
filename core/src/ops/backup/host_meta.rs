//! H10: the host's own critical metadata, snapshotted into `host-meta`.

use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor, run_ok, shq};
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::{Level, PipelineEvent};

use super::config::BackupCfg;
use super::repo_init::{InitOutcome, init_repository};
use super::restic::restic;
use super::retention::retention_doomed;
use super::shell::sh;

/// Host files outside `state_dir` that the host-meta snapshot carries when
/// they exist. Public so the disaster-recovery runbook lists the same set.
pub const HOST_META_EXTRAS: &[&str] = &[
    "/usr/local/bin/smart-textfile-collector.py",
    "/etc/systemd/system/smart-collector.service",
    "/etc/systemd/system/smart-collector.timer",
    // fix-111 (host-meta-gaps, 2026-09-27): what a rebuilt host needs and no
    // backup held. The VLAN bridges every container sits on; the storage and
    // job definitions; the VM configurations, which carry Home Assistant's
    // Zigbee USB passthrough (VM 101) — read from pmxcfs on the host, the VMs
    // themselves are not touched; the swappiness drop-in; and rclone's
    // remote, without which restic cannot reach Google Drive at all. All
    // tiny, and all skipped when absent like the three above.
    "/etc/network/interfaces",
    "/etc/pve/storage.cfg",
    "/etc/pve/jobs.cfg",
    "/etc/pve/qemu-server",
    "/etc/sysctl.d/99-homelab-swappiness.conf",
    "/root/.config/rclone/rclone.conf",
];

/// H10 hardening: snapshot the host's own critical metadata — the secrets
/// vault, state.json, and TLS material — into a dedicated `host-meta` repo.
/// Without this, losing the host disk loses the keys needed for recovery.
/// fix-171 round 3: fixed and unconditional.
pub const HOST_META_BACKUP_STEPS: &[&str] = &["init repo", "host extras", "snapshot", "retention"];

pub async fn backup_host_meta(ctx: &OpCtx<'_>, cfg: &BackupCfg) -> OperationReport {
    let mut runner = Runner::new("host-meta-backup", ctx.sink, ctx.journal);
    runner.plan(HOST_META_BACKUP_STEPS);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    step!(runner, "init repo", init_repo(exec, cfg).await);
    let mut present: Vec<String> = Vec::new();
    step!(runner, "host extras", {
        present = present_extras(exec, ctx).await?;
        Ok(StepOutcome::Unchanged)
    });
    step!(runner, "snapshot", snapshot(exec, ctx, cfg, &present).await);
    step!(runner, "retention", retention(exec, ctx, cfg).await);
    runner.log(
        Level::Info,
        "[host-meta] vault/state/tls snapshot complete".to_string(),
    );
    runner.finish_ok()
}

/// gap-24: read the answer; only "already exists" is harmless.
async fn init_repo(exec: &dyn Executor, cfg: &BackupCfg) -> Result<StepOutcome, CoreError> {
    let init = restic(
        &cfg.restic_base,
        "host-meta",
        &cfg.password_file,
        &["init"],
        120,
    );
    Ok(match init_repository(exec, &init, "host-meta").await? {
        InitOutcome::Created => StepOutcome::Changed,
        InitOutcome::Existed => StepOutcome::Unchanged,
    })
}

/// Which of the extras this host actually has. Read once, so the snapshot
/// step gets a list that cannot fail on a missing path.
async fn present_extras(exec: &dyn Executor, ctx: &OpCtx<'_>) -> Result<Vec<String>, CoreError> {
    let mut present: Vec<String> = Vec::new();
    for p in HOST_META_EXTRAS {
        let out = exec
            .run(&sh(&format!("test -e {} && echo yes || true", shq(p)), 30))
            .await?;
        if out.stdout.trim() == "yes" {
            present.push(p.to_string());
        }
    }
    ctx.sink.emit(PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg: format!(
            "[host-meta] {} of {} host extra(s) present: {}",
            present.len(),
            HOST_META_EXTRAS.len(),
            if present.is_empty() {
                "—".to_string()
            } else {
                present.join(", ")
            }
        ),
    });
    Ok(present)
}

/// Everything under `state_dir` the snapshot carries, plus the daemon's
/// configuration (F180): it was NOT in this backup until 2026-09-02, found by
/// walking the disaster-recovery runbook against what is on Google Drive. It
/// holds the API token, the notify bearer, the OPNsense credential path and
/// the ZFS jobs, so a rebuilt host would have had its keys back and still
/// needed that file retyped from memory.
fn snapshot_sources(state_dir: &str, present: &[String]) -> Vec<String> {
    let mut sources = vec![
        format!("{state_dir}/secrets"),
        format!("{state_dir}/state.json"),
        format!("{state_dir}/tls-cert.pem"),
        format!("{state_dir}/tls-key.pem"),
        // The intent repo carries every applied compose file plus its git
        // history — cheap to include, and it turns "restore the host" into
        // "restore the host AND know what ran on it".
        format!("{state_dir}/repo"),
        "/etc/homelab/host.toml".to_string(),
    ];
    sources.extend(present.iter().cloned());
    sources
}

async fn snapshot(
    exec: &dyn Executor,
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
    present: &[String],
) -> Result<StepOutcome, CoreError> {
    let sources = snapshot_sources(&ctx.state_dir, present);
    let mut args: Vec<&str> = vec!["backup"];
    args.extend(sources.iter().map(String::as_str));
    let cmd = restic(
        &cfg.restic_base,
        "host-meta",
        &cfg.password_file,
        &args,
        600,
    );
    run_ok(exec, &cmd).await?;
    Ok(StepOutcome::Changed)
}

/// fix-111 (host-meta-gaps, 2026-09-27): this repository was never pruned, so
/// every rotated secret was kept for ever. The fleet-wide tiers apply, whose
/// last tier is unbounded: history stays, one snapshot per bucket.
async fn retention(
    exec: &dyn Executor,
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
) -> Result<StepOutcome, CoreError> {
    let pw = &cfg.password_file;
    let list = restic(
        &cfg.restic_base,
        "host-meta",
        pw,
        &["snapshots", "--json"],
        300,
    );
    let out = run_ok(exec, &list).await?;
    let doomed = retention_doomed(&out.stdout, &cfg.tiers, ctx.now_unix);
    if doomed.is_empty() {
        return Ok(StepOutcome::Unchanged);
    }
    let mut args: Vec<&str> = vec!["forget"];
    args.extend(doomed.iter().map(|s| s.as_str()));
    args.push("--prune");
    run_ok(exec, &restic(&cfg.restic_base, "host-meta", pw, &args, 900)).await?;
    Ok(StepOutcome::Changed)
}
