//! Housekeeping that outlives an operation: pruning incident bundles, push staging, compacting the journal, and rendering findings.

use crate::*;

/// fix-103: one rendering for `homelab check`, the TUI and the nightly log —
/// a summary first, then the findings grouped by severity.
pub(crate) fn render_findings(findings: &[homelab_core::ops::fleetcheck::Finding]) -> String {
    homelab_core::ops::fleetcheck::render(findings)
}

/// T85: where a stack's binaries wait between `StageNativeBinary` and the
/// `DeployStack` that installs them. Under the state directory (root-only,
/// in no backup — a staged binary is re-fetchable and short-lived).
pub(crate) fn staged_binaries_dir(state_dir: &str, stack: &str) -> String {
    format!("{}/staged/{}", state_dir, stack)
}

/// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
/// remove the bundles older than `BUNDLE_MAX_AGE_DAYS` and those beyond the
/// newest `BUNDLE_MAX_COUNT`; returns how many went. Nothing pruned them.
pub(crate) fn prune_incidents(
    state_dir: &str,
    now: u64,
    max_age_days: u64,
    max_count: usize,
) -> usize {
    use homelab_core::incidents::bundles_to_prune;
    let dir = format!("{}/incidents", state_dir);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut removed = 0;
    for name in bundles_to_prune(&names, now, max_age_days, max_count) {
        match std::fs::remove_dir_all(format!("{}/{}", dir, name)) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("incidents: could not remove {} :: {}", name, e),
        }
    }
    if removed > 0 {
        info!(
            "incidents: removed {} bundle(s) older than {} days or beyond the newest {}",
            removed, max_age_days, max_count
        );
    }
    removed
}

/// rule-20: remove `push-staging-*` files under the state dir that
/// `util::stale_push_staging` judges orphaned — left behind by a push the
/// daemon never finished. Called at start (the previous run's leftovers)
/// and after every mutating operation (that operation's own, if it failed
/// partway). Best-effort and silent on an empty/missing directory: a state
/// dir with nothing staged is the common case, not a fault.
pub(crate) fn cleanup_push_staging(state_dir: &str, now: u64) -> usize {
    use homelab_core::ops::util::{STALE_PUSH_STAGING_MAX_AGE_S, stale_push_staging};
    let entries: Vec<(String, u64)> = std::fs::read_dir(state_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let mtime = e
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((name, mtime))
        })
        .collect();
    let mut removed = 0;
    for name in stale_push_staging(&entries, now, STALE_PUSH_STAGING_MAX_AGE_S) {
        match std::fs::remove_file(format!("{}/{}", state_dir, name)) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("push-staging cleanup: could not remove {} :: {}", name, e),
        }
    }
    if removed > 0 {
        info!(
            "push-staging cleanup: removed {} orphaned file(s) older than {}s",
            removed, STALE_PUSH_STAGING_MAX_AGE_S
        );
    }
    removed
}

/// fix-131: cut journal.jsonl back when it has outgrown its limit
/// (`incidents::compact_journal`), written whole through a temp file.
/// Called only while nothing writes the journal: at start, and under the
/// operation lock.
///
/// gap-26: the caller passes `config.journal_max_bytes` (see REGISTER.md).
pub(crate) fn compact_journal_file(state_dir: &str, max_bytes: u64) {
    let path = format!("{}/journal.jsonl", state_dir);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(cut) = homelab_core::incidents::compact_journal(&content, max_bytes as usize) else {
        return;
    };
    let tmp = format!("{}.compact.tmp", path);
    let written = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(cut.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &path)
    })();
    match written {
        Ok(()) => info!(
            "journal.jsonl: cut from {} to {} bytes, interrupted operations kept",
            content.len(),
            cut.len()
        ),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!("journal.jsonl: could not compact :: {}", e);
        }
    }
}

/// fix-131: one incident bundle as text: what failed, the versions, the
/// end of the transcript and where the rest is. `name` must be a bundle
/// name as `homelab incidents` lists it, checked before it becomes a path.
pub(crate) fn incident_text(state_dir: &str, name: &str) -> Result<String, String> {
    const TAIL: usize = 60;
    let plain = !name.is_empty()
        && name.split_once('-').is_some_and(|(ts, op)| {
            !op.is_empty() && !ts.is_empty() && ts.chars().all(|c| c.is_ascii_digit())
        })
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !plain {
        return Err(format!(
            "'{}' is not a bundle name :: `homelab incidents` lists them (<unix-time>-<operation>)",
            name
        ));
    }
    let dir = format!("{}/incidents/{}", state_dir, name);
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!(
            "no bundle named {} :: `homelab incidents` lists the ones kept (bundles older than {} \
             days are removed)",
            name,
            homelab_core::incidents::BUNDLE_MAX_AGE_DAYS
        ));
    }
    let read = |f: &str| std::fs::read_to_string(format!("{}/{}", dir, f)).unwrap_or_default();
    let mask = homelab_core::executor::mask_secrets;
    let mut out = format!("incident {}\n", name);
    if let Ok(report) = serde_json::from_str::<serde_json::Value>(&read("report.json")) {
        let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
        out.push_str(&format!("  operation: {}\n", s(&report["op"])));
        let err = &report["error"];
        if !err.is_null() {
            out.push_str(&format!("  what:   {}\n", mask(&s(&err["what"]))));
            out.push_str(&format!("  why:    {}\n", mask(&s(&err["why"]))));
            out.push_str(&format!("  remedy: {}\n", mask(&s(&err["remedy"]))));
        }
    }
    let versions = read("versions.txt");
    out.push_str(&format!(
        "  versions: {}\n",
        versions.split_whitespace().collect::<Vec<_>>().join(" ")
    ));
    let lines: Vec<String> = read("events.jsonl")
        .lines()
        .filter_map(|l| serde_json::from_str::<PipelineEvent>(l).ok())
        .filter_map(|e| match e {
            PipelineEvent::Line { msg, .. } => Some(mask(&msg)),
            _ => None,
        })
        .collect();
    let from = lines.len().saturating_sub(TAIL);
    out.push_str(&format!(
        "last {} of {} transcript line(s):\n",
        lines.len() - from,
        lines.len()
    ));
    for l in &lines[from..] {
        out.push_str(&format!("  {}\n", l));
    }
    out.push_str(&format!(
        "the whole bundle, root only on the host: {} (commands.sh replays what ran)\n",
        dir
    ));
    Ok(out)
}
