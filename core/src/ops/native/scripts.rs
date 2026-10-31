//! Shell snippets that run inside a native service's container to keep,
//! restore and health-check its binary. Pure string builders: the callers
//! decide when they run, these only say what runs.

use crate::ops::util::shq;

/// C7 update supervision — the safety net the app's own self-update cannot
/// be. The app updates itself (`update_cmd`, Kenny's route C+); around it
/// the homelab preserves the running binary, restarts into the new one only
/// when the binary actually changed, verifies health, and rolls back from
/// OUTSIDE the app when the new version does not come up.
/// F300: does the new version STAY up, rather than merely come up.
///
/// The check this replaces exited 0 on the first `is-active` that said
/// `active`, and took that reading immediately after `systemctl restart`
/// returned. For `Type=exec` the unit is active the moment the binary has
/// been exec'd, so the five iterations were very nearly dead code: it asked
/// "did it start", not "is it still running". A binary that binds, signals
/// ready and dies three seconds later passed it — and then, under the
/// `StartLimitIntervalSec=0` this project recommends for native units,
/// systemd restarts it forever while the update is recorded as healthy and
/// the armed rollback never fires. Found by the chassis-rs architecture
/// critic reading this file, not by it happening.
///
/// Two readings, because one of them can be fooled and the other cannot:
///
/// - **Still active**, sampled across a settle window. Necessary, not
///   sufficient — a service crash-looping on a 5-second timer is genuinely
///   `active` for part of every cycle, so sampling alone can land on the
///   good moments and see nothing wrong.
/// - **`NRestarts` unchanged**, which is a counter and not a sample. If
///   systemd restarted the unit even once during the window, the number
///   moved, and no amount of lucky timing hides it. The baseline is taken
///   AFTER the unit first reports active, so a manual restart's own effect
///   on the counter cannot matter.
///
/// The wait-for-active phase comes first and is generous (20 s): a
/// `Type=notify` service that takes a moment to signal readiness must not be
/// mistaken for one that failed.
pub fn health_script(unit: &str) -> String {
    format!(
        "systemctl restart {u} || exit 1; \
         i=0; while [ $i -lt 10 ]; do \
           [ \"$(systemctl is-active {u})\" = active ] && break; \
           sleep 2; i=$((i+1)); \
         done; \
         [ \"$(systemctl is-active {u})\" = active ] || {{ echo NEVER_ACTIVE; exit 1; }}; \
         n0=$(systemctl show {u} -p NRestarts --value); \
         i=0; while [ $i -lt 5 ]; do \
           sleep 2; \
           [ \"$(systemctl is-active {u})\" = active ] || {{ echo DIED_IN_WINDOW; exit 1; }}; \
           [ \"$(systemctl show {u} -p NRestarts --value)\" = \"$n0\" ] || \
             {{ echo RESTART_LOOP; exit 1; }}; \
           i=$((i+1)); \
         done; \
         exit 0",
        u = unit
    )
}

/// F300: stop the unit before writing over its binary.
///
/// The rollback used to `cp -p` straight onto the running program's path.
/// Writing to a file that is currently being executed gives ETXTBSY, and
/// `Restart=always` with a 5-second timer means the broken binary is being
/// executed again every five seconds — so the copy races the restarts, and a
/// lost race is reported as "ROLLBACK ALSO FAILED — service needs hands NOW"
/// when the rollback was simply never allowed to write.
///
/// `stop` is separated by `;` rather than `&&` on purpose: stopping a unit
/// that is already dead is not a failure, and must not abort the restore.
///
/// No settle window here, unlike the health check above. This binary was
/// running before the update, so the question is whether the restore took
/// effect — not whether an unproven version is stable. Doubling the worst
/// case for that would delay the loud failure report the operator needs.
pub fn rollback_script(unit: &str, prev: &str, binary: &str) -> String {
    format!(
        "systemctl stop {u}; cp -p {prev} {bin} && systemctl start {u} && sleep 2 && \
         [ \"$(systemctl is-active {u})\" = active ]",
        u = unit,
        prev = shq(prev),
        bin = shq(binary)
    )
}

/// fix-114 (native-rollback-copies-deleted, 2026-09-27): set the kept
/// previous binary aside, then copy the running one to its place. An update
/// that turns out not to change the binary puts the set-aside one back
/// (`keep_one_previous_script`), so an unchanged night never replaces N-1
/// with N. A failed copy puts it back at once. A set-aside copy that is
/// already there was left by a run that stopped half way, and is older than
/// the one in place: it is kept, not overwritten.
pub fn preserve_script(binary: &str, prev: &str) -> String {
    format!(
        "if [ -f {p} ] && [ ! -f {old} ]; then mv -f {p} {old}; fi; cp -p {b} {p} || \
         {{ if [ -f {old} ]; then mv -f {old} {p}; fi; exit 1; }}",
        p = shq(prev),
        old = shq(&format!("{}.old", prev)),
        b = shq(binary)
    )
}

/// fix-114: after a healthy run, exactly one previous binary stays beside
/// the program. When the binary changed, the copy taken before the run is
/// that previous one and the older one goes; when it did not change, the
/// previous binary from before the run comes back.
///
/// This reverses the deletion fix-10 added for a 2 GB rootfs: a release that
/// starts fine and misbehaves an hour later had no N-1 binary on disk, on the
/// notification path. CT 109 has 4 GB now, and the kit's own `.prev` (the
/// same version a second time) still goes.
pub fn keep_one_previous_script(binary: &str, prev: &str) -> String {
    format!(
        "if cmp -s {b} {p}; then if [ -f {old} ]; then mv -f {old} {p}; else rm -f {p}; fi; \
         else rm -f {old}; fi",
        b = shq(binary),
        p = shq(prev),
        old = shq(&format!("{}.old", prev))
    )
}

/// fix-114: after a rollback, the previous binary from before the failed run
/// is the kept one again.
pub(super) fn restore_set_aside_script(prev: &str) -> String {
    format!(
        "if [ -f {old} ]; then mv -f {old} {p}; fi",
        old = shq(&format!("{}.old", prev)),
        p = shq(prev)
    )
}

/// Remove the within-run rollback copy once the run has proven healthy.
///
/// `.homelab-prev` is read only by the function that writes it, as the way
/// back from an install or a supervised update that does not come up. Nothing
/// ever deleted it, so every deploy left a full copy of the program behind
/// forever — and the chassis kit keeps a `.prev` of its own beside it, so the
/// same version sat on disk twice. Measured on 2026-09-10: 220 MB of programs
/// on CT 109's 2.0 GB rootfs, 70 MB of it the duplicate, and the disk at 98%.
/// Deleting it after success costs nothing: past this point it can no longer
/// be used, because the next run makes its own.
pub fn drop_stale_rollback_script(prev: &str) -> String {
    format!("rm -f {}", shq(prev))
}
