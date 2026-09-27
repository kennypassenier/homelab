//! ask-9 (Kenny, 2026-09-27): what a stack, app or native unit leaves
//! behind when it leaves the files is KEPT FOREVER by default.
//!
//! The restic repositories, the /appdata directories and the vault copies of
//! anything destroyed, forgotten or dropped from a stack file stay where they
//! are. Three things make that safe rather than a slow leak:
//!
//! * the operation that retires something records exactly what it left
//!   (`HostState::retired`), so "what are we keeping" has an answer that
//!   does not depend on anybody's memory;
//! * the fleet check names every entry as `Noted` — visible on every
//!   `homelab check` and in the nightly round, never a failure;
//! * `homelab wipe <key>` is the only way to delete them, after the name is
//!   typed. Nothing automatic calls it: not the nightly round, not a deploy,
//!   not `apply`.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::native::NativeServiceManifest;
use crate::ops::fleetcheck::{Finding, Severity};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;
use crate::state::{HostState, RetiredKind, RetiredRecord};

use super::OpCtx;

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

/// The restic repositories a manifest's storage writes to: one per owner of
/// a directory that is actually backed up (`<owner>-config`, D25).
pub fn repos_of(m: &StackManifest) -> Vec<String> {
    sorted(
        m.storage
            .iter()
            .filter(|s| !s.no_data && s.no_backup.is_none())
            .map(|s| format!("{}-config", s.owner(&m.stack_name)))
            .collect(),
    )
}

/// The repositories native units back up to (`<unit>-config`); a stateless
/// unit keeps none.
fn repos_of_units(natives: &[NativeServiceManifest]) -> Vec<String> {
    natives
        .iter()
        .filter(|n| !n.stateless)
        .map(|n| format!("{}-config", n.unit))
        .collect()
}

/// Record a whole stack as retired (destroy, forget). Anything recorded
/// earlier for one of its apps or units is folded into this record, so a
/// wipe of the stack covers everything the stack ever left behind.
pub fn retire_stack(
    state: &mut HostState,
    stack: &str,
    vmid: u16,
    manifest: Option<&StackManifest>,
    natives: &[NativeServiceManifest],
    state_dir: &str,
    now: u64,
) {
    let mut repos = manifest.map(repos_of).unwrap_or_default();
    repos.extend(repos_of_units(natives));
    let mut appdata: Vec<String> = manifest
        .map(|m| m.storage.iter().map(|s| s.host_path.clone()).collect())
        .unwrap_or_default();
    let vault = vec![format!("{}/secrets/{}", state_dir, stack)];
    let prefix = format!("{}/", stack);
    let folded: Vec<String> = state
        .retired
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    for k in folded {
        if let Some(r) = state.retired.remove(&k) {
            repos.extend(r.repos);
            appdata.extend(r.appdata);
            // Their vault copies live inside the stack's vault directory,
            // which this record already names as a whole.
        }
    }
    state.retired.insert(
        stack.to_string(),
        RetiredRecord {
            kind: RetiredKind::Stack,
            stack: stack.to_string(),
            name: stack.to_string(),
            vmid,
            retired_at: now,
            repos: sorted(repos),
            appdata: sorted(appdata),
            vault,
        },
    );
}

/// Record an app that left a stack which still exists. Keeps the first
/// date: a second deploy without the app is not a second departure.
pub fn retire_app(
    state: &mut HostState,
    prior: &StackManifest,
    app: &str,
    state_dir: &str,
    now: u64,
) {
    let key = format!("{}/{}", prior.stack_name, app);
    if state.retired.contains_key(&key) {
        return;
    }
    let mine: Vec<&crate::manifest::MountSpec> = prior
        .storage
        .iter()
        .filter(|s| s.app.as_deref() == Some(app))
        .collect();
    state.retired.insert(
        key,
        RetiredRecord {
            kind: RetiredKind::App,
            stack: prior.stack_name.clone(),
            name: app.to_string(),
            vmid: prior.vmid,
            retired_at: now,
            repos: sorted(
                mine.iter()
                    .filter(|s| !s.no_data && s.no_backup.is_none())
                    .map(|_| format!("{}-config", app))
                    .collect(),
            ),
            appdata: sorted(mine.iter().map(|s| s.host_path.clone()).collect()),
            vault: vec![format!(
                "{}/secrets/{}/{}.env",
                state_dir, prior.stack_name, app
            )],
        },
    );
}

/// Record a native unit dropped from a stack's `natives:`. Its data dirs
/// stay inside the container (ask-8), its repository and vault copies here.
pub fn retire_unit(
    state: &mut HostState,
    stack: &str,
    vmid: u16,
    unit: &str,
    vault: Vec<String>,
    now: u64,
) {
    let key = format!("{}/{}", stack, unit);
    if state.retired.contains_key(&key) {
        return;
    }
    state.retired.insert(
        key,
        RetiredRecord {
            kind: RetiredKind::Unit,
            stack: stack.to_string(),
            name: unit.to_string(),
            vmid,
            retired_at: now,
            repos: vec![format!("{}-config", unit)],
            appdata: Vec::new(),
            vault: sorted(vault),
        },
    );
}

/// A deploy brought these back: they are no longer retired.
pub fn unretire(state: &mut HostState, m: &StackManifest) {
    state.retired.remove(&m.stack_name);
    for name in m.apps.iter().chain(m.natives.iter()) {
        state.retired.remove(&format!("{}/{}", m.stack_name, name));
    }
}

fn list(v: &[String]) -> String {
    if v.is_empty() {
        "none".into()
    } else {
        v.join(", ")
    }
}

/// ask-9: one `Noted` line per retired entry, naming what is kept and since
/// when — so keeping forever never turns into keeping without knowing.
pub fn evaluate_retired(state: &HostState) -> Vec<Finding> {
    state
        .retired
        .iter()
        .map(|(key, r)| Finding {
            severity: Severity::Noted,
            subject: key.clone(),
            what: format!(
                "retired {} ({}, vmid {}) — kept: restic {}; /appdata {}; vault {}",
                crate::state::ymd(r.retired_at),
                match r.kind {
                    RetiredKind::Stack => "stack",
                    RetiredKind::App => "app",
                    RetiredKind::Unit => "native unit",
                },
                r.vmid,
                list(&r.repos),
                list(&r.appdata),
                list(&r.vault)
            ),
            remedy: format!(
                "kept on purpose until you decide (ask-9); `homelab wipe {}` deletes exactly \
                 these after you type the name",
                key
            ),
        })
        .collect()
}

/// What a wipe of one retired entry would delete.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WipePlan {
    /// Restic repository names under `restic_base`.
    pub repos: Vec<String>,
    /// Host directories under /appdata.
    pub appdata: Vec<String>,
    /// Vault paths under `<state_dir>/secrets/`.
    pub vault: Vec<String>,
    /// Recorded, but used by a stack that is still managed (an app that
    /// moved stacks keeps its repository, D25) — never deleted.
    pub in_use: Vec<String>,
}

impl WipePlan {
    /// The list the operator reads before typing the name.
    pub fn render(&self, key: &str) -> String {
        let mut out = format!("wipe '{}' deletes, permanently:", key);
        for r in &self.repos {
            out.push_str(&format!("\n  restic repository  {}", r));
        }
        for p in &self.appdata {
            out.push_str(&format!("\n  /appdata directory {}", p));
        }
        for v in &self.vault {
            out.push_str(&format!("\n  vault copy         {}", v));
        }
        if self.repos.is_empty() && self.appdata.is_empty() && self.vault.is_empty() {
            out.push_str("\n  nothing (only the record)");
        }
        for u in &self.in_use {
            out.push_str(&format!("\n  KEPT (a managed stack still uses it) {}", u));
        }
        out
    }
}

/// Work out what a wipe of `key` would delete. Refuses anything that is not
/// a retired entry, or whose stack or app is back under management.
pub fn wipe_plan(state: &HostState, key: &str, state_dir: &str) -> Result<WipePlan, String> {
    let Some(r) = state.retired.get(key) else {
        return Err(if state.stacks.contains_key(key) {
            format!(
                "'{}' is a managed stack, not a retired one — only what a destroy, forget or \
                 deploy retired can be wiped",
                key
            )
        } else {
            format!("nothing retired is recorded as '{}'", key)
        });
    };
    if r.kind == RetiredKind::Stack && state.stacks.contains_key(&r.stack) {
        return Err(format!(
            "'{}' is managed again — its record is stale; deploy it once to clear it",
            key
        ));
    }
    if r.kind != RetiredKind::Stack {
        if let Some(st) = state.stacks.get(&r.stack) {
            let back = st.apps.contains(&r.name)
                || st
                    .manifest
                    .as_ref()
                    .is_some_and(|m| m.natives.contains(&r.name));
            if back {
                return Err(format!(
                    "'{}' is back in stack '{}' — refusing to delete what it uses",
                    r.name, r.stack
                ));
            }
        }
    }
    // What every managed stack still uses.
    let mut repos_used: Vec<String> = Vec::new();
    let mut paths_used: Vec<String> = Vec::new();
    for st in state.stacks.values() {
        if let Some(m) = st.manifest.as_ref() {
            repos_used.extend(repos_of(m));
            paths_used.extend(m.storage.iter().map(|s| s.host_path.clone()));
            paths_used.extend(m.data_mounts.iter().map(|d| d.host_path.clone()));
        }
        repos_used.extend(repos_of_units(&st.natives));
    }
    let mut plan = WipePlan::default();
    for repo in &r.repos {
        if repo.is_empty() || repo.contains('/') || repo.contains("..") {
            return Err(format!(
                "recorded repository '{}' is not a plain name",
                repo
            ));
        }
        if repos_used.contains(repo) {
            plan.in_use.push(repo.clone());
        } else {
            plan.repos.push(repo.clone());
        }
    }
    for p in &r.appdata {
        let rest = p.strip_prefix("/appdata/").unwrap_or("");
        if rest.is_empty()
            || rest
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(format!(
                "recorded directory '{}' is not a directory under /appdata/ — refusing",
                p
            ));
        }
        if paths_used
            .iter()
            .any(|u| u == p || u.starts_with(&format!("{}/", p)))
        {
            plan.in_use.push(p.clone());
        } else {
            plan.appdata.push(p.clone());
        }
    }
    let vault_root = format!("{}/secrets/", state_dir);
    for v in &r.vault {
        let rest = v.strip_prefix(&vault_root).unwrap_or("");
        if rest.is_empty()
            || rest
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(format!(
                "recorded vault path '{}' is not inside {} — refusing",
                v, vault_root
            ));
        }
        plan.vault.push(v.clone());
    }
    Ok(plan)
}

/// `homelab wipe <key>`: delete what a retired entry kept, then the record.
///
/// Never automatic (ask-9). `confirmed_name` is the name the operator typed;
/// it is checked here as well so core is safe on its own. Repositories that
/// a managed stack still uses are left alone and named. A failure stops the
/// wipe and keeps the record, so it can be run again.
pub async fn wipe(ctx: &OpCtx<'_>, key: &str, confirmed_name: &str) -> OperationReport {
    let op = format!("wipe-{}", key.replace('/', "-"));
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let say = |msg: String| {
        ctx.sink.emit(crate::sink::PipelineEvent::Line {
            level: Level::Warn,
            source: "HOST".into(),
            msg,
        })
    };

    if let Err(e) = runner
        .step("confirm", || async {
            if confirmed_name != key {
                return Err(CoreError::SafetyAbort(format!(
                    "typed name '{}' does not match '{}' — nothing deleted",
                    confirmed_name, key
                )));
            }
            Ok(StepOutcome::Unchanged)
        })
        .await
    {
        return runner.finish_err("confirm", &e);
    }

    let mut plan = WipePlan::default();
    if let Err(e) = runner
        .step("plan", || async {
            let state = crate::state::StateStore::new(exec, &ctx.state_dir)
                .load()
                .await?;
            plan = wipe_plan(&state, key, &ctx.state_dir).map_err(CoreError::SafetyAbort)?;
            Ok(StepOutcome::Unchanged)
        })
        .await
    {
        return runner.finish_err("plan", &e);
    }
    for u in &plan.in_use {
        say(format!("[wipe] kept {} — a managed stack still uses it", u));
    }

    if let Err(e) = runner
        .step("restic repositories", || async {
            if plan.repos.is_empty() {
                return Ok(StepOutcome::Unchanged);
            }
            let Some(remote) = ctx.backup.restic_base.strip_prefix("rclone:") else {
                return Err(CoreError::SafetyAbort(format!(
                    "restic_base '{}' is not an rclone remote — only those can be wiped from \
                     here; remove the repositories by hand",
                    ctx.backup.restic_base
                )));
            };
            for repo in &plan.repos {
                let path = format!("{}/{}", remote.trim_end_matches('/'), repo);
                run_ok(exec, &Cmd::new("rclone", &["purge", &path], 3600)).await?;
                say(format!("[wipe] deleted restic repository {}", path));
            }
            Ok(StepOutcome::Changed)
        })
        .await
    {
        return runner.finish_err("restic repositories", &e);
    }

    if let Err(e) = runner
        .step("appdata and vault", || async {
            let mut changed = false;
            for p in plan.appdata.iter().chain(plan.vault.iter()) {
                run_ok(exec, &Cmd::new("rm", &["-rf", "--", p], 600)).await?;
                say(format!("[wipe] deleted {}", p));
                changed = true;
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        })
        .await
    {
        return runner.finish_err("appdata and vault", &e);
    }

    if let Err(e) = runner
        .step("update state", || async {
            let store = crate::state::StateStore::new(exec, &ctx.state_dir);
            let mut state = store.load().await?;
            state.retired.remove(key);
            store.save(state).await?;
            Ok(StepOutcome::Changed)
        })
        .await
    {
        return runner.finish_err("update state", &e);
    }
    runner.log(
        Level::Info,
        format!("[wipe] '{}' is gone, with everything it kept", key),
    );
    runner.finish_ok()
}
