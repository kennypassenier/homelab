//! What a stack edit, a new stack and an import all plan: the changes, the
//! checked staging directory, and the plan's JSON.

use homelab_proto::StackManifest;

use super::check::{Checked, check_dir, others};
use crate::core::actions::{Refusal, SELF_STACK};
use crate::core::editplan::{self, Effect, FileDiff};
use crate::core::newstack;
use crate::core::stackedit::{self, AddAppFiles, FileChange, MANIFEST, StackEdit, StackTexts};
use crate::shell::workcopy::WorkingCopy;

pub(super) struct Planned {
    pub(super) changes: Vec<FileChange>,
    pub(super) checked: Checked,
    pub(super) old: Option<StackManifest>,
    pub(super) head: Option<String>,
    pub(super) sync_error: Option<String>,
    /// The change in words, for the commit subject (`stackedit::describe`).
    pub(super) summary: String,
}

/// What the edit would change on the machines, from the new manifest.
fn plan_effects(stack: &str, p: &Planned) -> Vec<Effect> {
    let manifest_path = format!("stacks/{stack}/{MANIFEST}");
    let others: Vec<String> = p
        .changes
        .iter()
        .filter(|c| c.path != manifest_path)
        .map(|c| c.path.clone())
        .collect();
    match &p.checked.manifest {
        // tile-watch: `plan_json` is pure and has no host.toml fetch of its
        // own, so the plan does not yet show the derived tile-watch rule —
        // only the hand-declared ones, same as before that decision. See
        // the report for wiring the fleet-wide `tile_watch_source` in here.
        // tile-watch-watcher-out: same reason, the fleet-wide targets are
        // empty here too — `plan_json` has no `HostState` to derive them
        // from, so a watcher stack's own preview shows no OUT rule yet.
        Some(new) => editplan::effects(p.old.as_ref(), new, &others, None, &Vec::new()),
        None => Vec::new(),
    }
}

pub(super) fn plan_json(
    stack: &str,
    kind: &str,
    feature: &str,
    p: &Planned,
    applied: serde_json::Value,
) -> (serde_json::Value, Vec<Effect>, Vec<FileDiff>) {
    let diffs = editplan::file_diffs(&p.changes);
    let effects = plan_effects(stack, p);
    let summary = if p.summary.is_empty() {
        editplan::summary(kind, &diffs, &effects)
    } else {
        p.summary.clone()
    };
    let subject = editplan::commit_subject(stack, &summary, feature);
    let json = serde_json::json!({
        "stack": stack,
        "kind": kind,
        "head": p.head,
        "sync_error": p.sync_error,
        "files": diffs,
        "effects": effects,
        "follow_ups": editplan::follow_ups(&effects),
        "problems": p.checked.problems,
        "valid": p.checked.problems.is_empty() && !p.changes.is_empty(),
        "unchanged": p.changes.is_empty(),
        "applied": applied,
        "subject": subject,
        "restarts_dashboard": stack == SELF_STACK,
    });
    (json, effects, diffs)
}

/// The preset's apps as files of `stack` (feat-stacks-3, add an app).
fn add_app_files(
    wc: &WorkingCopy,
    stack: &str,
    preset: &str,
    vmid: u16,
) -> Result<AddAppFiles, Refusal> {
    let presets = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"));
    let p = presets.iter().find(|p| p.name == preset).ok_or_else(|| {
        Refusal::new(
            "the app",
            format!("there is no preset {preset:?}"),
            "pick one the wizard lists",
        )
    })?;
    let Some(dir) = &p.dir else {
        return Err(Refusal::new(
            "the app",
            format!("the preset {preset} has no files"),
            "pick another",
        ));
    };
    if p.apps.is_empty() {
        return Err(Refusal::new(
            "the app",
            format!("the preset {preset} holds no app"),
            "pick another",
        ));
    }
    let ip =
        newstack::ip_for(vmid).unwrap_or_else(|| format!("10.10.10.{}", vmid.saturating_sub(100)));
    let mut out = AddAppFiles {
        apps: p.apps.clone(),
        owner_uid: homelab_client::scaffold::StackDefaults::default().appdata_owner_uid,
        ..Default::default()
    };
    fill_app_files(&mut out, dir, stack, vmid, &ip);
    Ok(out)
}

/// Read every app's files out of the preset, substituted for `stack`, and
/// collect the `/appdata` folders their compose files bind.
fn fill_app_files(out: &mut AddAppFiles, dir: &std::path::Path, stack: &str, vmid: u16, ip: &str) {
    let mut appdata = std::collections::BTreeSet::new();
    for app in out.apps.clone() {
        for (rel, raw) in crate::shell::workcopy::read_texts(&dir.join(&app)) {
            let content = homelab_client::scaffold::substitute(&raw, stack, vmid, ip);
            if rel.ends_with("docker-compose.yml") {
                appdata.extend(homelab_client::scaffold::appdata_paths_in(&content));
            }
            out.files.insert(format!("{app}/{rel}"), content);
        }
    }
    out.appdata = appdata.into_iter().collect();
}

/// Everything a stack edit's plan and commit share, read under the working
/// copy's lock.
pub(super) fn prepare(wc: &WorkingCopy, stack: &str, edit: &StackEdit) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let texts: StackTexts = wc.stack_texts(stack)?;
    let old = texts
        .get(MANIFEST)
        .and_then(|t| stackedit::parse_manifest(t).ok());
    let add = match edit {
        StackEdit::AddApp { preset, .. } => {
            let vmid = old.as_ref().map(|m| m.vmid).unwrap_or(0);
            Some(add_app_files(wc, stack, preset, vmid)?)
        }
        _ => None,
    };
    let changes = stackedit::changes(stack, &texts, edit, add.as_ref())?;
    let others = others(wc, stack);
    let checked = wc.with_staged(stack, &changes, |dir| check_dir(dir, stack, &others))?;
    let head = wc.status().head.map(|h| h.commit);
    let summary = stackedit::describe(edit, old.as_ref());
    Ok(Planned {
        changes,
        checked,
        old,
        head,
        sync_error,
        summary,
    })
}
