//! Restore drills and the device backup record, plus the check that finds watchers nobody declared.

use crate::*;

/// G14: restore one repository into a scratch directory and say what came
/// back. The judgement lives in core (`restoredrill::verdict`); this is the
/// shell that fetches the numbers and always cleans up after itself.
pub(crate) async fn run_restore_drill(
    exec: &RealExecutor,
    cfg: &homelab_core::ops::backup::BackupCfg,
    repo: &str,
    target: &str,
    pg: Option<&homelab_core::ops::restoredrill::PostgresCheck>,
) -> homelab_core::ops::restoredrill::Outcome {
    use homelab_core::ops::restoredrill::Outcome;
    // The restore and the generic checks live in core, shared with the
    // destroy's restore-check (`restoredrill::restore_and_judge`).
    let outcome = homelab_core::ops::restoredrill::restore_and_judge(exec, cfg, repo, target).await;
    // fix-62: a stack that declares a Postgres dump (`postgres_check_image`
    // on the mount) gets a throwaway Postgres restore check — only when the
    // ordinary checks above already passed, because a throwaway container
    // started against files that are not even readable proves nothing new.
    let outcome = match (pg, &outcome) {
        (Some(pg), Outcome::Passed { .. }) => {
            let ready = run_postgres_drill_check(exec, pg, target).await;
            homelab_core::ops::restoredrill::with_postgres_check(outcome, ready)
        }
        _ => outcome,
    };
    // Always: a drill that leaves a full restore behind fills the disk the
    // backups need.
    let _ = exec.run(&Cmd::new("rm", &["-rf", target], 300)).await;
    outcome
}

/// fix-62: start a throwaway Postgres container, inside the owning stack's
/// own container (it already has docker), against the data the drill just
/// restored, and read whether it reaches "ready to accept connections" —
/// `restic` returning files without error says nothing about whether
/// Postgres can still open them. `None`: the check itself could not run
/// (docker missing, the copy into the container failed) — a gap in what
/// this drill can prove, not a failed drill; `Some(false)` is a real
/// failure.
pub(crate) async fn run_postgres_drill_check(
    exec: &RealExecutor,
    pg: &homelab_core::ops::restoredrill::PostgresCheck,
    target: &str,
) -> Option<bool> {
    use homelab_core::executor::shq;
    const SCRATCH: &str = "/tmp/homelab-pg-drill";
    let src = format!("{}{}", target, pg.host_path);
    // The host side reads the data the drill already restored (no second
    // restic restore) and hands it to the container over tar, the same
    // direction `restore_native`'s safety copy uses the other way.
    let copy = format!(
        "tar -cf - -C {src} . | pct exec {vmid} -- sh -c 'rm -rf {scratch} && mkdir -p \
         {scratch} && tar -xf - -C {scratch}'",
        src = shq(&src),
        vmid = pg.vmid,
        scratch = SCRATCH,
    );
    match exec.run(&Cmd::new("sh", &["-c", &copy], 300)).await {
        Ok(o) if o.success() => {}
        _ => return None,
    }
    // 0: ready. 1: started but never became ready — a real failure. 2 (or
    // anything else): docker itself could not run the image at all.
    let check = format!(
        "docker rm -f homelab-pg-drill >/dev/null 2>&1; \
         cid=$(docker run -d --name homelab-pg-drill -v {scratch}:/var/lib/postgresql/data \
         {image}) || exit 2; \
         ok=0; \
         for i in $(seq 1 30); do \
         docker logs \"$cid\" 2>&1 | grep -q 'database system is ready to accept connections' \
         && ok=1 && break; sleep 1; done; \
         docker rm -f \"$cid\" >/dev/null 2>&1; rm -rf {scratch}; [ \"$ok\" = 1 ]",
        scratch = SCRATCH,
        image = shq(&pg.image),
    );
    match homelab_core::executor::pct_sh(exec, pg.vmid, &check, 60).await {
        Ok(o) if o.success() => Some(true),
        Ok(o) if o.code == 1 => Some(false),
        _ => None,
    }
}

/// H12: bearer check, extracted for testing.
/// F259: a watcher pointed at a path no writer writes to watches nothing
/// (see REGISTER.md). `watched_backups` checks that files keep arriving
/// somewhere; `device_backups` is what puts them there, at
/// `<restic_base>/<name>-config`.
///
/// Returns the watchers whose path matches no configured writer. Deliberately
/// a warning rather than a refusal: a watcher may legitimately point at
/// something written by a machine this suite does not manage, and refusing to
/// start over that would be worse than saying so.
pub(crate) fn orphan_watchers(
    watched: &[WatchedBackup],
    devices: &[homelab_core::ops::devicebackup::DeviceBackup],
    restic_base: &str,
) -> Vec<String> {
    // Two shapes have to line up before this can compare anything, and both
    // were got wrong on the first live configuration:
    //
    //  * `restic_base` carries an `rclone:` prefix (`rclone:gdrive:...`)
    //    because that is how restic names a remote; an rclone path does not.
    //  * a watcher usefully points at `<repo>/snapshots` rather than at the
    //    repo root, because the root holds `config`, written once at init and
    //    never again — so watching the root reports the creation date forever
    //    and goes stale while the backups are running fine.
    //
    // So: normalise the prefix, and accept the repo path or anything under
    // it. A guard that cries wolf on a correct configuration is worse than no
    // guard, because it teaches its reader to skip the line.
    // Both live in `ops::watched::feeds`, which the recorded watched-backup
    // state uses too.
    watched
        .iter()
        .filter(|w| {
            !devices
                .iter()
                .any(|d| homelab_core::ops::watched::feeds(&w.rclone_path, restic_base, &d.name))
        })
        .map(|w| format!("{} → {}", w.name, w.rclone_path))
        .collect()
}

/// Decision "Fleet check speed": a device backup this suite just made is
/// recorded for every watcher on its repository, so the fleet check knows
/// without listing Google Drive.
pub(crate) async fn record_device_backup(state: &AppState, device: &str) {
    let watchers: Vec<(String, String)> = state
        .config
        .watched_backups
        .iter()
        .map(|w| (w.name.clone(), w.rclone_path.clone()))
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let fed = homelab_core::ops::watched::record_device_backup(
        &RealExecutor,
        &state.config.state_dir,
        &watchers,
        &state.config.backup.restic_base,
        device,
        now,
    )
    .await;
    if !fed.is_empty() {
        info!(
            "device backup {} recorded for watcher(s) {}",
            device,
            fed.join(", ")
        );
    }
}
