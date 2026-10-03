//! C7 adoption: take over a hand-built native-service container without
//! restarting it. Verification first — the container must really be what
//! the manifest claims — then the stack is recorded in state so the
//! nightly machinery (backup, update supervision) picks it up.

use crate::error::CoreError;
use crate::executor::{Cmd, CmdOutput, Executor, TracingExecutor, run_ok};
use crate::native::{BackupPause, NativeServiceManifest};
use crate::runner::{OperationReport, Runner, Scope, StepFailure, StepOutcome};
use crate::sink::Level;

use super::util::shq;
use super::{OpCtx, util_pct_sh};

/// T77: a service's own nightly copy older than this is not tonight's copy.
/// 26 h leaves room for a late run without accepting yesterday's file.
pub const MAX_OWN_COPY_AGE_S: u64 = 26 * 3600;

/// chassis-rs `backup-pause --for <secs>`: the dead-man switch's own range
/// (the kit refuses outside 1..21600).
pub const MAX_CHASSIS_PAUSE_FOR_S: u64 = 21600;

/// fix-113 ADDENDUM (owner + chassis-rs agreement, 2026-10-01): "no guessed
/// N" — every `backup-pause --for` call, first and every renewal, asks for
/// this short a window. A run that dies mid-copy leaves the service
/// un-paused again within this long, never parked for hours on a guess.
pub const CHASSIS_PAUSE_FOR_S: u64 = 120;

/// How often the pause is renewed while work (the local copy, or — with no
/// staging — the tar-to-restic pipe itself) is still running under it.
pub const CHASSIS_HEARTBEAT_INTERVAL_S: u64 = 60;

/// The safety margin `fits_staging` asks of a staged copy's estimated size
/// before it trusts the estimate against free space and the cap.
const STAGING_SAFETY_MARGIN_PCT: u64 = 20;

/// fix-113 ADDENDUM: whether a staged local copy of `estimated_bytes` is
/// trusted to fit `free_bytes` of disk and the `cap_mib` staging cap, both
/// after a 20% margin on the estimate. Pure — the `du`/`df` readings that
/// feed it are the only I/O, done by the caller.
pub fn fits_staging(estimated_bytes: u64, free_bytes: u64, cap_mib: u64) -> bool {
    let needed = estimated_bytes.saturating_mul(100 + STAGING_SAFETY_MARGIN_PCT) / 100;
    let cap_bytes = cap_mib.saturating_mul(1024 * 1024);
    needed <= free_bytes && needed <= cap_bytes
}

/// fix-113 ADDENDUM: `du -scb` over every data dir, inside the container —
/// `None` when it cannot be read (no `du`, or a dir does not exist), in
/// which case the caller skips staging rather than guessing a size.
async fn estimate_data_bytes(exec: &dyn Executor, vmid: u16, data_dirs: &[String]) -> Option<u64> {
    if data_dirs.is_empty() {
        return Some(0);
    }
    let dirs = data_dirs
        .iter()
        .map(|d| shq(d))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!("du -scb {} 2>/dev/null | tail -1 | cut -f1", dirs);
    let out = util_pct_sh(exec, vmid, &script, 60).await.ok()?;
    out.success()
        .then(|| out.stdout.trim().parse::<u64>().ok())?
}

/// fix-113 ADDENDUM: free bytes on the filesystem holding `dir` (created
/// first if missing) on THIS host — the staging directory is never inside
/// the container. `None` when it cannot be read.
async fn staging_free_bytes(exec: &dyn Executor, dir: &str) -> Option<u64> {
    let script = format!(
        "mkdir -p {d} && df -B1 --output=avail {d} | tail -1",
        d = shq(dir)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 30)).await.ok()?;
    out.success()
        .then(|| out.stdout.trim().parse::<u64>().ok())?
}

/// fix-113 ADDENDUM: deletes a staged tar — called before trusting a free-
/// space reading (a leftover from a run that died between writing it and
/// deleting it again, rule 20) and after every staged snapshot, success or
/// failure, so one is never left behind for the next run to find.
async fn remove_staging_file(exec: &dyn Executor, path: &str) {
    let _ = exec
        .run(&Cmd::new(
            "sh",
            &["-c", &format!("rm -f {}", shq(path))],
            30,
        ))
        .await;
}

/// fix-113 ADDENDUM: renews the chassis pause every
/// [`CHASSIS_HEARTBEAT_INTERVAL_S`] while the work it is racing against
/// ([`run_under_chassis_heartbeat`]) is still running. Bounded by
/// `max_renewals` so it can only ever LOSE that race, never win it: once
/// exhausted it parks on a future that never completes rather than ending
/// the race early and leaving the other side cut off mid-copy.
async fn chassis_pause_heartbeat(exec: &dyn Executor, vmid: u16, binary: &str, max_renewals: u64) {
    for _ in 0..max_renewals {
        exec.sleep_ms(CHASSIS_HEARTBEAT_INTERVAL_S * 1000).await;
        let _ = util_pct_sh(
            exec,
            vmid,
            &format!("{} backup-pause --for {}", shq(binary), CHASSIS_PAUSE_FOR_S),
            30,
        )
        .await;
    }
    std::future::pending::<()>().await;
}

/// fix-113 ADDENDUM: runs `cmd` while the chassis pause is renewed
/// underneath it, so work that outlives the first `--for 120` window is not
/// cut off mid-write. `cmd`'s own `timeout_s` bounds how many renewals the
/// heartbeat is allowed (`+2` of headroom) — comfortably more than the
/// heartbeat could ever need before `cmd` itself resolves (success, failure
/// or its own timeout), so the heartbeat branch structurally cannot win the
/// race in [`futures_util::future::select`] (which polls the first future —
/// `cmd` — before the second on every poll, so a `cmd` that is already done,
/// as every scripted `MockExecutor` response is, never triggers a single
/// renewal: every existing test exercises this exact path unchanged).
async fn run_under_chassis_heartbeat(
    exec: &dyn Executor,
    vmid: u16,
    binary: &str,
    cmd: &Cmd,
) -> Result<CmdOutput, CoreError> {
    let max_renewals = cmd.timeout_s / CHASSIS_HEARTBEAT_INTERVAL_S + 2;
    let work = Box::pin(run_ok(exec, cmd));
    let heartbeat = Box::pin(chassis_pause_heartbeat(exec, vmid, binary, max_renewals));
    match futures_util::future::select(work, heartbeat).await {
        futures_util::future::Either::Left((out, _heartbeat)) => out,
        futures_util::future::Either::Right(((), _work)) => {
            unreachable!("chassis_pause_heartbeat never completes before cmd's own timeout")
        }
    }
}

/// Adopt an existing container as a managed native-service stack. Never
/// starts, stops or restarts anything — a running production service is
/// exactly what must stay untouched while the homelab takes ownership.
/// fix-171 round 3: adoption's own fixed step plan, announced before its
/// first step. Every step here is unconditional — a precondition that does
/// not hold (no env file to seal) makes the step a no-op, never an absent
/// one — so this list is the same for every run, like `INSTALL_STEPS`.
pub const ADOPT_STEPS: &[&str] = &[
    "validate manifest",
    "guard target",
    "verify service",
    "verify paths",
    "tag as managed",
    "describe",
    "seal env file",
    "record state",
];

/// fix-step-plan-nested: `adopt`'s own step names, qualified the way they
/// are marked when `adopt` runs nested inside another op (`install_native`
/// always ends with one) — the single source both that composer and
/// `admin`'s batch-total use, so neither can drift from what `adopt_impl`
/// actually marks.
pub fn adopt_plan_names(stack_name: &str) -> Vec<String> {
    let prefix = format!("adopt-{}", stack_name);
    ADOPT_STEPS
        .iter()
        .map(|s| format!("{prefix} :: {s}"))
        .collect()
}

/// fix-step-plan-nested: the step logic behind `adopt`, written to run
/// through a `Scope` — as the outermost call (`adopt`, below) or nested
/// inside `install_native`'s own composed plan, sharing its `Runner` and
/// qualifying every mark with `"adopt-<stack>"`.
async fn adopt_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    scoped_step!(scope, "validate manifest", {
        crate::native::validate_native(m)
            .map_err(|p| CoreError::SafetyAbort(format!("native manifest: {}", p.join("; "))))?;
        Ok(StepOutcome::Unchanged)
    });

    scoped_step!(scope, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // The unit must be running AND wired the way the manifest claims:
    // adopting a half-truth would make every later backup and update act on
    // the wrong paths.
    scoped_step!(scope, "verify service", {
        let unit = format!("{}.service", m.unit);
        let active =
            util_pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
        // Exact match: "inactive" and "failed" must not sneak through a
        // suffix check ("inactive".ends_with("active") is true — the test
        // caught exactly that).
        if active.stdout.trim() != "active" {
            return Err(CoreError::SafetyAbort(format!(
                "unit {} is not active ('{}') — adoption never starts services; start it \
                 yourself and re-run",
                unit,
                active.stdout.trim()
            )));
        }
        let show = util_pct_sh(
            exec,
            m.vmid,
            &format!("systemctl show {} -p ExecStart -p EnvironmentFiles", unit),
            30,
        )
        .await?;
        if !show.stdout.contains(&m.binary) {
            return Err(CoreError::SafetyAbort(format!(
                "unit {} does not exec '{}' (systemd says: {}) — fix the stack file to match \
                 reality, not the other way around",
                unit,
                m.binary,
                show.stdout.trim()
            )));
        }
        if let Some(env_file) = &m.env_file
            && !show.stdout.contains(env_file.as_str())
        {
            return Err(CoreError::SafetyAbort(format!(
                "unit {} does not read EnvironmentFile {} — fix the stack file to match \
                     reality",
                unit, env_file
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    scoped_step!(scope, "verify paths", {
        let mut script = format!("test -x {}", shq(&m.binary));
        for d in &m.data_dirs {
            script.push_str(&format!(" && test -d {}", shq(d)));
        }
        if let Some(env_file) = &m.env_file {
            script.push_str(&format!(" && test -f {}", shq(env_file)));
        }
        let out = util_pct_sh(exec, m.vmid, &script, 30).await?;
        if !out.success() {
            return Err(CoreError::SafetyAbort(format!(
                "binary/data/env paths do not all exist in CT {} (checked: {} {:?} {:?})",
                m.vmid, m.binary, m.data_dirs, m.env_file
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // The `homelab` tag is what makes "managed" visible in the Proxmox list.
    // It was applied only where a container is CREATED, so the two adopted
    // ones carried no tag and a filter on it silently missed them — a signal
    // that means "built by the orchestrator" while it reads as "managed by
    // the orchestrator" (Kenny's question, 2026-08-31). Adoption is the other
    // way in, so it sets the tag too. Additive: any tag somebody else put
    // there stays.
    scoped_step!(scope, "tag as managed", {
        let vm = m.vmid.to_string();
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        let tags: Vec<String> = cfg
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("tags:"))
            .map(|v| v.split(';').map(|t| t.trim().to_string()).collect())
            .unwrap_or_default();
        if tags.iter().any(|t| t == "homelab") {
            return Ok(StepOutcome::Unchanged);
        }
        let mut all: Vec<String> = tags.into_iter().filter(|t| !t.is_empty()).collect();
        all.push("homelab".into());
        let joined = all.join(";");
        run_ok(exec, &Cmd::new("pct", &["set", &vm, "--tags", &joined], 30)).await?;
        Ok(StepOutcome::Changed)
    });

    // The description is the tag's fault one field over. `pct set
    // --description` runs where a container is CREATED (deploy.rs), so an
    // adopted container keeps whatever it had — and what CT 109 had was
    // "mailbox 1.0.0 — durable message hub", naming a program that no longer
    // runs and two paths that no longer exist (`/var/lib/mailbox`,
    // `/etc/mailbox/mailbox.env`; the store moved to /appdata on 2026-09-01).
    //
    // Both hand-written descriptions in the fleet had drifted by the time
    // this was measured, which is the argument against fixing them by hand:
    // almanac's still pointed at `/etc/almanac/latch.env` while its service
    // file had moved to `/appdata/almanac/almanac-config/latch.env`. A
    // description nobody derives is a description nobody keeps true.
    //
    // So it says the one thing that stays true — which stack owns this
    // container — and points at the file that carries the detail, instead of
    // copying facts that will move again. Idempotent by construction: every
    // service in a shared container renders the same string (T5), so the
    // three natives on CT 109 do not fight over it.
    scoped_step!(scope, "describe", {
        let vm = m.vmid.to_string();
        let desc = format!(
            "managed by homelab v2 :: stack {} :: native service(s) under systemd. \
             Authoritative detail — units, binaries, data and env paths — lives in \
             stacks/{}/service.yml in the intent repo, not here.",
            m.stack_name, m.stack_name
        );
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        let current = cfg
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("description:"))
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        // pct reports the description percent-encoded; compare on the marker
        // rather than round-tripping the encoding.
        let marker = format!("stack%20{}%20", m.stack_name);
        if current.contains(&marker) || current.contains(&format!("stack {} ", m.stack_name)) {
            return Ok(StepOutcome::Unchanged);
        }
        run_ok(
            exec,
            &Cmd::new("pct", &["set", &vm, "--description", &desc], 30),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // gap-27: an adopted service's env file gets a copy in the host's vault,
    // like a deployed one, so a lost container can get it back. Read without
    // echoing it (fix-39); nothing in the container is written.
    scoped_step!(scope, "seal env file", {
        let Some(env) = &m.env_file else {
            return Ok(StepOutcome::Unchanged);
        };
        let vault = format!(
            "{}/secrets/{}/{}",
            ctx.state_dir,
            m.stack_name,
            crate::ops::deploy::vault_key(env)
        );
        Ok(if seal_one(exec, m.vmid, env, &vault).await? {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    scoped_step!(scope, "record state", {
        let store = crate::state::StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await?;
        if let Some(existing) = state.stacks.get(&m.stack_name)
            && existing.vmid != m.vmid
        {
            return Err(CoreError::SafetyAbort(format!(
                "stack '{}' already exists on vmid {} — refusing to re-point it to {}",
                m.stack_name, existing.vmid, m.vmid
            )));
        }
        // T5: several native services share one container, so adoption adds
        // to the list rather than replacing it. Re-adopting the same unit
        // replaces just that entry — which is how a manifest correction, like
        // the mailbox→kyu rename, lands without disturbing its neighbours.
        let previous = state.stacks.get(&m.stack_name);
        let last_backup = previous.map(|s| s.last_backup).unwrap_or(0);
        let mut natives: Vec<crate::native::NativeServiceManifest> =
            previous.map(|s| s.natives.clone()).unwrap_or_default();
        match natives.iter_mut().find(|n| n.unit == m.unit) {
            Some(slot) => *slot = m.clone(),
            None => natives.push(m.clone()),
        }
        let mut apps: Vec<String> = natives.iter().map(|n| n.unit.clone()).collect();
        apps.sort();
        apps.dedup();
        let fresh = crate::state::StackState {
            applied_source: None,
            vmid: m.vmid,
            hostname: m.hostname.clone(),
            apps,
            applied_at: ctx.now_unix,
            last_backup,
            applied_hash: String::new(),
            manifest: None,
            natives,
            enabled: true,
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        };
        // fix-164: install-native re-adopts every time; a stack a deploy
        // already recorded keeps its manifest, hash, routes, enabled flag and
        // the rest, and only its natives, apps, vmid and hostname change.
        let record = match state.stacks.get(&m.stack_name) {
            Some(prev) => crate::state::StackState {
                vmid: m.vmid,
                hostname: m.hostname.clone(),
                apps: fresh.apps,
                natives: fresh.natives,
                ..prev.clone()
            },
            None => fresh,
        };
        state.stacks.insert(m.stack_name.clone(), record);
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    scope.log(
        Level::Info,
        format!(
            "[adopt] {} ({}) is now managed — service untouched, nightly backup + update \
             supervision from tonight",
            m.stack_name, m.hostname
        ),
    );
    Ok(())
}

pub async fn adopt(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let mut scope = Scope::top(&format!("adopt-{}", m.stack_name), ctx.sink, ctx.journal);
    scope.plan_if_top(ADOPT_STEPS);
    let result = adopt_impl(ctx, m, &mut scope).await;
    scope.finish(result)
}

/// T11: install a native service's binary and unit file into a container
/// the orchestrator has already created.
///
/// This is the half of C7 that never existed. `stacks/kyu/lxc-compose.yml`
/// has said since 2026-08-31 that a rebuild goes "1. this manifest recreates
/// the container; 2. restore puts the data back; 3. the three binaries are
/// installed the way C7 installs them" — and step 3 was a sentence, not a
/// verb. A stack that can only be finished by the person who remembers how
/// it was built is not managed, which is the exact reason that container
/// manifest was written in the first place.
///
/// The container is NOT created here: `homelab deploy` already does that for
/// a `native_only` stack, skipping the docker bootstrap. Duplicating it would
/// give two provisioning paths that drift.
///
/// `binary_b64` is verified by the CLIENT against the release's SHA256SUMS
/// before it is sent, the same way H7 stages the host's own binary — the
/// desktop has the authenticated `gh`, so the private repository never needs
/// a credential on the Proxmox host.
///
/// A first install and a re-install are deliberately the same operation. The
/// difference that matters is whether there is a previous binary to roll back
/// to, and that is a fact about the container, not a flag the caller passes.
/// fix-171 round 2: install-native's own step plan — fixed, every run,
/// because every step here is already unconditional (a precondition that
/// does not hold, like no previous binary to preserve, makes the step a
/// no-op, never an absent one).
pub const INSTALL_STEPS: &[&str] = &[
    "validate manifest",
    "guard target",
    "preserve previous binary",
    "stage binary",
    "check the glibc the binary needs",
    "install unit file",
    "activate",
    "own program directory",
    "keep one previous binary",
];

/// fix-step-plan-nested: `install_native`'s own full plan — its own
/// `INSTALL_STEPS` followed by the `adopt` it always ends with, as a flat
/// sibling (`adopt_plan_names`), never double-prefixed. `own_prefix` is
/// `None` when `install_native` is the outermost call (its own names stay
/// bare, same as before this fix) or `Some("install-<unit>")` when a
/// composing caller (`release_install`) is folding this whole plan into its
/// own — the one source `install_native`'s wrapper and that composer both
/// call, so they cannot drift apart the way the admin dashboard's separate
/// constant-times-units sum once did.
pub fn install_plan_names(m: &NativeServiceManifest, own_prefix: Option<&str>) -> Vec<String> {
    let mut all: Vec<String> = match own_prefix {
        None => INSTALL_STEPS.iter().map(|s| s.to_string()).collect(),
        Some(p) => INSTALL_STEPS
            .iter()
            .map(|s| format!("{p} :: {s}"))
            .collect(),
    };
    all.extend(adopt_plan_names(&m.stack_name));
    all
}

/// fix-step-plan-nested: the step logic behind `install_native`, written to
/// run through a `Scope` — as the outermost call or nested inside
/// `release_install`'s own composed plan.
#[allow(clippy::too_many_arguments)]
async fn install_native_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    binary_b64: &str,
    unit_file: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let unit = format!("{}.service", m.unit);
    let prev = format!("{}.homelab-prev", m.binary);
    let staged = format!("{}.homelab-new", m.binary);
    let unit_path = format!("/etc/systemd/system/{}", unit);

    scoped_step!(scope, "validate manifest", {
        crate::native::validate_native(m)
            .map_err(|p| CoreError::SafetyAbort(format!("native manifest: {}", p.join("; "))))?;
        if unit_file.trim().is_empty() {
            return Err(CoreError::Validation(format!(
                "no unit file for {} — the file that makes the service exist is not in the \
                 repository, and installing a binary without it produces a container with a \
                 program on it and nothing to run it",
                m.unit
            )));
        }
        if !unit_file.contains(&m.binary) {
            return Err(CoreError::Validation(format!(
                "the unit file does not exec '{}' — the same mismatch adoption refuses, caught \
                 before it is written rather than after",
                m.binary
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    scoped_step!(scope, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // Whether there is something to fall back to is discovered, not assumed.
    let mut had_previous = false;
    scoped_step!(scope, "preserve previous binary", {
        let probe = util_pct_sh(
            exec,
            m.vmid,
            &format!("test -f {} && echo yes || echo no", shq(&m.binary)),
            30,
        )
        .await?;
        had_previous = probe.stdout.trim() == "yes";
        if !had_previous {
            return Ok(StepOutcome::Unchanged);
        }
        let out = util_pct_sh(exec, m.vmid, &preserve_script(&m.binary, &prev), 120).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "cannot preserve the running {} — refusing to replace a binary with no way back",
                m.binary
            )));
        }
        Ok(StepOutcome::Changed)
    });

    // The binary travels as base64 text because that is what `pct push`
    // carries reliably; it is decoded on the container. The decoded file is
    // staged BESIDE the target rather than over it, so a transfer that dies
    // half way leaves the running service on its own binary.
    scoped_step!(scope, "stage binary", {
        let b64_path = format!("{}.b64", staged);
        crate::ops::util::push_content(exec, m.vmid, &b64_path, binary_b64, "600").await?;
        let script = format!(
            "base64 -d {b64} > {new} && chmod 755 {new} && rm -f {b64} && test -s {new}",
            b64 = shq(&b64_path),
            new = shq(&staged)
        );
        let out = util_pct_sh(exec, m.vmid, &script, 300).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "could not decode the binary into {} ({}) — nothing was replaced",
                staged,
                out.stderr.trim()
            )));
        }
        Ok(StepOutcome::Changed)
    });

    // T87: the one reading that tells a crash loop from a working install,
    // taken while the running service is still untouched. On refusal the
    // staged file goes too, so a retry starts clean and nothing on the
    // container looks half-installed.
    let mut glibc_note = String::new();
    scoped_step!(scope, "check the glibc the binary needs", {
        let out = util_pct_sh(exec, m.vmid, &glibc_probe_script(&staged), 60).await?;
        match glibc_verdict(&out.stdout) {
            Ok(fine) => {
                glibc_note = fine;
                Ok(StepOutcome::Unchanged)
            }
            Err(why) => {
                let _ = util_pct_sh(exec, m.vmid, &format!("rm -f {}", shq(&staged)), 30).await;
                // fix-114: nothing is installed, so the kept previous binary
                // from before this run is the kept one again.
                if had_previous {
                    let _ = util_pct_sh(exec, m.vmid, &restore_set_aside_script(&prev), 30).await;
                }
                Err(CoreError::SafetyAbort(format!(
                    "{} :: the staged copy was removed; the running {} is untouched",
                    why, m.unit
                )))
            }
        }
    });

    scope.log(Level::Info, format!("[install] {}", glibc_note));

    scoped_step!(scope, "install unit file", {
        crate::ops::util::push_content(exec, m.vmid, &unit_path, unit_file, "644").await?;
        let out = util_pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "systemctl daemon-reload failed: {}",
                out.stderr.trim()
            )));
        }
        Ok(StepOutcome::Changed)
    });

    scoped_step!(scope, "activate", {
        // Stopping first is deliberate: replacing the file under a running
        // process leaves the old one mapped, so the service keeps running the
        // version that was just replaced and every reading afterwards lies.
        let script = format!(
            "systemctl stop {u} 2>/dev/null; mv -f {new} {bin} && \
             systemctl enable {u} >/dev/null 2>&1 && systemctl start {u} && \
             for i in 1 2 3 4 5; do \
               [ \"$(systemctl is-active {u})\" = active ] && exit 0; sleep 2; done; exit 1",
            u = unit,
            new = shq(&staged),
            bin = shq(&m.binary)
        );
        let out = util_pct_sh(exec, m.vmid, &script, 180).await?;
        if out.success() {
            return Ok(StepOutcome::Changed);
        }
        if !had_previous {
            // Nothing to roll back to, and saying so plainly matters: a
            // rollback message here would claim a safety net that does not
            // exist. The unit is left stopped rather than restart-looping.
            let _ = util_pct_sh(exec, m.vmid, &format!("systemctl stop {}", unit), 60).await;
            let log = util_pct_sh(
                exec,
                m.vmid,
                &format!("journalctl -u {} -n 20 --no-pager 2>&1 | tail -20", unit),
                60,
            )
            .await?;
            return Err(CoreError::Other(format!(
                "{} did not come up and there is no previous binary to return to — this was a \
                 FIRST install, so the container now has the program and no working service. \
                 Last log lines: {}",
                m.unit,
                log.stdout.trim()
            )));
        }
        let rollback = format!(
            "cp -p {prev} {bin} && systemctl restart {u} && sleep 2 && \
             [ \"$(systemctl is-active {u})\" = active ]",
            prev = shq(&prev),
            bin = shq(&m.binary),
            u = unit
        );
        let rb = util_pct_sh(exec, m.vmid, &rollback, 180).await?;
        let _ = util_pct_sh(exec, m.vmid, &restore_set_aside_script(&prev), 60).await;
        Err(CoreError::Other(format!(
            "the installed {} did not come up healthy — rolled back to the previous binary ({})",
            m.unit,
            if rb.success() {
                "service restored and active"
            } else {
                "ROLLBACK ALSO FAILED — service needs hands NOW"
            }
        )))
    });

    let mut stale_kept = false;
    scoped_step!(scope, "own program directory", {
        let Some(user) = unit_user(unit_file) else {
            // A unit with no User= runs as root and already owns everything.
            return Ok(StepOutcome::Unchanged);
        };
        let out = util_pct_sh(exec, m.vmid, &own_program_dir_script(&user, &m.binary), 120).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "{} is installed and running but its program directory is still root's ({}) — \
                 the service cannot update itself from here",
                m.unit,
                out.stderr.trim()
            )));
        }
        Ok(StepOutcome::Changed)
    });

    // fix-114: keep exactly one previous binary, as a supervised update does.
    scoped_step!(scope, "keep one previous binary", {
        if !had_previous {
            return Ok(StepOutcome::Unchanged);
        }
        let out = util_pct_sh(
            exec,
            m.vmid,
            &keep_one_previous_script(&m.binary, &prev),
            60,
        )
        .await?;
        if !out.success() {
            // Not fatal: the service is up and correct, this only leaves a
            // copy on disk. Reported after the step rather than failing a
            // good install over it.
            stale_kept = true;
            return Ok(StepOutcome::Unchanged);
        }
        Ok(StepOutcome::Changed)
    });

    if stale_kept {
        scope.log(
            Level::Warn,
            format!(
                "could not settle the kept previous binary at {} — check it and {}.old by hand",
                prev, prev
            ),
        );
    }

    // Installed and healthy: the same record adoption writes, so a service
    // built this way and one taken over by hand are indistinguishable
    // afterwards — which is the point. fix-step-plan-nested: nested through
    // THIS scope (flattened — `adopt_impl`'s marks land on whichever
    // `Runner` this call chain is ultimately `Top` for, qualified
    // "adopt-<stack> :: …", a sibling of "install-<unit> :: …" rather than
    // compounded under it), not a second standalone op with its own plan.
    let mut adopt_scope = scope.child(format!("adopt-{}", m.stack_name));
    adopt_impl(ctx, m, &mut adopt_scope).await?;

    scope.log(
        Level::Info,
        format!(
            "[install] {} on {} — binary installed, unit active, stack recorded",
            m.unit, m.hostname
        ),
    );
    Ok(())
}

pub async fn install_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    binary_b64: &str,
    unit_file: &str,
) -> OperationReport {
    let mut scope = Scope::top(&format!("install-{}", m.unit), ctx.sink, ctx.journal);
    scope.plan_if_top(&install_plan_names(m, None));
    let result = install_native_impl(ctx, m, binary_b64, unit_file, &mut scope).await;
    scope.finish(result)
}

/// fix-146 (native-empty-rebuild, Kenny 2026-09-27, form "Keuzes helpers"):
/// what the deploy found about a native unit's data before starting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmptyUnit {
    /// Its data directories hold something: nothing to do.
    HasData,
    /// Empty, and its repository has no snapshot (or does not exist): a new
    /// service.
    Fresh,
    /// Empty; the newest snapshot was unpacked back into the container.
    Restored,
    /// Unpacked, but the unit must not start yet: why.
    RestoredHold(String),
    /// Empty, and whether a snapshot exists could not be read: why.
    CheckFailed(String),
    /// Empty with history, and the restore failed: why.
    RestoreFailed(String),
}

/// fix-146: a rebuilt container starts its native units with empty data
/// directories until somebody restores them by hand (op-11), while a compose
/// stack's empty directories are refilled on deploy (E3). The unit then
/// starts empty and fix-63 has to refuse that night's backup. Now the deploy
/// asks first: empty, with history, means the newest snapshot is unpacked
/// back into the container — the op-11 procedure, `restic dump` piped into
/// `tar` inside the container so owners stay the container's own — BEFORE
/// the unit starts.
///
/// A unit archived from its own copy (`backup_from_newest`, kyu) gets that
/// copy back, not a live store: the copy has to become the live file by hand
/// (the stack file's restore note names it), so that unit is left stopped.
pub async fn restore_empty_unit(
    exec: &dyn Executor,
    cfg: &crate::ops::backup::BackupCfg,
    m: &NativeServiceManifest,
) -> Result<EmptyUnit, CoreError> {
    if m.stateless || m.data_dirs.is_empty() {
        return Ok(EmptyUnit::HasData);
    }
    let dirs = m
        .data_dirs
        .iter()
        .map(|d| shq(d))
        .collect::<Vec<_>>()
        .join(" ");
    let probe = util_pct_sh(
        exec,
        m.vmid,
        &format!("find {} -mindepth 1 2>/dev/null | head -1", dirs),
        60,
    )
    .await?;
    if !probe.stdout.trim().is_empty() {
        return Ok(EmptyUnit::HasData);
    }
    let listing = exec
        .run(&crate::ops::backup::restic_cmd(
            cfg,
            &m.unit,
            &["snapshots", "--json"],
            120,
        ))
        .await;
    // fix-54's rule: only an empty list and restic's "repository does not
    // exist" (exit 10) mean there is nothing to restore.
    let history = match listing {
        Ok(out) if out.success() => crate::ops::backup::parse_snapshots_json(&out.stdout),
        Ok(out) if out.code == 10 => Vec::new(),
        Ok(out) => {
            return Ok(EmptyUnit::CheckFailed(format!(
                "rc={} :: {}",
                out.code,
                crate::executor::trace_line(out.stderr.trim())
            )));
        }
        Err(e) => return Ok(EmptyUnit::CheckFailed(e.to_string())),
    };
    if history.is_empty() {
        return Ok(EmptyUnit::Fresh);
    }
    let script = format!(
        "set -o pipefail; env RESTIC_REPOSITORY={base}/{unit}-config RESTIC_PASSWORD_FILE={pw} \
         RESTIC_CACHE_DIR={cache} restic dump latest /{unit}-data.tar | \
         pct exec {vmid} -- tar -xf - -C /",
        base = cfg.restic_base,
        unit = m.unit,
        pw = cfg.password_file,
        cache = crate::ops::backup::RESTIC_CACHE_DIR,
        vmid = m.vmid
    );
    let out = exec
        .run(&Cmd::new("sh", &["-c", &script], cfg.restore_timeout_s))
        .await;
    match out {
        Ok(o) if o.success() => {}
        Ok(o) => {
            return Ok(EmptyUnit::RestoreFailed(format!(
                "rc={} :: {}",
                o.code,
                crate::executor::trace_line(o.stderr.trim())
            )));
        }
        Err(e) => return Ok(EmptyUnit::RestoreFailed(e.to_string())),
    }
    // fix-146: `after_restore` is the generic form of what used to be a
    // manual step (op-11) — a command that turns whatever the snapshot
    // unpacked into the live store this unit expects. Run it BEFORE the
    // caller is told the unit may start; a failure here must hold the unit
    // back exactly like a failed unpack, because starting it now would run
    // the service against data the command never finished seeding.
    if let Some(cmd) = &m.after_restore {
        let out = util_pct_sh(exec, m.vmid, cmd, 300).await;
        match out {
            Ok(o) if o.success() => {}
            Ok(o) => {
                return Ok(EmptyUnit::RestoreFailed(format!(
                    "after_restore failed, rc={} :: {}",
                    o.code,
                    crate::executor::trace_line(o.stderr.trim())
                )));
            }
            Err(e) => {
                return Ok(EmptyUnit::RestoreFailed(format!(
                    "after_restore failed: {}",
                    e
                )));
            }
        }
        return Ok(EmptyUnit::Restored);
    }
    if let Some(glob) = &m.backup_from_newest {
        return Ok(EmptyUnit::RestoredHold(format!(
            "its archive is the service's own copy ({}), not a live store, and this service \
             declares no after_restore to seed it: put the newest copy in place as the live \
             file as the stack file's restore note says (docs/OPERATIONS_RUNBOOK.md op-11), \
             then start it",
            glob
        )));
    }
    Ok(EmptyUnit::Restored)
}

/// fix-113 (owner decision, 2026-10-01): what `backup_native` does after
/// running `<binary> backup-pause --for <secs>` in `BackupPause::Chassis`
/// mode, decided from its exit code (chassis-rs commit 722249b) and, only
/// for exit 3, whether the unit is active — a pure function so each branch
/// is a test, not a live run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChassisPauseOutcome {
    /// Exit 0: the binary itself quiesced the service (a listener answered,
    /// or it stopped the unit, or the unit was not running) — its stdout
    /// first word says which. Resume afterwards with `backup-resume`.
    Paused(String),
    /// Exit 2 (a binary built before `backup-pause` existed: clap's own
    /// "unrecognized subcommand") or exit 3 with the unit active (no
    /// listener and the binary could not stop it either): fall back to the
    /// `BackupPause::Unit` mechanism — the homelab stops the unit itself.
    FallBackToStop,
    /// Exit 3 with the unit not active: nothing is running, so there is
    /// nothing to pause and nothing to resume.
    NothingRunning,
    /// Exit 1, or any code outside 0..=3: fail loudly — the unit started
    /// again on its own (exit 1) or the binary said something this build
    /// does not understand. Nothing was archived.
    Failed(String),
}

/// See [`ChassisPauseOutcome`]. `unit_active` is read only for exit 3 — the
/// one code whose meaning depends on it — so a caller need not probe the
/// unit for the other three.
pub fn decide_chassis_pause(
    exit_code: i32,
    stdout: &str,
    unit_active: bool,
) -> ChassisPauseOutcome {
    let stdout = stdout.trim();
    match exit_code {
        0 => ChassisPauseOutcome::Paused(stdout.to_string()),
        2 => ChassisPauseOutcome::FallBackToStop,
        3 => {
            if unit_active {
                ChassisPauseOutcome::FallBackToStop
            } else {
                ChassisPauseOutcome::NothingRunning
            }
        }
        1 => ChassisPauseOutcome::Failed(format!(
            "backup-pause failed (exit 1) — the unit started again on its own: {}",
            stdout
        )),
        other => ChassisPauseOutcome::Failed(format!(
            "backup-pause exited {} (expected 0, 1, 2 or 3): {}",
            other, stdout
        )),
    }
}

/// feat-backup-2: `restore_empty_unit`'s pipeline, gated and chosen rather
/// than automatic-and-always-latest: the operator picks a snapshot from the
/// Backups page and confirms by typing the stack's name (the same gate
/// `backup::restore_confirmed` gives the compose path). Unlike
/// `restore_empty_unit` this stops the unit first (a live restore under a
/// running service would interleave the unpack with its own writes) and
/// takes a safety copy of the current data directories on the HOST first,
/// mirroring the compose path's `pre_restore_dir` — so a restore that turns
/// out wrong is not the second data loss of the day.
pub async fn restore_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    cfg: &crate::ops::backup::BackupCfg,
    snapshot: &str,
    confirm: Option<&str>,
) -> OperationReport {
    let op = format!("restore-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    // fix-171 round 3: known from the manifest alone, before anything runs —
    // a unit that declares no data never gets past "safety gates", and
    // `after_restore` is a fixed field of the manifest, not a runtime
    // decision.
    let mut steps: Vec<&str> = vec!["safety gates"];
    if !(m.stateless || m.data_dirs.is_empty()) {
        steps.push("stop unit");
        steps.push("safety copy");
        steps.push("unpack snapshot");
        if m.after_restore.is_some() {
            steps.push("re-seed from restore");
        }
        steps.push("start unit");
    }
    runner.plan(&steps);

    runner.log(
        Level::Warn,
        format!(
            "[restore] {} (native unit {}) from snapshot '{}'",
            m.stack_name, m.unit, snapshot
        ),
    );

    step!(runner, "safety gates", {
        crate::ops::backup::restore_confirmed(&m.stack_name, confirm)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    if m.stateless || m.data_dirs.is_empty() {
        runner.log(
            Level::Info,
            format!("[restore] {} declares no data — nothing to restore", m.unit),
        );
        return runner.finish_ok();
    }

    step!(runner, "stop unit", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!("systemctl stop {}", shq(&m.unit)),
            60,
        )
        .await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: format!("systemctl stop {}", m.unit),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(StepOutcome::Changed)
    });

    // fix-64's rule, repeated for the native path: the data about to be
    // overwritten is copied aside on the HOST before anything is unpacked
    // over it, into the same `pre-restore/` area the compose restore uses.
    let safety_copy_dest = format!(
        "{}/pre-restore/{}-{}",
        ctx.state_dir, m.stack_name, ctx.now_unix
    );
    step!(runner, "safety copy", {
        let dest = safety_copy_dest.clone();
        let dirs = m
            .data_dirs
            .iter()
            .map(|d| shq(d))
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "mkdir -p {dest} && pct exec {vmid} -- tar -cf - {dirs} | tar -xf - -C {dest}",
            dest = shq(&dest),
            vmid = m.vmid,
            dirs = dirs,
        );
        let out = exec.run(&Cmd::new("sh", &["-c", &script], 1800)).await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: "safety copy of the current data".into(),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(StepOutcome::Changed)
    });
    runner.log(
        Level::Info,
        format!(
            "[restore] current data copied aside to {}",
            safety_copy_dest
        ),
    );

    step!(runner, "unpack snapshot", {
        let script = format!(
            "set -o pipefail; env RESTIC_REPOSITORY={base}/{unit}-config \
             RESTIC_PASSWORD_FILE={pw} RESTIC_CACHE_DIR={cache} restic dump {snap} \
             /{unit}-data.tar | pct exec {vmid} -- tar -xf - -C /",
            base = cfg.restic_base,
            unit = m.unit,
            pw = cfg.password_file,
            cache = crate::ops::backup::RESTIC_CACHE_DIR,
            snap = snapshot,
            vmid = m.vmid,
        );
        let out = exec
            .run(&Cmd::new("sh", &["-c", &script], cfg.restore_timeout_s))
            .await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: format!("restic dump {} | tar -x", snapshot),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(StepOutcome::Changed)
    });

    // fix-146: the same re-seed step the automatic empty-rebuild path runs
    // (`restore_empty_unit`) — a unit whose archive is not already its live
    // store (`backup_from_newest`, kyu) needs this before it is safe to
    // start, and a hand-triggered restore from the dashboard is exactly as
    // much "a restore" as the automatic one.
    if let Some(cmd) = &m.after_restore {
        step!(runner, "re-seed from restore", {
            let out = util_pct_sh(exec, m.vmid, cmd, 300).await?;
            if !out.success() {
                return Err(CoreError::Command {
                    rendered: "after_restore".into(),
                    detail: out.stderr.trim().to_string(),
                });
            }
            Ok(StepOutcome::Changed)
        });
    }

    step!(runner, "start unit", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!("systemctl start {}", shq(&m.unit)),
            60,
        )
        .await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: format!("systemctl start {}", m.unit),
                detail: out.stderr.trim().to_string(),
            });
        }
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!("[restore] {} restored and restarted", m.unit),
    );
    runner.finish_ok()
}

/// C7 nightly backup for a native stack. The data lives INSIDE the
/// container (adoption never restarts a service, so a bind-mount to
/// /appdata was never an option); the snapshot therefore streams
/// `pct exec tar` straight into `restic --stdin` — one host-side pipeline,
/// nothing written in between. Repo naming and tiered retention are the
/// same as every compose stack: `<base>/<unit>-config` (D25: named after the
/// service, not the stack).
pub async fn backup_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    cfg: &crate::ops::backup::BackupCfg,
) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    // fix-115: a unit that declares it keeps nothing has nothing to archive.
    // Story: docs/deployment/REGISTER.md.
    if m.stateless && m.data_dirs.is_empty() && m.backup_from_newest.is_none() {
        runner.log(
            Level::Info,
            format!(
                "[backup] {} is stateless — nothing to archive, no repository",
                m.unit
            ),
        );
        return runner.finish_ok();
    }

    // fix-171 round 3: the pause step's own name, and whether a resume step
    // can ever follow it, are both decided by `m.backup_pause` — a fixed
    // field of the manifest, known before anything runs. Whether "resume
    // the service" actually fires (Chassis can find nothing running to
    // pause) is a live decision, so a run that skips it still marks it.
    let mut steps: Vec<&str> = vec![
        "guard target",
        "init repo",
        "find the service's own newest copy",
        "empty data over history",
        "clear stale locks",
    ];
    match m.backup_pause {
        BackupPause::Off => {}
        BackupPause::Unit => steps.push("pause the service"),
        BackupPause::Chassis => steps.push("pause the service (chassis)"),
    }
    steps.push("snapshot");
    if !matches!(m.backup_pause, BackupPause::Off) {
        steps.push("resume the service");
    }
    steps.push("retention");
    runner.plan(&steps);

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "init repo", {
        // gap-24: the answer is read, as for compose stacks and host-meta.
        let init = crate::ops::backup::restic_cmd(cfg, &m.unit, &["init"], 120);
        let outcome = crate::ops::backup::init_repository(ctx.exec, &init, &m.unit).await?;
        if outcome == crate::ops::backup::InitOutcome::Created {
            crate::ops::backup::same_password_as_host_meta(
                ctx.exec,
                &cfg.restic_base,
                &cfg.password_file,
                &m.unit,
            )
            .await?;
            return Ok(StepOutcome::Changed);
        }
        Ok(StepOutcome::Unchanged)
    });

    // T77 (D94): a service that makes its own verified copy is archived from
    // that copy, and only when it is fresh. The age is measured against the
    // container's clock, in the same command that finds the file, so a copy
    // from a run that silently stopped cannot pass as tonight's.
    let mut own_copy: Option<String> = None;
    step!(runner, "find the service's own newest copy", {
        let Some(glob) = &m.backup_from_newest else {
            return Ok(StepOutcome::Unchanged);
        };
        let script = format!(
            "f=$(ls -1t {glob} 2>/dev/null | head -1); [ -n \"$f\" ] || exit 3; \
             echo \"$f $(( $(date +%s) - $(stat -c %Y \"$f\") ))\"",
            glob = glob
        );
        let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
        let line = out.stdout.trim().to_string();
        let (file, age) = match line.rsplit_once(' ') {
            Some((f, a)) if out.success() && !f.is_empty() => {
                (f.to_string(), a.parse::<u64>().unwrap_or(u64::MAX))
            }
            _ => {
                return Err(CoreError::Other(format!(
                    "no copy matches {} on {} — the service's own backup has not produced \
                     one; refusing to archive nothing and call it a backup",
                    glob, m.hostname
                )));
            }
        };
        if age > MAX_OWN_COPY_AGE_S {
            return Err(CoreError::Other(format!(
                "the newest copy {} is {} h old (limit {} h) — the service's own backup did \
                 not run; archiving a stale copy would look exactly like success (M-D94)",
                file,
                age / 3600,
                MAX_OWN_COPY_AGE_S / 3600
            )));
        }
        own_copy = Some(file);
        Ok(StepOutcome::Unchanged)
    });
    if let Some(f) = &own_copy {
        runner.log(
            Level::Info,
            format!("[backup] {} archives its own copy {}", m.stack_name, f),
        );
    }

    // fix-63: empty directories are archived only into a repository without
    // history — a new service — and refused over one that has some, so an
    // empty rebuild can never become the latest snapshot of a service with
    // real history. Story: docs/deployment/REGISTER.md.
    step!(runner, "empty data over history", {
        if own_copy.is_some() || m.data_dirs.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let dirs = m
            .data_dirs
            .iter()
            .map(|d| shq(d))
            .collect::<Vec<_>>()
            .join(" ");
        let probe = util_pct_sh(
            exec,
            m.vmid,
            &format!("find {} -type f 2>/dev/null | head -1", dirs),
            120,
        )
        .await?;
        if !probe.stdout.trim().is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let listing = crate::executor::run_ok(
            exec,
            &crate::ops::backup::restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 300),
        )
        .await?;
        let history = crate::ops::backup::parse_snapshots_json(&listing.stdout);
        let Some(newest) = history.iter().map(|(_, t)| *t).max() else {
            return Ok(StepOutcome::Unchanged);
        };
        Err(CoreError::SafetyAbort(format!(
            "{} holds no files on {} while {}-config has {} snapshot(s), the newest from {} :: \
             archiving it would make an empty state the latest snapshot and start pruning the \
             real history. This is what a rebuilt container looks like before its data is \
             put back: restore it first (docs/OPERATIONS_RUNBOOK.md op-11), and the next \
             backup runs",
            m.data_dirs.join(", "),
            m.hostname,
            m.unit,
            history.len(),
            crate::state::ymd(newest)
        )))
    });

    // fix-113 (native-tar-no-quiesce, 2026-09-27): a run killed mid-snapshot
    // leaves a stale lock, as on the compose path; restic only removes locks
    // of processes that are gone, so this is always safe.
    step!(runner, "clear stale locks", {
        let _ = exec
            .run(&crate::ops::backup::restic_cmd(
                cfg,
                &m.unit,
                &["unlock"],
                120,
            ))
            .await;
        Ok(StepOutcome::Unchanged)
    });

    // fix-113: a service declared `backup_pause` is quiesced for the tar, so
    // its store is not archived mid-write. Stopping first and failing loudly
    // when it will not stop: a tar of a store still being written is the
    // torn backup this exists to prevent.
    let unit = format!("{}.service", m.unit);
    // Whether the resume phase has anything to do: `Unit` always does (it
    // always stopped the unit); `Chassis` does whenever the pause phase
    // actually quiesced or stopped something (`Paused` or the stop
    // fallback) — not when nothing was running to begin with, which is the
    // one case with nothing to put back.
    let mut needs_resume = false;
    // `Chassis` / `Paused` only: the binary itself is what quiesced things,
    // so it is also what un-quiesces them, ahead of the homelab's own
    // "is it active" check. The stop fallback below is symmetrical with
    // `Unit` instead — the homelab did its own `systemctl stop`, so its own
    // `systemctl start` undoes it, the same resume `Unit` always used.
    let mut chassis_resume = false;
    // Set inside the step closures (which may not call `runner.log`
    // themselves — logging here, after `step!` returns, is the pattern the
    // rest of this function already uses for `own_copy`).
    let mut pause_note: Option<String> = None;
    // fix-113 ADDENDUM: where the LOCAL staged copy landed, when staging was
    // used — set only inside `BackupPause::Chassis`'s `Paused` arm. Its
    // presence is what tells the "snapshot" step to read a file instead of
    // the container, and the final cleanup to remove it; its absence with
    // `chassis_resume` true is what tells "snapshot" to hold the pause open
    // (renewed) across the tar-to-restic pipe itself.
    let mut staged_file: Option<String> = None;
    // fix-113: whatever the "resume the service" step below logs — set
    // there, or (ADDENDUM) right after a staged local copy, which resumes
    // long before that step runs.
    let mut resume_note: Option<String> = None;
    match m.backup_pause {
        BackupPause::Off => {}
        BackupPause::Unit => {
            step!(runner, "pause the service", {
                let out =
                    util_pct_sh(exec, m.vmid, &format!("systemctl stop {}", unit), 120).await?;
                if !out.success() {
                    return Err(CoreError::Other(format!(
                        "{} would not stop for its backup ({}) — nothing was archived",
                        unit,
                        out.stderr.trim()
                    )));
                }
                Ok(StepOutcome::Changed)
            });
            needs_resume = true;
        }
        BackupPause::Chassis => {
            step!(runner, "pause the service (chassis)", {
                let out = util_pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "{} backup-pause --for {}",
                        shq(&m.binary),
                        CHASSIS_PAUSE_FOR_S
                    ),
                    60,
                )
                .await?;
                // Exit 3 ("impossible") only decides anything once we also
                // know whether the unit is still active.
                let unit_active = if out.code == 3 {
                    let probe =
                        util_pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30)
                            .await?;
                    probe.stdout.trim() == "active"
                } else {
                    false
                };
                match decide_chassis_pause(out.code, &out.stdout, unit_active) {
                    ChassisPauseOutcome::Paused(mode) => {
                        pause_note = Some(format!("backup-pause: {}", mode));
                        // fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01):
                        // the pause is held open, renewed, only for a LOCAL
                        // copy — never for restic's own upload. When a
                        // staging directory is configured and the copy is
                        // estimated to fit it (`fits_staging`), tar it there
                        // under the renewed pause, resume the SERVICE the
                        // moment that local copy is done (right here, not
                        // after the possibly-slow upload), and leave
                        // `staged_file` for the "snapshot" step below to read
                        // from instead of the container.
                        if let Some(dir) = &cfg.staging_dir {
                            let _ = exec
                                .run(&Cmd::new(
                                    "sh",
                                    &["-c", &format!("mkdir -p {}", shq(dir))],
                                    30,
                                ))
                                .await;
                            let stage_path = format!("{}/{}-stage.tar", dir, m.unit);
                            // rule 20: a leftover from a run that died
                            // between writing this file and deleting it
                            // again, cleared at the start of THIS run before
                            // the free-space reading below is trusted.
                            remove_staging_file(exec, &stage_path).await;
                            let estimate = estimate_data_bytes(exec, m.vmid, &m.data_dirs).await;
                            let free = staging_free_bytes(exec, dir).await;
                            let use_staging = matches!(
                                (estimate, free),
                                (Some(e), Some(f)) if fits_staging(e, f, cfg.staging_cap_mib)
                            );
                            if use_staging {
                                let dirs = m
                                    .data_dirs
                                    .iter()
                                    .map(|d| shq(d))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                let tar_script = format!(
                                    "set -o pipefail; pct exec {} -- tar -cf - {} > {}",
                                    m.vmid,
                                    dirs,
                                    shq(&stage_path)
                                );
                                let tar_cmd =
                                    Cmd::new("sh", &["-c", &tar_script], cfg.snapshot_timeout_s);
                                let tar_result =
                                    run_under_chassis_heartbeat(exec, m.vmid, &m.binary, &tar_cmd)
                                        .await;
                                // Resume before anything else, success or
                                // failure — a torn local copy must not leave
                                // the service paused while this run decides
                                // what to do about it.
                                let resume_out = util_pct_sh(
                                    exec,
                                    m.vmid,
                                    &format!("{} backup-resume", shq(&m.binary)),
                                    60,
                                )
                                .await;
                                match tar_result {
                                    Ok(_) => {
                                        resume_note = resume_out
                                            .as_ref()
                                            .ok()
                                            .map(|o| format!("backup-resume: {}", o.stdout.trim()));
                                        staged_file = Some(stage_path);
                                        return Ok(StepOutcome::Changed);
                                    }
                                    Err(e) => {
                                        remove_staging_file(exec, &stage_path).await;
                                        return Err(CoreError::Other(format!(
                                            "{} local staging copy failed ({}) — resume: \
                                             {:?}; nothing was archived",
                                            unit,
                                            e,
                                            resume_out.map(|o| o.stdout)
                                        )));
                                    }
                                }
                            }
                        }
                        // No staging dir configured, or the copy did not
                        // fit: back up live under the renewed pause (the
                        // "snapshot" step below renews it across the whole
                        // tar-to-restic pipe).
                        needs_resume = true;
                        chassis_resume = true;
                        Ok(StepOutcome::Changed)
                    }
                    ChassisPauseOutcome::FallBackToStop => {
                        pause_note = Some(format!(
                            "backup-pause could not quiesce it (rc={}) — falling back to \
                             stopping the unit",
                            out.code
                        ));
                        let stop =
                            util_pct_sh(exec, m.vmid, &format!("systemctl stop {}", unit), 120)
                                .await?;
                        if !stop.success() {
                            return Err(CoreError::Other(format!(
                                "{} would not stop for its backup ({}) — nothing was archived",
                                unit,
                                stop.stderr.trim()
                            )));
                        }
                        needs_resume = true;
                        Ok(StepOutcome::Changed)
                    }
                    ChassisPauseOutcome::NothingRunning => {
                        pause_note = Some("backup-pause: not running, nothing to pause".into());
                        Ok(StepOutcome::Unchanged)
                    }
                    ChassisPauseOutcome::Failed(why) => Err(CoreError::Other(format!(
                        "{} backup-pause: {} — nothing was archived",
                        unit, why
                    ))),
                }
            });
        }
    }
    if let Some(note) = &pause_note {
        runner.log(Level::Info, format!("[backup] {} {}", unit, note));
    }

    let snapshot_result = runner
        .step("snapshot", || async {
            // fix-113 ADDENDUM: a staged local copy already landed on this
            // host (and the service is already resumed) — read it instead of
            // the container, so this step's own duration (restic's upload)
            // never touches the pause.
            if let Some(stage_path) = &staged_file {
                let script = format!(
                    "set -o pipefail; cat {} | \
                 env RESTIC_REPOSITORY={}/{}-config RESTIC_PASSWORD_FILE={} \
                 RESTIC_CACHE_DIR={} \
                 restic backup --stdin --stdin-filename {}-data.tar --tag {}",
                    shq(stage_path),
                    cfg.restic_base,
                    m.unit,
                    cfg.password_file,
                    crate::ops::backup::RESTIC_CACHE_DIR,
                    m.unit,
                    cfg.trigger.tag()
                );
                crate::executor::run_ok(
                    exec,
                    &Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s),
                )
                .await?;
                return Ok(StepOutcome::Changed);
            }
            let dirs = match &own_copy {
                Some(f) => shq(f),
                None => m
                    .data_dirs
                    .iter()
                    .map(|d| shq(d))
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            // pipefail is load-bearing: without it a dead `pct exec tar` still
            // yields a "successful" empty snapshot — a backup that lies.
            // F171: the same restic cache constant `backup.rs` uses for every
            // compose stack, so this hand-built environment does not drift
            // from it. Story: docs/deployment/REGISTER.md.
            let script = format!(
                "set -o pipefail; pct exec {} -- tar -cf - {} | \
             env RESTIC_REPOSITORY={}/{}-config RESTIC_PASSWORD_FILE={} \
             RESTIC_CACHE_DIR={} \
             restic backup --stdin --stdin-filename {}-data.tar --tag {}",
                // D25: named after the SERVICE, not the stack. T5 puts several
                // services on one container, and a per-stack repository would
                // fold them into one — so moving any of them elsewhere would
                // leave its history behind, which is what D25 exists to prevent.
                m.vmid,
                dirs,
                cfg.restic_base,
                m.unit,
                cfg.password_file,
                crate::ops::backup::RESTIC_CACHE_DIR,
                m.unit,
                cfg.trigger.tag()
            );
            let cmd = Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s);
            if chassis_resume {
                // fix-113 ADDENDUM: no staging (disabled, or the copy did
                // not fit) — the pause is held open and renewed across the
                // tar AND the restic upload together, since there is no
                // separate local phase to shorten it with.
                run_under_chassis_heartbeat(exec, m.vmid, &m.binary, &cmd).await?;
            } else {
                crate::executor::run_ok(exec, &cmd).await?;
            }
            Ok(StepOutcome::Changed)
        })
        .await;

    // fix-113 ADDENDUM: a staged tar must not survive the run (rule 20) —
    // removed here regardless of whether the snapshot step above succeeded.
    if let Some(stage_path) = &staged_file {
        remove_staging_file(exec, stage_path).await;
    }

    // fix-113: the paused service is started again whatever the snapshot
    // did, as `backup` resumes what it quiesced: a backup that leaves a
    // service off is worse than one that fails.
    if needs_resume {
        step!(runner, "resume the service", {
            if chassis_resume {
                let out = util_pct_sh(
                    exec,
                    m.vmid,
                    &format!("{} backup-resume", shq(&m.binary)),
                    60,
                )
                .await?;
                resume_note = Some(format!("backup-resume: {}", out.stdout.trim()));
            }
            let out = util_pct_sh(
                exec,
                m.vmid,
                &format!(
                    "systemctl start {u}; sleep 2; [ \"$(systemctl is-active {u})\" = active ]",
                    u = unit
                ),
                120,
            )
            .await?;
            if !out.success() {
                return Err(CoreError::Other(format!(
                    "{} did not come back after its backup — the service is DOWN and needs \
                     hands now",
                    unit
                )));
            }
            Ok(StepOutcome::Changed)
        });
    } else if matches!(m.backup_pause, BackupPause::Chassis) {
        // fix-171 round 3: "resume the service" is in the plan whenever
        // pausing is possible at all (BackupPause != Off), but Chassis can
        // decide there was nothing running to pause (`NothingRunning`) — a
        // live decision the manifest alone cannot predict. Marked skipped
        // rather than left silent, so n still reaches the announced m.
        runner.skip("resume the service");
    }
    if let Some(note) = &resume_note {
        runner.log(Level::Info, format!("[backup] {} {}", unit, note));
    }
    if let Err(e) = snapshot_result {
        return runner.finish_err("snapshot", &e);
    }

    // W2 / fix-113: the stack file's own retention, as compose stacks have
    // had since W2. Read here so every caller gets it without being told.
    let tiers = match crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .load()
        .await
    {
        Ok(state) => crate::ops::backup::stack_tiers(&state, &m.stack_name, &cfg.tiers),
        Err(_) => cfg.tiers.clone(),
    };

    step!(runner, "retention", {
        let out = crate::executor::run_ok(
            exec,
            &crate::ops::backup::restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 300),
        )
        .await?;
        let doomed = crate::ops::backup::retention_doomed(&out.stdout, &tiers, ctx.now_unix);
        if doomed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        crate::executor::run_ok(
            exec,
            &crate::ops::backup::restic_cmd(cfg, &m.unit, &args, 900),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[backup] {} (native, in-container) snapshot complete",
            m.stack_name
        ),
    );
    runner.finish_ok()
}

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
/// The user a unit runs as, read from its `User=` line.
///
/// The orchestrator installs a binary as root; the chassis kit's own update
/// then wants to keep the version it replaces beside it, and a service cannot
/// move root's file out of the way. On 2026-09-10 that surfaced as
/// `cannot keep the previous binary at /opt/kyu/bin/kyu.prev: Operation not
/// permitted` on all three services of CT 109 — a deploy that had reported
/// success three times. The unit is the only place that says who the service
/// is, and it is already in hand here, so nothing new has to be declared.
pub fn unit_user(unit_file: &str) -> Option<String> {
    unit_file
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("User="))
        .map(str::trim)
        .filter(|u| !u.is_empty() && !u.starts_with('%'))
        .map(str::to_string)
}

/// Hand the service its own program directory.
///
/// Recursive on the directory holding the binary, which is exactly what the
/// kit needs to write its `.prev` beside it. Anything a service needs beyond
/// that, its own unit says and its own deploy does.
pub fn own_program_dir_script(user: &str, binary: &str) -> String {
    format!(
        "d=$(dirname {bin}) && chown -R {u}:{u} \"$d\" && chown {u}:{u} {bin}",
        bin = shq(binary),
        u = user
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

/// T87: what the staged binary asks of the container's C library, and what
/// the container has — read on the container, in one line, without tools.
///
/// A dynamically linked program names every glibc version it needs as a
/// plain string (`GLIBC_2.39`) in its version-needs table, so `grep -a`
/// finds the highest one without binutils; a static build carries none.
/// `getconf GNU_LIBC_VERSION` is the C library asking itself. Measured on
/// CT 109 (glibc 2.41): the static kyu binary answers `need=none`, `curl`
/// answers `need=2.34`, `bash` `need=2.38`.
///
/// Why it exists: on 2026-09-09 three kit releases built against 2.39 were
/// installed on a Debian 12 container with 2.36, and the fault surfaced at
/// the restart as a crash loop under `Restart=always` — the most expensive
/// place there is (F304). Static builds made that go away for now; this is
/// the net under the next non-static release.
pub fn glibc_probe_script(staged: &str) -> String {
    format!(
        "need=$(grep -ao 'GLIBC_2\\.[0-9]*' {bin} 2>/dev/null | sed 's/GLIBC_//' | \
         sort -t. -k2,2n -u | tail -1); \
         have=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{{print $2}}'); \
         echo \"need=${{need:-none}} have=${{have:-unknown}}\"",
        bin = shq(staged)
    )
}

/// The verdict on that probe's one line. `Ok` carries the sentence to log;
/// `Err` carries the sentence to refuse with. Fail-closed on purpose: a
/// requirement that cannot be compared is refused, because the alternative
/// is discovering the answer as a crash loop at the first restart.
pub fn glibc_verdict(probe_output: &str) -> Result<String, String> {
    let line = probe_output.trim();
    let mut need: Option<&str> = None;
    let mut have: Option<&str> = None;
    for tok in line.split_whitespace() {
        if let Some(v) = tok.strip_prefix("need=") {
            need = Some(v);
        } else if let Some(v) = tok.strip_prefix("have=") {
            have = Some(v);
        }
    }
    let (Some(need), Some(have)) = (need, have) else {
        return Err(format!(
            "could not read what glibc the staged binary needs (probe said '{}')",
            line
        ));
    };
    if need == "none" {
        return Ok("the binary is static — it asks nothing of the container's glibc".into());
    }
    let minor = |v: &str| -> Option<u32> { v.strip_prefix("2.")?.parse().ok() };
    match (minor(need), minor(have)) {
        (Some(n), Some(h)) if n <= h => Ok(format!(
            "the binary needs glibc {} and the container has {}",
            need, have
        )),
        (Some(_), Some(_)) => Err(format!(
            "the binary needs glibc {} and the container has {} — it would install fine and \
             crash-loop at the first restart; refusing. Ship a static build, or one \
             built against glibc {} or older",
            need, have, have
        )),
        _ => Err(format!(
            "the binary needs glibc {} and the container's version could not be read ('{}') — \
             refusing rather than finding out at the first restart",
            need, have
        )),
    }
}

/// B1: what the latest release of a repository offers — the tag, and the
/// download URLs of the asset and its checksum list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRefs {
    pub tag: String,
    pub asset_url: String,
    pub sums_url: String,
    /// fix-29: `SHA256SUMS.minisig`, or None while the release is unsigned.
    pub sig_url: Option<String>,
}

/// GitHub's `releases/latest` answer, reduced to what an update needs. A
/// release without the asset, or without SHA256SUMS, is refused here — an
/// unverifiable binary is exactly the hand-built step this replaces.
pub fn parse_latest_release(json: &str, asset: &str) -> Result<ReleaseRefs, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("release listing is not JSON: {}", e))?;
    if let Some(msg) = v.get("message").and_then(|m| m.as_str())
        && v.get("tag_name").is_none()
    {
        return Err(format!("GitHub answered: {}", msg));
    }
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or("release listing carries no tag_name")?
        .to_string();
    let url_of = |name: &str| -> Option<String> {
        v.get("assets")?
            .as_array()?
            .iter()
            .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))?
            .get("browser_download_url")?
            .as_str()
            .map(str::to_string)
    };
    let asset_url =
        url_of(asset).ok_or_else(|| format!("release {} carries no asset '{}'", tag, asset))?;
    let sums_url = url_of("SHA256SUMS").ok_or_else(|| {
        format!(
            "release {} has no SHA256SUMS — refusing to install an unverified binary",
            tag
        )
    })?;
    let sig_url = url_of(crate::release_sig::SIG_ASSET);
    Ok(ReleaseRefs {
        tag,
        asset_url,
        sums_url,
        sig_url,
    })
}

/// dashboard-latch: the newest published release in GitHub's release LIST
/// (`/releases`, newest first) that carries `asset`, `SHA256SUMS` and the
/// signature over it. Drafts and pre-releases are passed over, and so is a
/// release that is not signed yet: between the CI upload and the author's
/// signature the newest release is unsigned, and a container missing a tool
/// should get the newest one that can be verified rather than fail its
/// deploy for an hour. Each entry is read by [`parse_latest_release`], so
/// the refusals are the same as for a single release.
pub fn newest_signed_release(json: &str, asset: &str) -> Result<ReleaseRefs, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("release list is not JSON: {}", e))?;
    let Some(list) = v.as_array() else {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("not a list of releases");
        return Err(format!("GitHub answered: {}", msg));
    };
    let flag = |r: &serde_json::Value, k: &str| r.get(k).and_then(|b| b.as_bool()) == Some(true);
    list.iter()
        .filter(|r| !flag(r, "draft") && !flag(r, "prerelease"))
        .filter_map(|r| parse_latest_release(&r.to_string(), asset).ok())
        .find(|refs| refs.sig_url.is_some())
        .ok_or_else(|| {
            format!(
                "no signed release carries '{}' (asset, SHA256SUMS and {}) — refusing to \
                 install an unverified binary",
                asset,
                crate::release_sig::SIG_ASSET
            )
        })
}

/// dashboard-latest: one release of GitHub's release LIST, reduced to what
/// a "release tag" dropdown needs — the tag, and whether it carries
/// `SHA256SUMS.minisig` (fix-29: a release without it is listed but not
/// selectable, never silently skipped).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReleaseListItem {
    pub tag: String,
    pub signed: bool,
}

/// GitHub's `releases` (list) answer, reduced to the tag and signed state
/// of every published release, in the order GitHub gives them (newest
/// first); drafts and pre-releases are left out, the same as
/// [`newest_signed_release`].
pub fn list_releases(json: &str) -> Result<Vec<ReleaseListItem>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("release list is not JSON: {}", e))?;
    let Some(list) = v.as_array() else {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("not a list of releases");
        return Err(format!("GitHub answered: {}", msg));
    };
    let flag = |r: &serde_json::Value, k: &str| r.get(k).and_then(|b| b.as_bool()) == Some(true);
    Ok(list
        .iter()
        .filter(|r| !flag(r, "draft") && !flag(r, "prerelease"))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?.to_string();
            let signed = r
                .get("assets")
                .and_then(|a| a.as_array())
                .is_some_and(|assets| {
                    assets.iter().any(|a| {
                        a.get("name").and_then(|n| n.as_str())
                            == Some(crate::release_sig::SIG_ASSET)
                    })
                });
            Some(ReleaseListItem { tag, signed })
        })
        .collect())
}

/// The checksum a SHA256SUMS file lists for `filename`, if any.
pub fn listed_sha(sums: &str, filename: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(h), Some(f)) if f.trim_start_matches('*') == filename => {
                Some(h.to_ascii_lowercase())
            }
            _ => None,
        }
    })
}

/// fix-116: the tag of `repo`'s latest release and the checksum its
/// SHA256SUMS lists for `asset`. Used only to decide that nothing needs to be
/// done; installing still goes through the signed path (`release_update`) or
/// the service's own verb.
async fn latest_listed_sha(
    exec: &dyn Executor,
    repo: &str,
    asset: &str,
) -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let out = exec
        .run(&Cmd::new(
            "curl",
            &[
                "-sSL",
                "-m",
                "30",
                "-H",
                "Accept: application/vnd.github+json",
                &url,
            ],
            60,
        ))
        .await
        .map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(out.stderr.trim().to_string());
    }
    let refs = parse_latest_release(&out.stdout, asset)?;
    let sums = exec
        .run(&Cmd::new("curl", &["-sSL", "-m", "60", &refs.sums_url], 90))
        .await
        .map_err(|e| e.to_string())?;
    if !sums.success() {
        return Err(sums.stderr.trim().to_string());
    }
    let sha = listed_sha(&sums.stdout, asset)
        .ok_or_else(|| format!("SHA256SUMS of {} lists no '{}'", refs.tag, asset))?;
    Ok((refs.tag, sha))
}

/// B1: the orchestrator's own update of a native service, from the host.
///
/// The client's `install-native` fetches through `gh`; the host has no `gh`
/// and needs none for a public repository — `curl` against the GitHub API
/// reaches it from pve (measured 200 on 2026-09-20). The decision is made
/// on checksums, not versions: SHA256SUMS is fetched first (a few hundred
/// bytes), and only when the listed sum differs from the installed binary's
/// is the asset downloaded, verified on the host, and handed to
/// `install_native` — the same staged-beside, glibc-checked, rollback-armed
/// path a client install takes. The unit file comes from the container
/// itself: it is the one systemd is running, and a rebuild put it there
/// from the repository.
/// fix-171 round 3 ("exactly the kyu install where Kenny saw the counter
/// climb"): `release_update`'s own fixed step plan — unpinned, and the unit
/// file always read from the container (never given). Every one of
/// `release_install`'s early exits (no release_repo, unsigned release, the
/// binary already current) is a legitimate reason to stop, not a defect, so
/// each one skip-marks whatever of this list it does not reach rather than
/// leaving the run's total to climb mid-flight. The one case that marks
/// nothing at all is "no release_repo declared" — nothing is ever attempted,
/// so nothing needs a plan.
pub const RELEASE_UPDATE_STEPS: &[&str] = &[
    "guard target",
    "ask GitHub for the latest release",
    "read the checksum list",
    "compare with the installed binary",
    "download and verify the asset",
    "read the unit file from the container",
];

/// fix-step-plan-nested: `release_install`'s own full plan — its own named
/// steps (which vary with `pinned`/`unit_from_repo`, both known up front)
/// followed by the `install_native` (itself followed by `adopt`) it always
/// ends with when it gets that far. Every name here is marked exactly once,
/// run or skip, by `release_install` — the single source that function and
/// `admin`'s batch-total both call, so the two can never drift the way
/// `RELEASE_UPDATE_STEPS.len() * units` once did (18 announced for kyu where
/// 69 marks actually ran).
pub fn release_install_plan_names(
    m: &NativeServiceManifest,
    pinned: Option<&str>,
    unit_from_repo: Option<&str>,
) -> Vec<String> {
    if m.release_repo.is_none() {
        // Nothing is ever attempted: the "zero marks, no plan needed" shape.
        return Vec::new();
    }
    let asked = match pinned {
        None => "ask GitHub for the latest release".to_string(),
        Some(t) => format!("ask GitHub for release {t}"),
    };
    let mut steps: Vec<String> = vec!["guard target".to_string(), asked];
    steps.push("read the checksum list".to_string());
    steps.push("compare with the installed binary".to_string());
    steps.push("download and verify the asset".to_string());
    if unit_from_repo.is_none() {
        steps.push("read the unit file from the container".to_string());
    }
    steps.extend(install_plan_names(m, Some(&format!("install-{}", m.unit))));
    steps
}

/// fix-171 round 3 (superseded in its per-unit SHAPE by fix-step-plan-nested
/// above; `admin` now calls [`release_install_plan_names`] directly, the
/// same function `release_install` itself plans from): kept as the cheap
/// "does this unit contribute anything at all" check `admin` filters units
/// with before summing their full plans.
pub fn release_update_plan_len(m: &NativeServiceManifest) -> usize {
    if m.release_repo.is_some() {
        RELEASE_UPDATE_STEPS.len()
    } else {
        0
    }
}

pub async fn release_update(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    release_install(ctx, m, None, None).await
}

/// TUI parity round (dash-install-native): `install-native` of a chosen
/// release from the dashboard, which has no `gh`. The host downloads the
/// release named `tag` itself, as [`release_update`] downloads the latest:
/// the signature over SHA256SUMS (refused, not skipped, when the release is
/// unsigned: a person asked for this one), the binary against it, and the
/// unit file from the repository (`unit_file`, sent by the dashboard from
/// its working copy, as the CLI sends it). Installed through
/// [`install_native`], staged beside, glibc-checked, rollback armed.
pub async fn install_release(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    tag: &str,
    unit_file: &str,
) -> OperationReport {
    release_install(ctx, m, Some(tag), Some(unit_file)).await
}

/// A release tag as a URL path segment may carry it.
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

async fn release_install(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    pinned: Option<&str>,
    unit_from_repo: Option<&str>,
) -> OperationReport {
    let op = match pinned {
        None => format!("release-update-{}", m.unit),
        Some(_) => format!("install-native-{}", m.unit),
    };
    let Some(_) = m.release_repo.clone() else {
        // fix-171 round 3: nothing is ever attempted here, so there is
        // nothing to plan either — the same "zero marks, no plan needed"
        // shape as `backup_native`'s stateless early return.
        let scope = Scope::top(&op, ctx.sink, ctx.journal);
        scope.log(
            Level::Info,
            format!(
                "[release] {} declares no release_repo — nothing to fetch",
                m.unit
            ),
        );
        return scope.finish(Ok(()));
    };

    let mut scope = Scope::top(&op, ctx.sink, ctx.journal);
    // fix-step-plan-nested: the FULL plan, including the `install_native` +
    // `adopt` this always ends with when it gets that far — one source
    // (`release_install_plan_names`) shared with `admin`'s batch sum, so the
    // two can never drift apart the way `RELEASE_UPDATE_STEPS.len() * units`
    // once did (18 announced for kyu where 69 marks actually ran, LIVE
    // 2026-10-02).
    scope.plan_if_top(&release_install_plan_names(m, pinned, unit_from_repo));
    let result = release_install_impl(ctx, m, pinned, unit_from_repo, &mut scope).await;
    scope.finish(result)
}

/// fix-step-plan-nested: the step logic behind `release_install`, run
/// through the `Scope` its own wrapper (above) already planned — it ends by
/// composing `install_native` (which itself composes `adopt`) as NESTED
/// calls sharing this same scope's `Runner`, rather than two more
/// standalone ops each announcing their own plan.
async fn release_install_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    pinned: Option<&str>,
    unit_from_repo: Option<&str>,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    // Checked by the wrapper before this is even called — a plan was
    // already announced, so a repository is known to exist.
    let repo = m.release_repo.clone().expect("checked by release_install");
    let asset = m.release_asset.clone().unwrap_or_else(|| m.unit.clone());
    let asked = match pinned {
        None => "ask GitHub for the latest release".to_string(),
        Some(t) => format!("ask GitHub for release {t}"),
    };
    // The nested install_native + adopt names, for the skip-marking every
    // early exit below must do — never reached in that case, but still
    // part of this run's announced plan, and every name in it gets exactly
    // one mark.
    let install_prefix = format!("install-{}", m.unit);
    let install_adopt_names = install_plan_names(m, Some(&install_prefix));
    let skip_install_adopt = |scope: &mut Scope<'_, 'a>| {
        for name in &install_adopt_names {
            scope.skip(name);
        }
    };

    scoped_step!(scope, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    if let Some(tag) = pinned
        && !valid_tag(tag)
    {
        return Err(StepFailure {
            step: "check the tag".to_string(),
            err: CoreError::Other(format!(
                "{tag:?} is not a release tag (letters, digits, dots and dashes)"
            )),
        });
    }
    let mut refs: Option<ReleaseRefs> = None;
    scoped_step!(scope, asked.as_str(), {
        let url = match pinned {
            None => format!("https://api.github.com/repos/{}/releases/latest", repo),
            Some(t) => format!("https://api.github.com/repos/{}/releases/tags/{}", repo, t),
        };
        let out = exec
            .run(&Cmd::new(
                "curl",
                &[
                    "-sSL",
                    "-m",
                    "30",
                    "-H",
                    "Accept: application/vnd.github+json",
                    &url,
                ],
                60,
            ))
            .await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "could not reach GitHub for {}: {}",
                repo,
                out.stderr.trim()
            )));
        }
        refs = Some(parse_latest_release(&out.stdout, &asset).map_err(CoreError::Other)?);
        Ok(StepOutcome::Unchanged)
    });
    let refs = refs.expect("set by the step above");

    // fix-29: an unsigned release is not installed. Not an error: the author
    // signs after uploading, and the next night tries again.
    let Some(sig_url) = refs.sig_url.clone() else {
        // fix-171 round 3: an unsigned release is a legitimate reason to
        // stop, in BOTH branches below — not a step that crashed — so the
        // rest of the announced plan is marked skipped rather than left
        // for n to fall permanently short of m. fix-step-plan-nested: that
        // now includes the nested install_native + adopt names too — never
        // reached, still part of this run's announced plan.
        scope.skip("read the checksum list");
        scope.skip("compare with the installed binary");
        scope.skip("download and verify the asset");
        if unit_from_repo.is_none() {
            scope.skip("read the unit file from the container");
        }
        skip_install_adopt(scope);
        if pinned.is_some() {
            return Err(StepFailure {
                step: "read the checksum list".to_string(),
                err: CoreError::SafetyAbort(format!(
                    "{} {} of {} is not signed (no SHA256SUMS.minisig) — not installing it; \
                     sign it first",
                    m.unit, refs.tag, repo
                )),
            });
        }
        scope.log(
            Level::Info,
            format!(
                "[release] {} {} of {} is not signed yet — skipped, tried again next night",
                m.unit, refs.tag, repo
            ),
        );
        return Ok(());
    };

    let mut wanted = String::new();
    scoped_step!(scope, "read the checksum list", {
        let out = exec
            .run(&Cmd::new("curl", &["-sSL", "-m", "60", &refs.sums_url], 90))
            .await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "could not fetch SHA256SUMS of {} {}: {}",
                repo,
                refs.tag,
                out.stderr.trim()
            )));
        }
        let sums = out.stdout.clone();
        let sig = exec
            .run(&Cmd::new("curl", &["-sSL", "-m", "60", &sig_url], 90))
            .await?;
        if !sig.success() {
            return Err(CoreError::Other(format!(
                "could not fetch the signature of {} {}: {}",
                repo,
                refs.tag,
                sig.stderr.trim()
            )));
        }
        crate::release_sig::verify_sums(&sums, &sig.stdout)
            .map_err(|e| CoreError::Other(format!("{} {} of {}: {}", m.unit, refs.tag, repo, e)))?;
        wanted = listed_sha(&sums, &asset).ok_or_else(|| {
            CoreError::Other(format!(
                "SHA256SUMS of {} {} lists no '{}' — refusing an unverifiable binary",
                repo, refs.tag, asset
            ))
        })?;
        Ok(StepOutcome::Unchanged)
    });

    let mut current = false;
    scoped_step!(scope, "compare with the installed binary", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!("sha256sum {} 2>/dev/null | cut -d' ' -f1", shq(&m.binary)),
            60,
        )
        .await?;
        current = out.stdout.trim().eq_ignore_ascii_case(&wanted);
        Ok(StepOutcome::Unchanged)
    });
    if current {
        // fix-171 round 3: already on the wanted binary is success, not
        // failure — the remaining planned steps, including install_native +
        // adopt, are marked skipped (fix-step-plan-nested).
        scope.skip("download and verify the asset");
        if unit_from_repo.is_none() {
            scope.skip("read the unit file from the container");
        }
        skip_install_adopt(scope);
        scope.log(
            Level::Info,
            format!(
                "[release] {} already runs {} of {} — nothing to install",
                m.unit, refs.tag, repo
            ),
        );
        return Ok(());
    }

    let staged = format!(
        "{}/staged/{}/{}.release",
        ctx.state_dir, m.stack_name, m.unit
    );
    let mut b64 = String::new();
    scoped_step!(scope, "download and verify the asset", {
        let dir = staged.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        let script = format!(
            "mkdir -p {d} && curl -sSL -m 600 -o {f} {u} && sha256sum {f} | cut -d' ' -f1",
            d = shq(dir),
            f = shq(&staged),
            u = shq(&refs.asset_url)
        );
        let out = exec.run(&Cmd::new("sh", &["-c", &script], 700)).await?;
        if !out.success() {
            let _ = exec.run(&Cmd::new("rm", &["-f", &staged], 30)).await;
            return Err(CoreError::Other(format!(
                "download of {} {} failed: {}",
                asset,
                refs.tag,
                out.stderr.trim()
            )));
        }
        if !out.stdout.trim().eq_ignore_ascii_case(&wanted) {
            let _ = exec.run(&Cmd::new("rm", &["-f", &staged], 30)).await;
            return Err(CoreError::SafetyAbort(format!(
                "CHECKSUM MISMATCH for {} in {} {} — download corrupted or tampered; nothing \
                 installed",
                asset, repo, refs.tag
            )));
        }
        let enc = exec
            .run(&Cmd::new("base64", &["-w0", &staged], 300))
            .await?;
        let _ = exec.run(&Cmd::new("rm", &["-f", &staged], 30)).await;
        if !enc.success() || enc.stdout.trim().is_empty() {
            return Err(CoreError::Other(format!(
                "could not encode {} for the transfer into the container",
                staged
            )));
        }
        b64 = enc.stdout.trim().to_string();
        Ok(StepOutcome::Changed)
    });

    let mut unit_file = unit_from_repo.map(str::to_string).unwrap_or_default();
    if unit_from_repo.is_none() {
        scoped_step!(scope, "read the unit file from the container", {
            let out = util_pct_sh(
                exec,
                m.vmid,
                &format!("cat /etc/systemd/system/{}.service", m.unit),
                30,
            )
            .await?;
            if !out.success() || out.stdout.trim().is_empty() {
                return Err(CoreError::Other(format!(
                    "{}.service is not on {} — a service without its unit is not one this \
                 orchestrator installs; adopt or deploy it first",
                    m.unit, m.hostname
                )));
            }
            unit_file = out.stdout;
            Ok(StepOutcome::Unchanged)
        });
    }

    // fix-step-plan-nested: nested through THIS scope (flattened to the one
    // underlying `Runner`, qualified "install-<unit> :: …" — `install_native`
    // itself goes on to nest `adopt` the same way), not a second and third
    // standalone op each announcing their own plan.
    let mut install_scope = scope.child(install_prefix);
    install_native_impl(ctx, m, &b64, &unit_file, &mut install_scope).await?;

    scope.log(
        Level::Info,
        format!(
            "[release] {} updated to {} of {} — installed under the armed rollback",
            m.unit, refs.tag, repo
        ),
    );
    Ok(())
}

/// T85: fill a deploy's binary map from what was staged one message at a
/// time. A unit whose entry is missing or empty is looked up; a unit that
/// arrived with its bytes (an older client) is left alone; an entry that is
/// empty with nothing staged is dropped, so the deploy treats the binary as
/// not shipped rather than as an empty program. Returns the units filled.
pub fn merge_staged_binaries(
    natives: &[String],
    map: &mut std::collections::BTreeMap<String, String>,
    fetch: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut merged = Vec::new();
    for unit in natives {
        let present = map.get(unit).map(|v| !v.trim().is_empty()).unwrap_or(false);
        if present {
            continue;
        }
        match fetch(unit) {
            Some(b64) if !b64.trim().is_empty() => {
                map.insert(unit.clone(), b64);
                merged.push(unit.clone());
            }
            _ => {}
        }
    }
    map.retain(|_, v| !v.trim().is_empty());
    merged
}

pub fn rollback_script(unit: &str, prev: &str, binary: &str) -> String {
    format!(
        "systemctl stop {u}; cp -p {prev} {bin} && systemctl start {u} && sleep 2 && \
         [ \"$(systemctl is-active {u})\" = active ]",
        u = unit,
        prev = shq(prev),
        bin = shq(binary)
    )
}

/// `stored_at` is when the host last wrote the manifest this runs from —
/// `StackState::applied_at`, passed in because core never reads a clock. It is
/// only used to make the skip message below say where its facts come from.
pub async fn update_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    stored_at: Option<u64>,
) -> OperationReport {
    let op = format!("update-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let unit = format!("{}.service", m.unit);
    let prev = format!("{}.homelab-prev", m.binary);

    let Some(update_cmd) = m.update_cmd.clone() else {
        // This used to read "skipped by decision", which is what an empty
        // field looks like from the inside and a deliberate choice from the
        // outside. On 2026-09-10 it said that three times about three
        // services whose stack files all carried an update_cmd: the host was
        // reading a copy stored before those files were written. A service
        // left on an old version for months while the nightly round reports
        // every night that this is intended is never found, so the message
        // now names the copy it read and how to refresh it.
        let when = match stored_at {
            Some(t) => format!("stored on {}", crate::state::ymd(t)),
            None => "stored at an unrecorded time".to_string(),
        };
        runner.log(
            Level::Info,
            format!(
                "[update] {} skipped: the manifest the host has {} carries no update_cmd. \
                 This is the host's copy, not what the repository says — if the stack file \
                 has one, `homelab adopt <path to the service>` refreshes it.",
                m.stack_name, when
            ),
        );
        return runner.finish_ok();
    };

    // fix-171 round 3: `m.update_cmd` is a manifest field, so by this point
    // the run is committed to the full fixed plan — whether it stops early
    // because the release is already current (a live GitHub decision) is
    // handled by skip-marking the rest, below.
    runner.plan(&[
        "guard target",
        "read the installed binary",
        "preserve binary",
        "run self-update",
        "restart if changed",
        "keep one previous binary",
        "drop the kit's own rollback copy",
    ]);

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    let mut before = String::new();
    step!(runner, "read the installed binary", {
        let sum = util_pct_sh(
            exec,
            m.vmid,
            &format!("sha256sum {} | cut -d' ' -f1", shq(&m.binary)),
            60,
        )
        .await?;
        before = sum.stdout.trim().to_string();
        Ok(StepOutcome::Unchanged)
    });

    // fix-116: the release is asked first, as `release_update` does, and a
    // service already on it is left alone — no copy, no update run. When
    // the release cannot be read the service's own update decides, as
    // before. Story: docs/deployment/REGISTER.md.
    if let Some(repo) = &m.release_repo {
        match latest_listed_sha(exec, repo, m.asset_name()).await {
            Ok((tag, listed)) if !before.is_empty() && listed.eq_ignore_ascii_case(&before) => {
                // fix-171 round 3: already current is success, not a crash —
                // the four remaining planned steps are marked skipped.
                runner.skip("preserve binary");
                runner.skip("run self-update");
                runner.skip("restart if changed");
                runner.skip("keep one previous binary");
                runner.skip("drop the kit's own rollback copy");
                runner.log(
                    Level::Info,
                    format!(
                        "[update] {} already runs {} of {} — nothing copied, no update run",
                        m.unit, tag, repo
                    ),
                );
                return runner.finish_ok();
            }
            Ok(_) => {}
            Err(why) => runner.log(
                Level::Info,
                format!(
                    "[update] could not read the latest release of {} ({}) — the service's own \
                     update decides",
                    repo, why
                ),
            ),
        }
    }

    step!(runner, "preserve binary", {
        let out = util_pct_sh(exec, m.vmid, &preserve_script(&m.binary, &prev), 60).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "cannot preserve {} — refusing to update without a rollback copy",
                m.binary
            )));
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "run self-update", {
        let out = util_pct_sh(exec, m.vmid, &update_cmd, 900).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "'{}' failed: {} — binary untouched, service still on the old version",
                update_cmd,
                out.stderr.trim()
            )));
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "restart if changed", {
        let sum = util_pct_sh(
            exec,
            m.vmid,
            &format!("sha256sum {} | cut -d' ' -f1", shq(&m.binary)),
            60,
        )
        .await?;
        if sum.stdout.trim() == before {
            // Already current: no restart, no nightly service blip.
            return Ok(StepOutcome::Unchanged);
        }
        let out = util_pct_sh(exec, m.vmid, &health_script(&unit), 180).await?;
        if out.success() {
            return Ok(StepOutcome::Changed);
        }
        // The armed rollback: restore the preserved binary from OUTSIDE the
        // (dead) app, restart, and report the failure loudly either way.
        let rb = util_pct_sh(exec, m.vmid, &rollback_script(&unit, &prev, &m.binary), 180).await?;
        // fix-114: the binary running now is the one kept before this run,
        // so the previous one from before it is the kept one again.
        let _ = util_pct_sh(exec, m.vmid, &restore_set_aside_script(&prev), 60).await;
        Err(CoreError::Other(format!(
            "new {} version did not come up healthy — rolled back to the previous binary ({}); \
             investigate before the next nightly run",
            m.stack_name,
            if rb.success() {
                "service restored and active"
            } else {
                "ROLLBACK ALSO FAILED — service needs hands NOW"
            }
        )))
    });

    // fix-114: exactly one previous binary stays, the way back when this
    // release misbehaves after its health window.
    let mut stale_kept = false;
    step!(runner, "keep one previous binary", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &keep_one_previous_script(&m.binary, &prev),
            60,
        )
        .await?;
        if !out.success() {
            stale_kept = true;
            return Ok(StepOutcome::Unchanged);
        }
        Ok(StepOutcome::Changed)
    });

    if stale_kept {
        runner.log(
            Level::Warn,
            format!(
                "could not settle the kept previous binary at {} — check it and {}.old by hand",
                prev, prev
            ),
        );
    }

    // fix-10: the chassis kit keeps its own `.prev` beside the binary, and
    // this run keeps `.homelab-prev` — the same version twice, 73 MB of it
    // on CT 109's small rootfs after the first was already gone. Past this
    // point the service has proven healthy on the binary it runs, so the
    // kit's copy has nothing left to return to either; Kenny, 2026-09-19:
    // the deploy removes it after a healthy update. A rolled-back run never
    // reaches here, so the copy the rollback may need is never touched.
    let kit_prev = format!("{}.prev", m.binary);
    let mut kit_kept = false;
    step!(runner, "drop the kit's own rollback copy", {
        let out = util_pct_sh(exec, m.vmid, &drop_stale_rollback_script(&kit_prev), 60).await?;
        if !out.success() {
            kit_kept = true;
            return Ok(StepOutcome::Unchanged);
        }
        Ok(StepOutcome::Changed)
    });
    if kit_kept {
        runner.log(
            Level::Warn,
            format!(
                "could not remove {} — the kit's copy stays on disk",
                kit_prev
            ),
        );
    }
    runner.log(
        Level::Info,
        format!("[update] {} self-update supervised — healthy", m.stack_name),
    );
    runner.finish_ok()
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
fn restore_set_aside_script(prev: &str) -> String {
    format!(
        "if [ -f {old} ]; then mv -f {old} {p}; fi",
        old = shq(&format!("{}.old", prev)),
        p = shq(prev)
    )
}

/// fix-114: `homelab rollback-native <stack>/<unit>` — go back to the kept
/// previous binary by hand, when a release that passed its health window
/// misbehaves later. The unit is stopped, the kept binary copied into place
/// and held to the same health check as an update; if it does not come up,
/// the binary that was running goes back. The version rolled back from
/// becomes the kept one, so running it again returns. The stack's automatic
/// updates are parked (fix-59), or the next night would reinstall the release
/// that was just rolled back; `homelab enable <stack>` resumes them.
pub async fn rollback_native(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let op = format!("rollback-{}", m.unit);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let unit = format!("{}.service", m.unit);
    let prev = format!("{}.homelab-prev", m.binary);
    let swap = format!("{}.homelab-rollback", m.binary);

    // fix-171 round 3: fixed and unconditional, like `destroy::STEPS`.
    runner.plan(&[
        "guard target",
        "a previous binary is kept",
        "roll back",
        "keep the other as previous",
        "park automatic updates",
    ]);

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "a previous binary is kept", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!(
                "test -f {p} && ! cmp -s {b} {p} && echo yes || echo no",
                p = shq(&prev),
                b = shq(&m.binary)
            ),
            60,
        )
        .await?;
        if out.stdout.trim() != "yes" {
            return Err(CoreError::SafetyAbort(format!(
                "no previous binary of {} is kept at {} (or it is the one running) — nothing \
                 was stopped",
                m.unit, prev
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "roll back", {
        let script = format!(
            "systemctl stop {u}; cp -p {b} {swap} && cp -p {p} {b}",
            u = unit,
            b = shq(&m.binary),
            swap = shq(&swap),
            p = shq(&prev)
        );
        let out = util_pct_sh(exec, m.vmid, &script, 180).await?;
        if !out.success() {
            let _ = util_pct_sh(exec, m.vmid, &format!("systemctl start {}", unit), 60).await;
            return Err(CoreError::Other(format!(
                "could not put the kept binary of {} in place ({}) — the unit was started \
                 again on the binary it had",
                m.unit,
                out.stderr.trim()
            )));
        }
        let health = util_pct_sh(exec, m.vmid, &health_script(&unit), 180).await?;
        if health.success() {
            return Ok(StepOutcome::Changed);
        }
        let back =
            util_pct_sh(exec, m.vmid, &rollback_script(&unit, &swap, &m.binary), 180).await?;
        Err(CoreError::Other(format!(
            "the kept binary of {} did not come up healthy ({}) — {}",
            m.unit,
            health.stdout.trim(),
            if back.success() {
                "the binary that was running is back and active"
            } else {
                "putting the running binary back ALSO FAILED — the service needs hands NOW"
            }
        )))
    });

    step!(runner, "keep the other as previous", {
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!("mv -f {} {}", shq(&swap), shq(&prev)),
            60,
        )
        .await?;
        Ok(if out.success() {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    let now = ctx.now_unix;
    let stack = m.stack_name.clone();
    step!(runner, "park automatic updates", {
        crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
            .update(|s| {
                s.updates_parked.insert(stack.clone(), now);
            })
            .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Warn,
        format!(
            "[rollback] {} runs its previous binary again; automatic updates of {} are parked \
             until `homelab enable {}`",
            m.unit, m.stack_name, m.stack_name
        ),
    );
    runner.finish_ok()
}

/// fix-114: which unit of a native stack a per-unit verb acts on — the one
/// named, or the only one there is.
pub fn select_unit(
    services: &[NativeServiceManifest],
    unit: Option<&str>,
) -> Result<NativeServiceManifest, String> {
    let names = || {
        services
            .iter()
            .map(|s| s.unit.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match unit {
        Some(u) => services
            .iter()
            .find(|s| s.unit == u)
            .cloned()
            .ok_or_else(|| format!("no unit '{}' on this stack (it has: {})", u, names())),
        None if services.len() == 1 => Ok(services[0].clone()),
        None => Err(format!(
            "this stack has several units ({}) — name one as <stack>/<unit>",
            names()
        )),
    }
}

/// gap-27 / redesign-stackhub-2: copy one secret file from a container into
/// the host's vault, read without echoing it (fix-39); nothing in the
/// container is written. False when the file is absent or empty there.
pub async fn seal_one(
    exec: &dyn Executor,
    vmid: u16,
    on_container: &str,
    vault: &str,
) -> Result<bool, CoreError> {
    let got = crate::executor::pct_sh_secret(
        exec,
        vmid,
        &format!(
            "cat {} 2>/dev/null || true",
            crate::ops::util::shq(on_container)
        ),
        60,
    )
    .await?;
    if got.stdout.trim().is_empty() {
        return Ok(false);
    }
    exec.write_file(vault, &got.stdout, 0o600).await?;
    Ok(true)
}
