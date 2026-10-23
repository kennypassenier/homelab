//! app-settings-enforced-1 (Kenny, 2026-10-23: "zie dat fixes permanent zijn
//! en dat het admin dashboard dit kan enforcen"): an app setting that a fix
//! depends on stays fixed.
//!
//! The case: qBittorrent let fake releases in (.7z, .iso) until its
//! `Session\ExcludedFileNames` was extended by hand (stop the container, edit
//! the file, start it). A probe in the app's `checks.yml` read the running
//! value in every fleet check, but a probe detects and nothing repaired.
//!
//! The app's `checks.yml` now carries a `settings:` list ([`AppSetting`]: the
//! file inside the container, its format, the key, the wanted value, and
//! whether the app must be stopped while the file is written). A deploy
//! registers the list in the host state; the deploy itself, every update and
//! the nightly round then call [`enforce_app`]:
//!
//! - the value already matches: nothing is written, nothing is stopped, one
//!   debug line is logged;
//! - it differs: the app is stopped (only when the entry says it must be),
//!   the file is read again (an app that rewrites its file on exit may have
//!   changed it), ONLY that key is written (every other byte of the file
//!   stays as it was), the app is started again, and a warning names the key
//!   and the app so it reaches the operator as a notice.
//!
//! **Never while a backup of the stack runs.** Every mutating operation holds
//! the host's one operation lock (AR12), and the nightly backups hold it for
//! the whole batch, so a settings write cannot overlap a backup; the nightly
//! round also calls this only after the backup batch returned.
//!
//! File contents are read through the quiet runner: an app config can hold a
//! password hash or an API key (qBittorrent's does) and the traced transcript
//! goes to Loki.

use std::collections::BTreeMap;

use crate::checks::{AppSetting, ServiceChecks, SettingFormat};
use crate::error::CoreError;
use crate::executor::{Executor, pct_sh, pct_sh_secret};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::{Level, PipelineEvent, Sink};
use crate::state::HostState;

use super::OpCtx;
use super::util::{compose_in_app, compose_running_services, push_content_staged, shq};

/// Replace the stack's wanted settings with what its deploy declared. An app
/// that dropped its last entry loses it; other stacks are untouched.
pub fn register(state: &mut HostState, stack: &str, checks: &BTreeMap<String, ServiceChecks>) {
    let prefix = format!("{}/", stack);
    state.app_settings.retain(|k, _| !k.starts_with(&prefix));
    for (app, sc) in checks {
        if !sc.settings.is_empty() {
            state
                .app_settings
                .insert(format!("{}/{}", stack, app), sc.settings.clone());
        }
    }
}

/// The wanted settings registered for one app; empty when it has none.
pub fn wanted(state: &HostState, stack: &str, app: &str) -> Vec<AppSetting> {
    state
        .app_settings
        .get(&format!("{}/{}", stack, app))
        .cloned()
        .unwrap_or_default()
}

/// Whether any app of the stack has a registered setting (the nightly round
/// runs the operation only then).
pub fn stack_has_settings(state: &HostState, stack: &str) -> bool {
    let prefix = format!("{}/", stack);
    state.app_settings.keys().any(|k| k.starts_with(&prefix))
}

/// Edit one file's text so that `setting` holds, touching nothing else.
/// `None` when it already holds. Line endings of untouched lines are kept as
/// they are; a replaced line keeps its own ending and the spacing before its
/// `=`; a missing key is added at the end of its section (a missing section at
/// the end of the file).
pub fn edit_text(text: &str, setting: &AppSetting) -> Option<String> {
    let ini = setting.format == SettingFormat::Ini;
    let section = setting.section.as_deref().unwrap_or("");
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    let mut in_section = !ini;
    let mut section_seen = !ini;
    // Where a missing key goes: after the last content line of the section.
    let mut insert_at: Option<usize> = if ini { None } else { Some(lines.len()) };
    let mut found = false;
    let mut changed = false;
    for (i, raw) in lines.iter_mut().enumerate() {
        let body = raw.trim_end_matches(['\n', '\r']).to_string();
        let ending = raw[body.len()..].to_string();
        let t = body.trim();
        if ini && t.starts_with('[') && t.ends_with(']') {
            in_section = &t[1..t.len() - 1] == section;
            if in_section {
                section_seen = true;
                insert_at = Some(i + 1);
            }
            continue;
        }
        if !in_section || t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if ini {
            insert_at = Some(i + 1);
        }
        let Some((k, v)) = body.split_once('=') else {
            continue;
        };
        if k.trim() != setting.key {
            continue;
        }
        found = true;
        if v != setting.value {
            *raw = format!("{}={}{}", k, setting.value, ending);
            changed = true;
        }
    }
    if found {
        return changed.then(|| lines.concat());
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let new_line = format!("{}={}{}", setting.key, setting.value, eol);
    if ini && !section_seen {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(eol);
        }
        if !out.is_empty() {
            out.push_str(eol);
        }
        out.push_str(&format!("[{}]{}{}", section, eol, new_line));
        return Some(out);
    }
    let at = insert_at.unwrap_or(lines.len());
    if at > 0 && !lines[at - 1].ends_with('\n') {
        lines[at - 1].push_str(eol);
    }
    lines.insert(at, new_line);
    Some(lines.concat())
}

/// What one app's enforcement did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Enforced {
    /// Keys that were wrong and are now written, in file order.
    pub put_back: Vec<String>,
    /// Entries whose value already matched.
    pub matched: usize,
    /// Files that do not exist (yet), so nothing could be put back.
    pub missing: Vec<String>,
}

fn line(sink: &dyn Sink, level: Level, msg: String) {
    sink.emit(PipelineEvent::Line {
        level,
        source: "HOST".into(),
        msg,
    });
}

/// Read one file inside the container; `None` when it does not exist.
async fn read_file(
    exec: &dyn Executor,
    vmid: u16,
    path: &str,
) -> Result<Option<String>, CoreError> {
    let script = format!(
        "if [ -f {p} ]; then cat {p}; else exit 3; fi",
        p = shq(path)
    );
    let out = pct_sh_secret(exec, vmid, &script, 30).await?;
    match out.code {
        0 => {
            if out.stdout.contains('\u{FFFD}') {
                return Err(CoreError::Other(format!(
                    "{} is not valid UTF-8; refusing to rewrite it",
                    path
                )));
            }
            Ok(Some(out.stdout))
        }
        3 => Ok(None),
        c => Err(CoreError::Other(format!(
            "reading {} failed (rc={}): {}",
            path,
            c,
            out.stderr.trim()
        ))),
    }
}

/// Apply every entry to `text`; the new text when any differed, with the keys
/// that did, and how many already matched.
fn apply_all(text: &str, group: &[&AppSetting]) -> (Option<String>, Vec<String>, usize) {
    let mut cur = text.to_string();
    let mut keys = Vec::new();
    let mut matched = 0;
    for s in group {
        match edit_text(&cur, s) {
            Some(next) => {
                cur = next;
                keys.push(s.key.clone());
            }
            None => matched += 1,
        }
    }
    ((!keys.is_empty()).then_some(cur), keys, matched)
}

/// Make one app's files hold the wanted values. The caller holds the host's
/// operation lock (see the module doc). `exec` may be the tracing executor:
/// file contents are only ever read through the quiet runner.
pub async fn enforce_app(
    exec: &dyn Executor,
    sink: &dyn Sink,
    vmid: u16,
    stack: &str,
    app: &str,
    settings: &[AppSetting],
) -> Result<Enforced, CoreError> {
    let mut report = Enforced::default();
    // One pass per file, in the order the entries first name them.
    let mut files: Vec<&str> = Vec::new();
    for s in settings {
        if !files.contains(&s.file.as_str()) {
            files.push(&s.file);
        }
    }
    for file in files {
        let group: Vec<&AppSetting> = settings.iter().filter(|s| s.file == file).collect();
        let Some(text) = read_file(exec, vmid, file).await? else {
            line(
                sink,
                Level::Info,
                format!(
                    "[settings] {}: {} does not exist yet, nothing to put back",
                    app, file
                ),
            );
            report.missing.push(file.to_string());
            continue;
        };
        let (mut new_text, mut keys, matched) = apply_all(&text, &group);
        if new_text.is_none() {
            line(
                sink,
                Level::Debug,
                format!(
                    "[settings] {}: {} setting(s) in {} already as wanted",
                    app, matched, file
                ),
            );
            report.matched += matched;
            continue;
        }
        let must_stop = group.iter().any(|s| s.stop_app);
        let mut stopped = false;
        if must_stop {
            // An app that is not running is left alone: nothing rewrites its
            // file, and it was not started by us so it is not started by us.
            let running = compose_running_services(exec, vmid, stack, app, "")
                .await?
                .is_none_or(|v| !v.is_empty());
            if running {
                let out = compose_in_app(exec, vmid, stack, app, "docker compose stop -t 60", 180)
                    .await?;
                if !out.success() {
                    return Err(CoreError::Other(format!(
                        "could not stop {} to write {}: {}",
                        app,
                        file,
                        out.stderr.trim()
                    )));
                }
                stopped = true;
                // It rewrites its file on the way out: decide again from what
                // it left, never from the copy read while it ran.
                let again = read_file(exec, vmid, file).await?.unwrap_or_default();
                let (n, k, _) = apply_all(&again, &group);
                new_text = n;
                keys = k;
            }
        }
        let written = match &new_text {
            Some(body) => write_file(exec, vmid, file, body).await,
            None => Ok(()),
        };
        if stopped {
            // Started again even when the write failed: a failed fix must
            // not also leave the app down.
            let up = compose_in_app(exec, vmid, stack, app, "docker compose start", 300).await?;
            if !up.success() {
                return Err(CoreError::Other(format!(
                    "could not start {} again: {}",
                    app,
                    up.stderr.trim()
                )));
            }
        }
        written?;
        report.matched += matched;
        for key in keys {
            line(
                sink,
                Level::Warn,
                format!("[settings] put back {} in {} ({})", key, app, file),
            );
            report.put_back.push(key);
        }
    }
    Ok(report)
}

/// Replace a file's content in place (`cat >` keeps its owner and mode, which
/// belong to the app's user), through a staged copy so no content rides on a
/// command line.
async fn write_file(
    exec: &dyn Executor,
    vmid: u16,
    dest: &str,
    body: &str,
) -> Result<(), CoreError> {
    let tmp = format!(
        "/tmp/homelab-setting-{}",
        super::util::staging_path(vmid, dest)
            .rsplit('-')
            .next()
            .unwrap_or("new")
    );
    push_content_staged(
        exec,
        vmid,
        &tmp,
        body,
        "0600",
        &super::util::staging_path(vmid, &tmp),
    )
    .await?;
    let out = pct_sh(
        exec,
        vmid,
        &format!("cat {t} > {d} && rm -f {t}", t = shq(&tmp), d = shq(dest)),
        30,
    )
    .await?;
    if out.success() {
        Ok(())
    } else {
        Err(CoreError::Other(format!(
            "writing {} failed: {}",
            dest,
            out.stderr.trim()
        )))
    }
}

/// The deploy/update flavour: a setting that cannot be put back is a warning
/// naming it, never a failed deploy or a skipped update. `Changed` when
/// anything was put back.
pub async fn enforce_or_warn(
    exec: &dyn Executor,
    sink: &dyn Sink,
    vmid: u16,
    stack: &str,
    app: &str,
    settings: &[AppSetting],
) -> StepOutcome {
    if settings.is_empty() {
        return StepOutcome::Unchanged;
    }
    match enforce_app(exec, sink, vmid, stack, app, settings).await {
        Ok(r) if !r.put_back.is_empty() => StepOutcome::Changed,
        Ok(_) => StepOutcome::Unchanged,
        Err(e) => {
            line(
                sink,
                Level::Warn,
                format!("[settings] {}: could not put the settings back: {}", app, e),
            );
            StepOutcome::Unchanged
        }
    }
}

/// The step name for one app, shared by the plan and the steps.
pub fn app_step(app: &str) -> String {
    format!("{} :: settings", app)
}

/// The nightly operation's whole plan: "safety gates", then one step per app.
pub fn enforce_plan_names(m: &StackManifest) -> Vec<String> {
    std::iter::once("safety gates".to_string())
        .chain(m.apps.iter().map(|a| app_step(a)))
        .collect()
}

/// The nightly round's pass over one stack: every app's registered settings.
/// A step per app (skipped when the app has none), so a failure names the app.
pub async fn enforce(ctx: &OpCtx<'_>, m: &StackManifest) -> OperationReport {
    let op = format!("settings-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = crate::executor::TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let plan = enforce_plan_names(m);
    runner.plan(&plan.iter().map(String::as_str).collect::<Vec<_>>());
    let state = match crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .load()
        .await
    {
        Ok(s) => s,
        Err(e) => return runner.finish_err("safety gates", &e),
    };
    step!(runner, "safety gates", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });
    for app in &m.apps {
        let name = app_step(app);
        let wanted = wanted(&state, &m.stack_name, app);
        if wanted.is_empty() {
            runner.skip(&name);
            continue;
        }
        step!(runner, &name, {
            let r = enforce_app(exec, ctx.sink, m.vmid, &m.stack_name, app, &wanted).await?;
            Ok(if r.put_back.is_empty() {
                StepOutcome::Unchanged
            } else {
                StepOutcome::Changed
            })
        });
    }
    runner.finish_ok()
}
