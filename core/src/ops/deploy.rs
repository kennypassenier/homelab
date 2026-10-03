//! The deploy operation (D1): validate → safety gates → storage → provision →
//! bootstrap → guards → intent to repo → push files/env → start apps →
//! verify → gateway route → record state. Every step runs through the shared
//! runner (AR3); every side effect through the Executor (AR2).

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok, shq};
use crate::manifest::{self, DeploySpec};
use crate::ops::{OpCtx, guards, util::push_content};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::safety;
use crate::sink::{Level, PipelineEvent};
use crate::state::{StackState, StateStore};

/// Shorthand: run a step, bail out of the operation on failure (A3).
///
/// S2: before bailing, leave a record that this stack was half-deployed. The
/// deploy's own `record state` step is the last one, so until now a failure
/// anywhere before it wrote nothing at all — see `mark_incomplete`.
macro_rules! step {
    ($runner:expr_2021, $exec:expr_2021, $ctx:expr_2021, $m:expr_2021, $name:expr_2021, $body:expr_2021) => {
        match $runner.step($name, || async { $body }).await {
            Ok(outcome) => outcome,
            Err(e) => {
                mark_incomplete($exec, $ctx, $m, $name).await;
                return $runner.finish_err($name, &e);
            }
        }
    };
}

/// Like `step!`, but the step must prove its own work (S2).
macro_rules! stepv {
    ($runner:expr_2021, $exec:expr_2021, $ctx:expr_2021, $m:expr_2021, $name:expr_2021, $body:expr_2021, $verify:expr_2021) => {
        match $runner
            .step_verified($name, || async { $body }, || async { $verify })
            .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                mark_incomplete($exec, $ctx, $m, $name).await;
                return $runner.finish_err($name, &e);
            }
        }
    };
}

/// Record that a deploy stopped part-way, so the stack is at least visible.
///
/// Deliberately silent about its own failures: this runs while another error
/// is already on its way to the operator, and a second one stacked on top of
/// it helps nobody. The worst case is the state we had before — no record —
/// which is exactly what this exists to avoid, so it is worth attempting and
/// not worth escalating.
async fn mark_incomplete(
    exec: &dyn crate::executor::Executor,
    ctx: &OpCtx<'_>,
    m: &crate::manifest::StackManifest,
    step: &str,
) {
    // A1 promises that a refused target runs ZERO commands, and D10 promises
    // the same for a manifest that does not validate. Writing a state record
    // is a command. These four steps all run before anything on the machine
    // has been touched, so a failure in them leaves nothing half-done and
    // there is nothing to record — recording anyway would both break the
    // guarantee and invent a stack the operator never deployed.
    const BEFORE_ANYTHING_CHANGES: [&str; 4] = [
        "validate",
        "safety gates",
        "registry cache",
        "hardware readiness",
    ];
    if BEFORE_ANYTHING_CHANGES.contains(&step) {
        return;
    }
    let store = StateStore::new(exec, &ctx.state_dir);
    let Ok(mut state) = store.load().await else {
        return;
    };
    match state.stacks.get_mut(&m.stack_name) {
        // Known already: keep everything, just mark where it stopped. The
        // manifest on record stays the last one that fully applied, because
        // that is what actually ran — not what this attempt wanted.
        Some(existing) => existing.incomplete_step = Some(step.to_string()),
        // Never recorded: write the minimum that makes it exist. Nightly
        // backups key off this entry, and 12 GB of configuration with no
        // backup was how this defect announced itself.
        None => {
            state.stacks.insert(
                m.stack_name.clone(),
                StackState {
                    applied_source: None,
                    vmid: m.vmid,
                    hostname: m.hostname.clone(),
                    apps: m.apps.clone(),
                    applied_at: ctx.now_unix,
                    last_backup: 0,
                    applied_hash: String::new(),
                    manifest: Some(m.clone()),
                    enabled: true,
                    route_file: None,
                    extra_route_files: Vec::new(),
                    natives: Vec::new(),
                    incomplete_step: Some(step.to_string()),
                    pushed_file_hashes: std::collections::BTreeMap::new(),
                    component_digests: Default::default(),
                },
            );
        }
    }
    let _ = store.save(state).await;
    ctx.sink.emit(PipelineEvent::Line {
        level: Level::Warn,
        source: "HOST".into(),
        msg: format!(
            "[state] '{}' recorded as incomplete at step '{}' — it is managed, \
             but what is on the container is not what the stack file says",
            m.stack_name, step
        ),
    });
}
/// Did `docker compose up -d` create or recreate a container? Compose ends
/// such a status line with the word `Created` or `Recreated`.
///
/// shell-strings-quoting (expert panel, 2026-09-27): this was
/// `contains("reated")`, which a warning that a volume "was not created by
/// Docker Compose" also matches, and the restart a changed file needs was
/// then skipped.
pub fn compose_reports_created(out: &str) -> bool {
    out.lines().any(|l| {
        let mut words = l.split_whitespace();
        words.next() == Some("Container")
            && matches!(words.last(), Some("Created") | Some("Recreated"))
    })
}

/// F129 · per app: where its compose lives in the container, what it said
/// before the cache rewrite, and the mode to write it back with.
type CacheOriginals =
    std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, (String, String, String)>>>;

/// Files under `/opt/<stack>/` on the container that this deploy did not send.
///
/// Compared against the spec's own file list, so the answer is "what the
/// repository no longer has" rather than "what looks old". Best-effort: a
/// container that will not answer produces an empty list rather than a
/// finding, because an unasked question must never become one.
///
/// `.env` files are excluded on purpose — they come from the host vault
/// (D12), never from the repository, so every one of them would be reported
/// as an orphan forever.
pub async fn orphan_files(
    exec: &dyn crate::executor::Executor,
    m: &crate::manifest::StackManifest,
    spec: &DeploySpec,
) -> Vec<String> {
    orphan_files_keeping(exec, m, spec, &[]).await
}

/// Directories inside container `vmid` that the orchestrator fills on behalf
/// of OTHER stacks: on the gateway, the route fragments (H1). Their files
/// are in no stack's file list, so without this a deploy of the gateway
/// would take them for its own orphans and remove every other stack's route.
pub fn generated_dirs(safety: &crate::safety::SafetyConfig, vmid: u16) -> Vec<String> {
    let mut v = Vec::new();
    if vmid == safety.gateway_vmid {
        v.push(safety.gateway_routes_dir.clone());
    }
    v
}

/// [`orphan_files`], leaving out anything under the `keep` directories
/// (absolute paths inside the container, see [`generated_dirs`]).
pub async fn orphan_files_keeping(
    exec: &dyn crate::executor::Executor,
    m: &crate::manifest::StackManifest,
    spec: &DeploySpec,
    keep: &[String],
) -> Vec<String> {
    let sent: std::collections::BTreeSet<&str> =
        spec.files.iter().map(|f| f.path.as_str()).collect();
    let kept = |rel: &str| {
        let abs = format!("/opt/{}/{}", m.stack_name, rel);
        keep.iter()
            .any(|d| abs.starts_with(&format!("{}/", d.trim_end_matches('/'))))
    };
    let out = match pct_sh(
        exec,
        m.vmid,
        &format!(
            "cd {0} 2>/dev/null && find . -type f -printf '%P\\n' 2>/dev/null || true",
            shq(&format!("/opt/{}", m.stack_name))
        ),
        120,
    )
    .await
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    out.stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.ends_with(".env"))
        .filter(|l| !sent.contains(*l))
        .filter(|l| !kept(l))
        .map(str::to_string)
        .collect()
}

/// A5: the vault filename for a file a unit reads.
///
/// fix-37: `<parent dir>/<name>`, so `/appdata/kyu/kyu-runner-config/token.env`
/// becomes `kyu-runner-config/token.env` under `<state_dir>/secrets/<stack>/`.
/// It used to be the bare file name, on the belief that two units on one
/// stack cannot name the same file. They can: kyu-runner and
/// http-switchboard both read `token.env`, so they shared ONE vault copy and
/// the vault kept whichever was copied last; a rebuild would have given both
/// the same token.
/// Below this share of available memory a deploy does not start
/// (corr-kuma-memory, 2026-09-27).
pub const MIN_MEMORY_AVAILABLE: f64 = 0.15;

/// `MemAvailable / MemTotal` from a container's /proc/meminfo (lxcfs shows
/// the container's own limit). None when either line is missing.
pub fn memory_available_fraction(meminfo: &str) -> Option<f64> {
    let field = |name: &str| {
        meminfo.lines().find_map(|l| {
            let rest = l.strip_prefix(name)?.trim_start_matches(':').trim();
            rest.split_whitespace().next()?.parse::<f64>().ok()
        })
    };
    let total = field("MemTotal")?;
    let avail = field("MemAvailable")?;
    (total > 0.0).then(|| avail / total)
}

pub fn vault_key(path: &str) -> String {
    let mut parts = path.rsplit('/');
    let name = parts.next().unwrap_or(path);
    match parts.next() {
        Some(dir) if !dir.is_empty() => format!("{}/{}", dir, name),
        _ => name.to_string(),
    }
}

/// The flat key older deploys wrote, still read when it is unambiguous.
fn legacy_vault_key(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// fix-147 (restore-check-failure, 2026-09-27): remember each empty
/// directory or native unit whose backup could not be checked, and forget
/// each one whose check succeeded. Keys are `<stack>:<path or unit>`. A
/// failure to record is said, never fatal: it must not block a deploy either.
pub(crate) async fn record_restore_checks(
    ctx: &OpCtx<'_>,
    stack: &str,
    failed: &[(String, String)],
    ok: &[String],
) {
    if failed.is_empty() && ok.is_empty() {
        return;
    }
    let now = ctx.now_unix;
    let recorded = crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .update(|s| {
            for what in ok {
                s.restore_check_failures
                    .remove(&format!("{}:{}", stack, what));
            }
            for (what, why) in failed {
                s.restore_check_failures.insert(
                    format!("{}:{}", stack, what),
                    crate::state::RestoreCheckFailure {
                        stack: stack.to_string(),
                        what: what.clone(),
                        at: now,
                        why: why.clone(),
                    },
                );
            }
        })
        .await;
    if let Err(e) = recorded {
        ctx.sink.emit(PipelineEvent::Line {
            level: Level::Warn,
            source: "HOST".into(),
            msg: format!(
                "[e3] could not record the backup checks of {} :: {}",
                stack, e
            ),
        });
    }
}

/// fix-171 round 2: deploy's own step plan — fixed, EVERY deploy, regardless
/// of how many apps the manifest declares, whether it has a firewall, gives
/// a gateway route, or needs a log shipper. Every step here is already
/// invoked unconditionally and no-ops when its precondition does not hold
/// (`baseline` when the container does not exist yet, `native units` when
/// the manifest declares none); the six that used to be invoked only
/// conditionally (`firewall`, `firewall after create`,
/// `refresh the tile watcher's firewall`, `gateway route`,
/// `retire gateway route`, `log shipper`) now take an explicit `skip` mark
/// in their `else` branch instead of silently not appearing — so `n`
/// reaches this list's length exactly, every time, and the dashboard's
/// total is this constant rather than a guess from a past run's count
/// (fix-171's first attempt, rejected for exactly that reason). Shared with
/// the dashboard (`admin::shell::actions::execute`) so Apply and a batch of
/// several deploys can announce the combined total before the first one
/// starts.
pub const STEPS: &[&str] = &[
    "validate",
    "safety gates",
    "baseline",
    "registry cache",
    "hardware readiness",
    "host storage",
    "auto-restore check",
    "metrics discovery",
    "firewall",
    "provision container",
    "firewall after create",
    "refresh the tile watcher's firewall",
    "wait for systemd",
    "bootstrap docker",
    "runaway guards",
    "log rotation",
    "retire dropped",
    "commit intent",
    "push files",
    "start apps",
    "storage ownership",
    "verify health",
    "gateway route",
    "retire gateway route",
    "orphan files",
    "garbage collect",
    "log shipper",
    "native units",
    "record state",
    "reconcile",
    "service checks",
];

pub async fn deploy(ctx: &OpCtx<'_>, spec: &DeploySpec) -> OperationReport {
    let m = &spec.manifest;
    let op = format!("deploy-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    runner.plan(STEPS);
    runner.log(
        Level::Info,
        format!("[sync][run ] deploy {} (vmid {})", m.stack_name, m.vmid),
    );

    // Every command flows through the tracing decorator, so transcripts are
    // both streamed (F2) and captured for incident replay (AR16).
    let texec = crate::executor::TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn crate::executor::Executor = &texec;
    let vm = m.vmid.to_string();
    let mut exists = false;
    let mut created = false;
    // W1: what the host actually offers, read once before anything is
    // created. None when the stack asks for no hardware at all.
    let mut gpu: Option<crate::ops::hardware::GpuDevices> = None;
    // For logging inside step bodies (the runner itself is mutably borrowed
    // by `step` while a body runs).
    let log_info = |msg: String| {
        ctx.sink.emit(PipelineEvent::Line {
            level: Level::Info,
            source: "HOST".into(),
            msg,
        })
    };
    // A5: the same, at a level that reaches the notification. A native unit
    // left unstarted because its program or its secret is missing is not a
    // detail in a transcript — it is the difference between a container that
    // was rebuilt and one that only looks rebuilt.
    let log_warn = |msg: String| {
        ctx.sink.emit(PipelineEvent::Line {
            level: Level::Warn,
            source: "HOST".into(),
            msg,
        })
    };

    // ── D10: never trust the client — validate host-side too. ────────────
    step!(runner, exec, ctx, m, "validate", {
        manifest::validate(spec)?;
        // fix-88: only the host knows which container is the gateway, so
        // the port-80 rule (traefik-lan-host-header-bypass) is checked here,
        // still before anything changes.
        if let Some(fw) = m.firewall.as_ref().filter(|f| f.enabled)
            && m.vmid == ctx.safety.gateway_vmid
        {
            // tile-watch: the derived rule counts too — a gateway tile
            // that opens port 80 on itself is exactly what
            // traefik-lan-host-header-bypass forbids, derived or not.
            let effective = crate::firewall::with_tile_watch(
                fw,
                &m.network.ip,
                &m.tiles,
                ctx.tile_watch_source.as_deref().unwrap_or(""),
                &ctx.tile_watch_targets,
            );
            let p = crate::firewall::gateway_problems(&effective);
            if !p.is_empty() {
                return Err(CoreError::Validation(p.join("; ")));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    // ── A1 + A2: refuse before anything mutates. ─────────────────────────
    step!(runner, exec, ctx, m, "safety gates", {
        exists = safety::check_deploy_target(exec, &ctx.safety, m).await?;
        Ok(StepOutcome::Unchanged)
    });

    // ── step-22 / ask-8: the record as it stood before this deploy. What
    // the stack file used to declare is the only way to know what has left
    // it — a mount, a native unit, a route — and so what to take away.
    // Read after the safety gates: a refused target reads nothing.
    let prior: Option<StackState> = StateStore::new(exec, &ctx.state_dir)
        .load()
        .await
        .ok()
        .and_then(|s| s.stacks.get(&m.stack_name).cloned());
    let prior_manifest: Option<manifest::StackManifest> =
        prior.as_ref().and_then(|p| p.manifest.clone());
    // ask-8 (`Stoppen, data bewaren`): only a unit the stack FILE used to
    // declare is retired. One registered by `homelab adopt` that no stack
    // file ever named is not the deploy's to take away.
    //
    // fix-186: a unit this deploy still declares as a native is never
    // dropped — and neither is one that moved from `natives:` to `apps:`
    // under the same name (a docker-to-native or native-to-docker
    // conversion is not a departure). Only a name declared NOWHERE in the
    // new manifest left the stack.
    let dropped_natives: Vec<String> = prior_manifest
        .as_ref()
        .map(|pm| {
            pm.natives
                .iter()
                .filter(|u| !m.natives.contains(u) && !m.apps.contains(u))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    // ── J1: the BEFORE half of every service's own health checks.
    //
    // Here, and not later, because "may rise, never fall" is only true while
    // the two readings are minutes apart — and because everything after this
    // point can change what is being measured. A stack that does not exist
    // yet simply reads empty, which the comparison treats as "nothing to
    // compare against" rather than as a fault.
    let mut baseline: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    step!(runner, exec, ctx, m, "baseline", {
        if !exists || spec.checks.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for (app, sc) in &spec.checks {
            let mut readings = Vec::new();
            for c in &sc.checks {
                let out = pct_sh(exec, m.vmid, &c.command, 120)
                    .await
                    .map(|o| o.stdout.trim().to_string())
                    .unwrap_or_default();
                readings.push((c.name.clone(), out));
            }
            baseline.insert(app.clone(), readings);
        }
        Ok(StepOutcome::Unchanged)
    });
    for (app, readings) in &baseline {
        for (name, value) in readings {
            log_info(format!("[baseline] {} · {} = {}", app, name, value));
        }
    }

    // ── D60: which upstreams does the cache answer for, right now?
    // Asked from inside the container that is about to pull, because that is
    // the only place the answer means anything. A cache that does not answer
    // is not an error: the image keeps naming its own origin and the pull
    // goes out to the internet exactly as it did before there was a cache.
    let mut cache_up: Vec<String> = Vec::new();
    step!(runner, exec, ctx, m, "registry cache", {
        let Some(cache) = ctx.registry_cache.as_ref() else {
            return Ok(StepOutcome::Unchanged);
        };
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        for up in &cache.upstreams {
            let probe = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "curl -fsS -m 3 -o /dev/null http://{}:{}/v2/ && echo UP",
                    cache.host, up.port
                ),
                30,
            )
            .await;
            if matches!(&probe, Ok(o) if o.stdout.contains("UP")) {
                cache_up.push(up.registry.clone());
            }
        }
        Ok(StepOutcome::Unchanged)
    });
    if let Some(cache) = ctx.registry_cache.as_ref()
        && !m.native_only
    {
        if cache_up.is_empty() {
            log_info(format!(
                "[cache] {} answered for nothing — every image keeps its own registry",
                cache.host
            ));
        } else {
            log_info(format!(
                "[cache] pulling via {} for: {}",
                cache.host,
                cache_up.join(", ")
            ));
        }
    }

    // ── W1: refuse hardware this host cannot give, and read the group ids
    // instead of assuming them. Before the storage step, because a stack
    // that cannot work here should not leave directories behind either.
    step!(runner, exec, ctx, m, "hardware readiness", {
        if m.lxc.gpu {
            gpu = Some(crate::ops::hardware::check_gpu(exec, &m.stack_name).await?);
        }
        if m.lxc.vpn {
            crate::ops::hardware::check_tun(exec, &m.stack_name).await?;
        }
        // M1: the directories this stack borrows rather than owns.
        crate::ops::hardware::check_data_mounts(exec, &m.stack_name, &m.data_mounts).await?;
        // Correction corr-kuma-memory (Kenny, 2026-09-27): a deploy that
        // (re)starts services in a container already short of memory tips it
        // into thrashing; on 2026-09-27 CT 107 stopped answering for eight
        // minutes that way. An existing container with under 15% available
        // is refused before anything moves. A container that does not exist
        // yet answers nothing here and is not held back.
        let meminfo = exec
            .run(&Cmd::new(
                "pct",
                &["exec", &m.vmid.to_string(), "--", "cat", "/proc/meminfo"],
                20,
            ))
            .await;
        if let Ok(out) = meminfo
            && let Some(avail) = memory_available_fraction(&out.stdout)
            && avail < MIN_MEMORY_AVAILABLE
        {
            return Err(CoreError::SafetyAbort(format!(
                "{} has {:.0}% of its memory available, under the {:.0}% a deploy \
                         needs :: raise memory_mb in the stack file and run `homelab resize \
                         stacks/{}` first, then deploy",
                m.hostname,
                avail * 100.0,
                MIN_MEMORY_AVAILABLE * 100.0,
                m.stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    });
    if let Some(g) = &gpu {
        log_info(format!(
            "[w1] {} gid {} · {} gid {}",
            g.card, g.card_gid, g.render, g.render_gid
        ));
    }

    // ── Host-side /appdata storage (survives container recreation). ──────
    step!(runner, exec, ctx, m, "host storage", {
        for mount in &m.storage {
            run_ok(exec, &Cmd::new("mkdir", &["-p", &mount.host_path], 30)).await?;
            if let Some(uid) = mount.host_owner_uid {
                let owner = format!("{}:{}", uid, uid);
                run_ok(exec, &Cmd::new("chown", &[&owner, &mount.host_path], 60)).await?;
            }
        }
        Ok(if m.storage.is_empty() {
            StepOutcome::Unchanged
        } else {
            StepOutcome::Changed
        })
    });

    // ── E3/O6: every empty config dir is refilled from its own latest
    // snapshot BEFORE apps start. Per PATH, not per stack: the first version
    // restored only when EVERY declared path was empty, so wiping one app's
    // config while its siblings were intact restored nothing and said nothing
    // — from the stack's point of view there was nothing wrong. The Ansible
    // generation checked each service directory separately; this restores
    // that. Backup-target trouble degrades to a loud warning, never a blocked
    // deploy (spec: upgraded-by-Kenny Must).
    step!(runner, exec, ctx, m, "auto-restore check", {
        if m.storage.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let bcfg = ctx.backup.clone();
        let mut restored_any = false;
        let mut failed_any = false;
        // fix-147: which checks failed and which succeeded, recorded below.
        let mut check_failed: Vec<(String, String)> = Vec::new();
        let mut check_ok: Vec<String> = Vec::new();
        for mount in &m.storage {
            // An app that declares it keeps nothing is empty BY DESIGN, so
            // "empty, therefore restore it" is exactly the wrong conclusion
            // (F154, story: docs/deployment/REGISTER.md).
            if mount.no_data {
                continue;
            }
            // fix-146: a native unit's data is archived as one tar stream,
            // not by path, so this per-path question always answered "fresh"
            // for it. The `native units` step restores it instead.
            if m.natives.iter().any(|n| n == mount.owner(&m.stack_name)) {
                continue;
            }
            let probe = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!("ls -A {} 2>/dev/null | head -1", shq(&mount.host_path)),
                    ],
                    30,
                ))
                .await?;
            if !probe.stdout.trim().is_empty() {
                continue;
            }
            let owner = mount.owner(&m.stack_name);
            let has_snapshot = exec
                .run(&crate::ops::backup::restic_cmd(
                    &bcfg,
                    owner,
                    &["snapshots", "--last", "--json", "--path", &mount.host_path],
                    120,
                ))
                .await;
            // fix-54: only two answers mean "nothing to restore": an empty
            // snapshot list, and restic's own "repository does not exist"
            // (exit 10 since restic 0.17; pve runs 0.18). Every other
            // failure (Drive unreachable, an expired rclone token, a wrong
            // password, the 120 s timeout) is a loud warning; the deploy
            // still goes on, because backup-target trouble never blocks a
            // deploy (E3 spec). Story: docs/deployment/REGISTER.md.
            let listed = match &has_snapshot {
                Ok(out) if out.success() => Ok(out.stdout.trim().to_string()),
                Ok(out) if out.code == 10 => Ok(String::new()),
                Ok(out) => Err(format!(
                    "rc={} :: {}",
                    out.code,
                    crate::executor::trace_line(out.stderr.trim())
                )),
                Err(e) => Err(e.to_string()),
            };
            let listed = match listed {
                Ok(listed) => listed,
                Err(why) => {
                    failed_any = true;
                    check_failed.push((mount.host_path.clone(), why.clone()));
                    ctx.sink.emit(PipelineEvent::Line {
                        level: Level::Warn,
                        source: "HOST".into(),
                        msg: format!(
                            "[e3] {} is empty and its backup could not be checked ({}) — deploy continues with that dir EMPTY; if it held data, restore it by hand before the next nightly backup",
                            mount.host_path, why
                        ),
                    });
                    continue;
                }
            };
            check_ok.push(mount.host_path.clone());
            if listed.is_empty() || listed == "[]" {
                log_info(format!(
                    "[e3] {} is empty and has no snapshot — fresh",
                    mount.host_path
                ));
                continue;
            }
            log_info(format!(
                "[e3] {} is empty and a snapshot exists — restoring",
                mount.host_path
            ));
            let restored = exec
                .run(&crate::ops::backup::restic_cmd(
                    &bcfg,
                    owner,
                    &[
                        "restore",
                        "latest",
                        "--target",
                        "/",
                        "--path",
                        &mount.host_path,
                    ],
                    bcfg.restore_timeout_s,
                ))
                .await;
            match restored {
                Ok(o) if o.success() => restored_any = true,
                _ => {
                    failed_any = true;
                    ctx.sink.emit(PipelineEvent::Line {
                        level: Level::Warn,
                        source: "HOST".into(),
                        msg: format!(
                            "[e3] AUTO-RESTORE FAILED for {} — deploy continues with that dir EMPTY; restore it by hand if it held data",
                            mount.host_path
                        ),
                    });
                }
            }
        }
        if restored_any && !failed_any {
            log_info("[e3] auto-restore complete".into());
        }
        record_restore_checks(ctx, &m.stack_name, &check_failed, &check_ok).await;
        Ok(if restored_any {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    // ── T1: tell Prometheus this stack exists. Written before the apps
    // start, removed by destroy — so the scrape list is a consequence of what
    // runs rather than a list somebody maintains and forgets. Best-effort on
    // purpose: a metrics stack that is down must never block a deploy.
    step!(runner, exec, ctx, m, "metrics discovery", {
        let Some(dir) = ctx.metrics_targets_dir.as_deref() else {
            return Ok(StepOutcome::Unchanged);
        };
        let path = crate::ops::discovery::target_file(dir, &m.stack_name);
        // fix-145 (expert panel 2026-09-27, inbox-ct118-unmanaged): a native
        // stack whose every unit is declared unmeasured (`metrics: false`,
        // inbox under Kenny's "Grenzen zetten") runs no node exporter, so a
        // target here would be an endpoint nothing answers on and an alert
        // that never clears. The fleet check already respects the flag.
        let unmeasured = m.apps.is_empty()
            && crate::state::StateStore::new(exec, &ctx.state_dir)
                .load()
                .await
                .ok()
                .and_then(|s| s.stacks.get(&m.stack_name).cloned())
                .is_some_and(|st| {
                    !st.natives.is_empty() && st.natives.iter().all(|n| n.metrics == Some(false))
                });
        if unmeasured {
            log_info(format!(
                "[t1] {} is declared unmeasured (metrics: false) — no scrape target",
                m.stack_name
            ));
            return Ok(StepOutcome::Unchanged);
        }
        // A stack with apps runs docker; a native-service stack does not.
        let body =
            crate::ops::discovery::targets_json(&m.stack_name, &m.network.ip, !m.apps.is_empty());
        if matches!(exec.read_file(&path).await, Ok(existing) if existing == body) {
            return Ok(StepOutcome::Unchanged);
        }
        let _ = exec.run(&Cmd::new("mkdir", &["-p", dir], 30)).await;
        match exec.write_file(&path, &body, 0o644).await {
            Ok(()) => Ok(StepOutcome::Changed),
            Err(e) => {
                log_info(format!(
                    "[t1] could not write {} ({}) — this stack will not be scraped until it is",
                    path, e
                ));
                Ok(StepOutcome::Unchanged)
            }
        }
    });

    // ── fix-88: the container's own firewall (flat-vlan-no-east-west-control,
    // traefik-lan-host-header-bypass, 2026-09-27). Before the container is
    // created, so a new one starts behind its rules; written only when the
    // rendering differs from what pve holds, and the transcript names the
    // lines that came and went. `enabled: false` is a declaration kept for
    // the stack-by-stack rollout and touches nothing.
    let fw_enabled = m.firewall.as_ref().is_some_and(|f| f.enabled);
    if let Some(fwspec) = m.firewall.as_ref() {
        step!(runner, exec, ctx, m, "firewall", {
            if !fwspec.enabled {
                log_info(format!(
                    "[firewall] declared, not enabled — {} is left as it is",
                    crate::firewall::fw_path(m.vmid)
                ));
                return Ok(StepOutcome::Unchanged);
            }
            let path = crate::firewall::fw_path(m.vmid);
            // tile-watch (owner decision "Afgeleid uit de tegels",
            // 2026-09-30): a derived inbound rule for the dashboard's own
            // tile watch, appended before rendering so it is written, held
            // against pve and shown in every plan/diff the same way a
            // hand-declared rule is — and gone again the deploy after its
            // tile is.
            let effective = crate::firewall::with_tile_watch(
                fwspec,
                &m.network.ip,
                &m.tiles,
                ctx.tile_watch_source.as_deref().unwrap_or(""),
                &ctx.tile_watch_targets,
            );
            let body = crate::firewall::render(&m.stack_name, &effective);
            let old = exec.read_file(&path).await.ok();
            let mut changed = false;
            if old.as_deref() == Some(body.as_str()) {
                log_info(format!("[firewall] {} unchanged", path));
            } else {
                // 0640: pmxcfs refuses any other mode outside priv/.
                exec.write_file(&path, &body, 0o640).await?;
                let (added, removed) =
                    crate::firewall::line_changes(old.as_deref().unwrap_or(""), &body);
                log_info(format!(
                    "[firewall] {} written: +{} -{}{}",
                    path,
                    added.len(),
                    removed.len(),
                    if old.is_none() { " (new file)" } else { "" }
                ));
                for l in &added {
                    log_info(format!("[firewall]   + {}", l));
                }
                for l in &removed {
                    log_info(format!("[firewall]   - {}", l));
                }
                changed = true;
            }
            // Without `firewall=1` on the NIC Proxmox applies none of it.
            // CT 116's flag was set by hand; the create path below sets it
            // for a new container.
            if exists {
                let cfg = run_ok(exec, &Cmd::new("pct", &["config", &vm], 30)).await?;
                let net0 = cfg
                    .stdout
                    .lines()
                    .find_map(|l| l.strip_prefix("net0:"))
                    .map(str::trim);
                if let Some(new) = net0.and_then(crate::firewall::net0_with_firewall) {
                    run_ok(exec, &Cmd::new("pct", &["set", &vm, "--net0", &new], 60)).await?;
                    log_info(format!(
                        "[firewall] net0 of {} now carries firewall=1 — the rules apply from here",
                        m.vmid
                    ));
                    changed = true;
                }
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    } else {
        runner.skip("firewall");
    }

    // ── C1: create or reuse the container; C3 boot policy at create. ─────
    step!(runner, exec, ctx, m, "provision container", {
        if !exists {
            let mut net = format!(
                "name=eth0,bridge={},firewall={},ip={},gw={}",
                m.network.bridge,
                if fw_enabled { 1 } else { 0 },
                m.network.ip,
                m.network.gateway
            );
            if let Some(tag) = m.network.vlan {
                net.push_str(&format!(",tag={}", tag));
            }
            // B8: template "clone:<vmid>" provisions by cloning the golden
            // template container instead of a full create — seconds, not
            // minutes, because docker + guards are already baked in. The
            // bootstrap steps below still run and simply skip everything.
            if let Some(tpl_vmid) = m.lxc.template.strip_prefix("clone:") {
                // O5: `pct clone` has no --unprivileged; a clone always
                // inherits the template's privilege level. Asking for one the
                // template cannot give used to produce the other silently,
                // and an app that then fails on permissions gives no hint why.
                // CT 105 and 106 are privileged and must stay so.
                let tpl_cfg = exec
                    .run(&Cmd::new("pct", &["config", tpl_vmid], 30))
                    .await?;
                let tpl_unpriv = tpl_cfg
                    .stdout
                    .lines()
                    .find_map(|l| l.strip_prefix("unprivileged:"))
                    .map(|v| v.trim() == "1")
                    .unwrap_or(false);
                if tpl_unpriv != m.lxc.unprivileged {
                    return Err(CoreError::SafetyAbort(format!(
                        "template {} is {}, but stack '{}' asks for {} — pct clone cannot change this, it always inherits the template",
                        tpl_vmid,
                        if tpl_unpriv {
                            "unprivileged"
                        } else {
                            "privileged"
                        },
                        m.stack_name,
                        if m.lxc.unprivileged {
                            "unprivileged"
                        } else {
                            "privileged"
                        },
                    )));
                }
                run_ok(
                    exec,
                    &Cmd::new(
                        "pct",
                        &[
                            "clone",
                            tpl_vmid,
                            &vm,
                            "--hostname",
                            &m.hostname,
                            "--full",
                            "1",
                            "--storage",
                            &m.resources.storage,
                        ],
                        600,
                    ),
                )
                .await?;
                // Clones inherit the template's config — apply this stack's.
                let mem = m.resources.memory_mb.to_string();
                let swap = m.resources.swap_mb.to_string();
                let cores = m.resources.cores.to_string();
                let desc = format!("managed by homelab v2 :: stack {}", m.stack_name);
                let mut set_args: Vec<String> = vec![
                    "set".into(),
                    vm.clone(),
                    "--net0".into(),
                    net.clone(),
                    "--memory".into(),
                    mem,
                    "--swap".into(),
                    swap,
                    "--cores".into(),
                    cores,
                    "--onboot".into(),
                    if m.boot.onboot { "1" } else { "0" }.into(),
                    "--description".into(),
                    desc,
                    "--tags".into(),
                    "homelab".into(),
                    // T62: declared (default `host`), same as the `pct
                    // create` path.
                    "--timezone".into(),
                    m.lxc.timezone.clone(),
                ];
                if let Some(order) = m.boot.order {
                    set_args.push("--startup".into());
                    set_args.push(format!("order={}", order));
                }
                let refs: Vec<&str> = set_args.iter().map(|s| s.as_str()).collect();
                run_ok(exec, &Cmd::new("pct", &refs, 60)).await?;
                // Grow the rootfs to the requested size (template base is 4G).
                if m.resources.disk_gb > 4 {
                    let size = format!("{}G", m.resources.disk_gb);
                    run_ok(
                        exec,
                        &Cmd::new("pct", &["resize", &vm, "rootfs", &size], 120),
                    )
                    .await?;
                }
                for (i, mount) in m.storage.iter().enumerate() {
                    let mp = format!("-mp{}", i);
                    let val = format!("{},mp={}", mount.host_path, mount.mount_point);
                    run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
                }
                // M1: borrowed directories continue the same numbering.
                for (i, dm) in m.data_mounts.iter().enumerate() {
                    let mp = format!("-mp{}", m.storage.len() + i);
                    let val = format!("{},mp={}", dm.host_path, dm.mount_point);
                    run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
                }
                if let Some(g) = &gpu {
                    let (dev0, dev1) = crate::ops::hardware::dev_args(g);
                    run_ok(
                        exec,
                        &Cmd::new("pct", &["set", &vm, "--dev0", &dev0, "--dev1", &dev1], 60),
                    )
                    .await?;
                }
                if m.lxc.vpn {
                    let conf_path = format!("/etc/pve/lxc/{}.conf", m.vmid);
                    let conf = exec.read_file(&conf_path).await?;
                    if !conf.contains("dev/net/tun") {
                        let extra = "lxc.cgroup2.devices.allow: c 10:200 rwm\nlxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file\n";
                        exec.write_file(&conf_path, &format!("{}{}", conf, extra), 0o640)
                            .await?;
                    }
                }
                // Same rule as the create path below: protection only
                // after every drive change (see the comment there).
                if m.lxc.protection {
                    run_ok(
                        exec,
                        &Cmd::new("pct", &["set", &vm, "--protection", "1"], 60),
                    )
                    .await?;
                }
                run_ok(exec, &Cmd::new("pct", &["start", &vm], 120)).await?;
                created = true;
                return Ok(StepOutcome::Changed);
            }
            let rootfs = format!("{}:{}", m.resources.storage, m.resources.disk_gb);
            let mut args: Vec<String> = vec![
                "create".into(),
                vm.clone(),
                m.lxc.template.clone(),
                "--hostname".into(),
                m.hostname.clone(),
                "--rootfs".into(),
                rootfs,
                "--net0".into(),
                net,
                "--memory".into(),
                m.resources.memory_mb.to_string(),
                "--swap".into(),
                m.resources.swap_mb.to_string(),
                "--cores".into(),
                m.resources.cores.to_string(),
                "--unprivileged".into(),
                if m.lxc.unprivileged { "1" } else { "0" }.into(),
                "--features".into(),
                m.lxc.features.clone(),
                "--onboot".into(),
                if m.boot.onboot { "1" } else { "0" }.into(),
                // Managed containers are recognizable in the Proxmox UI.
                "--description".into(),
                format!("managed by homelab v2 :: stack {}", m.stack_name),
                "--tags".into(),
                "homelab".into(),
                // T62: declared (default `host`), not hardcoded — a stack
                // that needs a different zone can say so.
                "--timezone".into(),
                m.lxc.timezone.clone(),
            ];
            if let Some(order) = m.boot.order {
                args.push("--startup".into());
                args.push(format!("order={}", order));
            }
            let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ok(exec, &Cmd::new("pct", &refs, 300)).await?;

            for (i, mount) in m.storage.iter().enumerate() {
                let mp = format!("-mp{}", i);
                let val = format!("{},mp={}", mount.host_path, mount.mount_point);
                run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
            }
            // M1: borrowed directories continue the same numbering.
            for (i, dm) in m.data_mounts.iter().enumerate() {
                let mp = format!("-mp{}", m.storage.len() + i);
                let val = format!("{},mp={}", dm.host_path, dm.mount_point);
                run_ok(exec, &Cmd::new("pct", &["set", &vm, &mp, &val], 60)).await?;
            }
            // H4: hardware passthrough flags. GPU via pct dev entries
            // (targeted gids — render/video — NOT the old ansible
            // chmod-0777-recurse). TUN needs raw lxc config lines that pct
            // has no flag for, appended to the container config.
            if let Some(g) = &gpu {
                let (dev0, dev1) = crate::ops::hardware::dev_args(g);
                run_ok(
                    exec,
                    &Cmd::new("pct", &["set", &vm, "--dev0", &dev0, "--dev1", &dev1], 60),
                )
                .await?;
            }
            if m.lxc.vpn {
                let conf_path = format!("/etc/pve/lxc/{}.conf", m.vmid);
                let conf = exec.read_file(&conf_path).await?;
                if !conf.contains("dev/net/tun") {
                    let extra = "lxc.cgroup2.devices.allow: c 10:200 rwm\nlxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file\n";
                    exec.write_file(&conf_path, &format!("{}{}", conf, extra), 0o640)
                        .await?;
                }
            }
            created = true;
        }
        // A container that already existed has its MOUNTS compared with the
        // stack file too, not just its boot policy. Proxmox will not change a
        // drive while protection is on, so the flag comes off for exactly as
        // long as the writes take and goes straight back.
        //
        // The gap this closes was found the hard way: the downloader was
        // provisioned without its two data disks (F118), the mounts were put
        // back by hand, and a redeploy would happily have reported success
        // while leaving a hand-made fix as the only thing holding them there.
        // A repair that lives only outside the repo is not a repair.
        if !created {
            let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
            if cfg.success() {
                let mut want: Vec<(String, String)> = Vec::new();
                for (i, mo) in m.storage.iter().enumerate() {
                    want.push((
                        format!("-mp{}", i),
                        format!("{},mp={}", mo.host_path, mo.mount_point),
                    ));
                }
                for (i, dm) in m.data_mounts.iter().enumerate() {
                    want.push((
                        format!("-mp{}", m.storage.len() + i),
                        format!("{},mp={}", dm.host_path, dm.mount_point),
                    ));
                }
                // ask-8 (`Loskoppelen`): a mount the stack no longer declares
                // is detached. Which ones: an `mpN` the stack file does not
                // ask for whose host directory this stack declared before
                // (or declares now under another number — a duplicate left
                // by a renumbering). A mount no version of the stack file
                // ever named was attached by hand, and is not this deploy's
                // to take away. The host directory is never touched.
                let wanted_keys: std::collections::BTreeSet<String> = want
                    .iter()
                    .map(|(k, _)| k.trim_start_matches('-').to_string())
                    .collect();
                let mut ours: std::collections::BTreeSet<String> = m
                    .storage
                    .iter()
                    .map(|s| s.host_path.clone())
                    .chain(m.data_mounts.iter().map(|d| d.host_path.clone()))
                    .collect();
                if let Some(pm) = prior_manifest.as_ref() {
                    ours.extend(pm.storage.iter().map(|s| s.host_path.clone()));
                    ours.extend(pm.data_mounts.iter().map(|d| d.host_path.clone()));
                }
                let mut stale: Vec<(String, String)> = Vec::new();
                for line in cfg.stdout.lines() {
                    let Some((key, val)) = line.trim().split_once(':') else {
                        continue;
                    };
                    let is_mp = key
                        .strip_prefix("mp")
                        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
                    if !is_mp || wanted_keys.contains(key) {
                        continue;
                    }
                    let host_path = val.trim().split(',').next().unwrap_or("").to_string();
                    if ours.contains(&host_path) {
                        stale.push((key.to_string(), host_path));
                    } else {
                        log_info(format!(
                            "[mounts] {} ({}) is in no version of this stack file — attached by \
                             hand, left alone",
                            key, host_path
                        ));
                    }
                }
                let missing: Vec<(String, String)> = want
                    .into_iter()
                    .filter(|(key, val)| {
                        let line = format!("{}: {}", key.trim_start_matches('-'), val);
                        !cfg.stdout.lines().any(|l| l.trim() == line)
                    })
                    .collect();
                if !missing.is_empty() || !stale.is_empty() {
                    let protected = cfg.stdout.lines().any(|l| l.trim() == "protection: 1");
                    if protected {
                        run_ok(
                            exec,
                            &Cmd::new("pct", &["set", &vm, "--protection", "0"], 60),
                        )
                        .await?;
                    }
                    // Detached first, so a renumbered mount never sits on two
                    // keys at once.
                    for (key, host_path) in &stale {
                        run_ok(exec, &Cmd::new("pct", &["set", &vm, "--delete", key], 60)).await?;
                        log_info(format!(
                            "[mounts] {} ({}) is no longer declared — detached; the directory \
                             on the host is kept",
                            key, host_path
                        ));
                    }
                    for (key, val) in &missing {
                        log_info(format!("[mounts] {} was not attached — {}", key, val));
                        run_ok(exec, &Cmd::new("pct", &["set", &vm, key, val], 60)).await?;
                    }
                    if protected {
                        run_ok(
                            exec,
                            &Cmd::new("pct", &["set", &vm, "--protection", "1"], 60),
                        )
                        .await?;
                    }
                    // A mount only appears inside a running container after a
                    // restart, so saying so is part of doing it.
                    log_info(
                        "[mounts] changed — a running container sees it after a reboot".into(),
                    );
                }
            }
        }

        // W3: a container that already existed has its boot policy compared
        // with the stack file and put back. Set at creation and never looked
        // at again means that after a power cut the fleet boots in the order
        // somebody typed years ago — and the rule that everything behind the
        // edge waits for Traefik lives only in a file nothing reads.
        // Resources are deliberately NOT touched here: raising them is
        // `homelab resize`, lowering them is a rebuild, and neither belongs
        // in an ordinary deploy. The fleet check reports those instead.
        if !created {
            let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
            if cfg.success() {
                let live = crate::ops::reconcile::parse(&cfg.stdout);
                let args = crate::ops::reconcile::boot_set_args(m, &live);
                if !args.is_empty() {
                    log_info(format!(
                        "[w3] boot policy drifted — {}",
                        crate::ops::reconcile::divergences(m, &live).join("; ")
                    ));
                    let mut argv: Vec<&str> = vec!["set", &vm];
                    argv.extend(args.iter().map(|a| a.as_str()));
                    run_ok(exec, &Cmd::new("pct", &argv, 60)).await?;
                }
            }
        }

        // The protection flag is intent like any other. It was set only on
        // the run that CREATED the container, so a container whose flag had
        // been turned off — by hand, or by an operation that lifted it and
        // did not put it back — stayed unprotected and nothing said so. Found
        // immediately after the mount reconciliation above: that step lifts
        // the flag to do its work, and the deploy that followed left it off.
        if !created && m.lxc.protection {
            let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
            if cfg.success() && !cfg.stdout.lines().any(|l| l.trim() == "protection: 1") {
                log_info("[protection] was off — the stack file asks for it".into());
                run_ok(
                    exec,
                    &Cmd::new("pct", &["set", &vm, "--protection", "1"], 60),
                )
                .await?;
            }
        }

        // Protection is deliberately the LAST provisioning act: Proxmox
        // refuses drive changes ("can't update CT ... drive 'mp0' -
        // protection mode enabled") once the flag is set, so it must land
        // after the rootfs resize and every mountpoint. Live-found on the
        // first protected stack with a bind mount (metrics, 2026-08-29).
        // Gated destroy (C2) lifts it deliberately before removal.
        if created && m.lxc.protection {
            run_ok(
                exec,
                &Cmd::new("pct", &["set", &vm, "--protection", "1"], 60),
            )
            .await?;
        }
        let status = run_ok(exec, &Cmd::new("pct", &["status", &vm], 30)).await?;
        if !status.stdout.contains("running") {
            run_ok(exec, &Cmd::new("pct", &["start", &vm], 120)).await?;
            return Ok(StepOutcome::Changed);
        }
        Ok(if created {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    // fix-154: Proxmox drops a vmid's firewall file when it creates that
    // vmid, so the file written above, before the container existed, is
    // gone once `pct clone` has made it. A new container gets the file
    // written again, now that it exists. Story: docs/deployment/REGISTER.md.
    if !exists && let Some(fwspec) = m.firewall.as_ref().filter(|f| f.enabled) {
        step!(runner, exec, ctx, m, "firewall after create", {
            let path = crate::firewall::fw_path(m.vmid);
            let effective = crate::firewall::with_tile_watch(
                fwspec,
                &m.network.ip,
                &m.tiles,
                ctx.tile_watch_source.as_deref().unwrap_or(""),
                &ctx.tile_watch_targets,
            );
            let body = crate::firewall::render(&m.stack_name, &effective);
            // Unconditionally: what pmxcfs reports for a vmid it has just
            // created is not something to compare against.
            exec.write_file(&path, &body, 0o640).await?;
            log_info(format!(
                "[firewall] {} written again after the container was created (creating a \
                     vmid removes its firewall file)",
                path
            ));
            Ok(StepOutcome::Changed)
        });
    } else {
        runner.skip("firewall after create");
    }

    // ── tile-watch-watcher-out (found 2026-10-01): this stack's own tiles
    // may have just changed what the WATCHER (`tile_watch_source`) needs to
    // reach outbound — a tile that arrived or left here changes the derived
    // OUT rule on a container this deploy never touches otherwise. Folding
    // this stack's fresh `m.tiles` into the fleet targets read a moment
    // earlier (`ctx.tile_watch_targets`, from `HostState`) means the
    // watcher's file is correct even though its own deploy has not run.
    // Skipped when there is no watcher, or this deploy IS the watcher (its
    // own "firewall" step above already derived its OUT rule from the same
    // fleet targets).
    if let (Some(watcher), Some(source)) = (
        ctx.tile_watch_watcher.as_ref(),
        ctx.tile_watch_source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    ) && watcher.stack != m.stack_name
    {
        step!(
            runner,
            exec,
            ctx,
            m,
            "refresh the tile watcher's firewall",
            {
                let fresh_targets = crate::firewall::with_fresh_target(
                    &ctx.tile_watch_targets,
                    &m.network.ip,
                    &m.tiles,
                    true,
                );
                let effective = crate::firewall::with_tile_watch(
                    &watcher.firewall,
                    &watcher.own_ip,
                    &watcher.tiles,
                    source,
                    &fresh_targets,
                );
                let path = crate::firewall::fw_path(watcher.vmid);
                let body = crate::firewall::render(&watcher.stack, &effective);
                let old = exec.read_file(&path).await.ok();
                if old.as_deref() == Some(body.as_str()) {
                    log_info(format!(
                        "[firewall] tile watcher {} ({}) unchanged",
                        path, watcher.stack
                    ));
                    return Ok(StepOutcome::Unchanged);
                }
                exec.write_file(&path, &body, 0o640).await?;
                let (added, removed) =
                    crate::firewall::line_changes(old.as_deref().unwrap_or(""), &body);
                log_info(format!(
                    "[firewall] tile watcher {} ({}) refreshed by this deploy's tiles: +{} -{}",
                    path,
                    watcher.stack,
                    added.len(),
                    removed.len()
                ));
                Ok(StepOutcome::Changed)
            }
        );
    } else {
        runner.skip("refresh the tile watcher's firewall");
    }

    step!(runner, exec, ctx, m, "wait for systemd", {
        let mut ready = false;
        for _ in 0..30 {
            let out = pct_sh(
                exec,
                m.vmid,
                "systemctl is-system-running 2>/dev/null || true",
                20,
            )
            .await?;
            let s = out.stdout.trim();
            if s.contains("running") || s.contains("degraded") {
                ready = true;
                break;
            }
            exec.sleep_ms(4000).await;
        }
        if !ready {
            return Err(CoreError::Other(
                "container never reached running/degraded".into(),
            ));
        }
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, exec, ctx, m, "bootstrap docker", {
        // A native-only container has no docker and must not be given any:
        // installing it would change the very thing this manifest exists to
        // reproduce exactly.
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        let probe = pct_sh(exec, m.vmid, "docker --version", 30).await?;
        if probe.success() {
            return Ok(StepOutcome::Unchanged);
        }
        let install = pct_sh(
            exec,
            m.vmid,
            "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq curl ca-certificates",
            600,
        )
        .await?;
        if !install.success() {
            return Err(CoreError::Command {
                rendered: "apt-get install prerequisites".into(),
                detail: install.stderr,
            });
        }
        let docker = pct_sh(exec, m.vmid, "curl -fsSL https://get.docker.com | sh", 900).await?;
        if !docker.success() {
            return Err(CoreError::Command {
                rendered: "get.docker.com".into(),
                detail: docker.stderr,
            });
        }
        pct_sh(exec, m.vmid, "systemctl enable --now docker", 120).await?;
        Ok(StepOutcome::Changed)
    });

    // ── B2 + A7. ─────────────────────────────────────────────────────────
    step!(runner, exec, ctx, m, "runaway guards", {
        guards::apply(
            exec,
            ctx.sink,
            m.vmid,
            !m.native_only,
            ctx.registry_cache.as_ref(),
        )
        .await?;
        Ok(StepOutcome::Unchanged)
    });

    // ── fix-24: rotation for logs the stack writes onto a data mount. ────
    step!(runner, exec, ctx, m, "log rotation", {
        if guards::apply_rotation(
            exec,
            m.vmid,
            &m.stack_name,
            &m.data_mounts,
            ctx.default_log_rotation.as_ref(),
        )
        .await?
        {
            Ok(StepOutcome::Changed)
        } else {
            Ok(StepOutcome::Unchanged)
        }
    });

    // ── step-22 / ask-8: what left the stack's files leaves the container.
    //
    // The intent repo's copy of the stack is the record of what the last
    // deploy placed, so it is read BEFORE this deploy overwrites it: a file it
    // has and the stack no longer has is a file the stack dropped. Two kinds
    // are acted on here, before anything later in the deploy can fail and
    // leave the record already rewritten:
    //
    // * a `rootfs/` file (a unit, a timer, a script on PATH) is removed from
    //   the container — a unit or timer is disabled and stopped first, and
    //   systemd reloads afterwards (`Automatisch bij deploy`, replacing D84's
    //   report-then-prune);
    // * a native unit dropped from `natives:` is stopped, disabled, its unit
    //   file and program removed (`Stoppen, data bewaren`) — its data dirs
    //   inside the container and its restic repository are kept.
    //
    // Everything else the copy has that the stack does not is removed from
    // the repo by the commit step below, so `git log` records the removal.
    let repo_stack_dir = format!("{}/repo/stacks/{}", ctx.state_dir, m.stack_name);
    let mut dropped_repo_files: Vec<String> = Vec::new();
    // Vault copies of each retired unit, for its retired record (ask-9).
    let mut unit_vault: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    // fix-199: an older client's `DeploySpec` may have no field at all for
    // something a newer one added, so an empty/absent `files` would read as
    // "none declared, remove the rest" rather than "this client cannot even
    // say" (`client/src/link.rs`'s 2026-08-31 incident). `client_knows`
    // gates the whole diff on it; a client too old to know this field skips
    // the step rather than retiring what it cannot declare.
    let files_removal = manifest::client_knows(spec.client_schema, "files");
    if !files_removal {
        log_info(format!(
            "[retire dropped] skipped — the deploying client (schema {}) predates this \
             declared-files list; nothing was retired on its behalf",
            spec.client_schema
        ));
    }
    if files_removal {
        step!(runner, exec, ctx, m, "retire dropped", {
            let listing = exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!(
                        "cd {} 2>/dev/null && find . -type f -printf '%P\\n' 2>/dev/null || true",
                        crate::ops::util::shq(&repo_stack_dir)
                    ),
                ],
                60,
            ))
            .await?;
            let declared: std::collections::BTreeSet<&str> =
                spec.files.iter().map(|f| f.path.as_str()).collect();
            dropped_repo_files = listing
                .stdout
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !declared.contains(*l))
                .map(str::to_string)
                .collect();
            let mut changed = false;
            let mut reload = false;
            for rel in &dropped_repo_files {
                if !rel.starts_with(manifest::ROOTFS_PREFIX) {
                    continue;
                }
                // A path the validator would refuse today was never placed.
                let Ok((dest, _)) = manifest::file_destination(&m.stack_name, rel) else {
                    continue;
                };
                if let Some(name) = dest.strip_prefix("/etc/systemd/system/") {
                    reload = true;
                    let is_unit = !name.contains('/')
                        && [".service", ".timer", ".socket", ".path"]
                            .iter()
                            .any(|x| name.ends_with(x));
                    if is_unit {
                        pct_sh(
                            exec,
                            m.vmid,
                            &format!(
                                "systemctl disable --now {} 2>&1 || true",
                                crate::ops::util::shq(name)
                            ),
                            120,
                        )
                        .await?;
                    }
                }
                run_ok(
                    exec,
                    &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &dest], 60),
                )
                .await?;
                log_info(format!(
                    "[orphans] removed {} — the stack no longer places it",
                    dest
                ));
                changed = true;
            }
            for unit in &dropped_natives {
                // The program's path: the registration knows it; the unit file
                // the last deploy placed says it too; the convention otherwise.
                let old_unit = exec
                    .read_file(&format!("{}/{}/{}.service", repo_stack_dir, unit, unit))
                    .await
                    .ok();
                let need = old_unit.as_deref().map(crate::native::unit_prereqs);
                let binary = prior
                    .as_ref()
                    .and_then(|p| p.natives.iter().find(|n| &n.unit == unit))
                    .map(|n| n.binary.clone())
                    .filter(|b| !b.is_empty())
                    .or_else(|| need.as_ref().and_then(|n| n.binary.clone()))
                    .unwrap_or_else(|| format!("/usr/local/bin/{}", unit));
                let mut vault: Vec<String> = need
                    .as_ref()
                    .map(|n| {
                        n.env_files
                            .iter()
                            .chain(n.credentials.iter())
                            .map(|f| {
                                format!(
                                    "{}/secrets/{}/{}",
                                    ctx.state_dir,
                                    m.stack_name,
                                    vault_key(f)
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(env) = prior
                    .as_ref()
                    .and_then(|p| p.natives.iter().find(|n| &n.unit == unit))
                    .and_then(|n| n.env_file.clone())
                {
                    vault.push(format!(
                        "{}/secrets/{}/{}",
                        ctx.state_dir,
                        m.stack_name,
                        vault_key(&env)
                    ));
                }
                unit_vault.insert(unit.clone(), vault);
                pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "systemctl disable --now {} 2>&1 || true",
                        crate::ops::util::shq(unit)
                    ),
                    120,
                )
                .await?;
                log_info(format!(
                    "[native] {} left the stack file — stopped and disabled",
                    unit
                ));
                for path in [format!("/etc/systemd/system/{}.service", unit), binary] {
                    run_ok(
                        exec,
                        &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &path], 60),
                    )
                    .await?;
                    log_info(format!("[native] removed {}", path));
                }
                log_info(format!(
                    "[native] {}'s data directories and its restic repository are kept",
                    unit
                ));
                reload = true;
                changed = true;
            }
            if reload {
                pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    } else {
        runner.skip("retire dropped");
    }

    // ── D4: intent into the host-local git repo (never secrets, A5). ─────
    step!(runner, exec, ctx, m, "commit intent", {
        let repo = format!("{}/repo", ctx.state_dir);
        let stack_dir = format!("{}/stacks/{}", repo, m.stack_name);
        // step-22: the copy holds what the stack declares and nothing else —
        // a file the stack dropped used to stay here forever.
        for rel in &dropped_repo_files {
            run_ok(
                exec,
                &Cmd::new("rm", &["-f", &format!("{}/{}", stack_dir, rel)], 30),
            )
            .await?;
        }
        if !dropped_repo_files.is_empty() {
            let _ = exec
                .run(&Cmd::new(
                    "find",
                    &[
                        &stack_dir,
                        "-mindepth",
                        "1",
                        "-type",
                        "d",
                        "-empty",
                        "-delete",
                    ],
                    30,
                ))
                .await;
        }
        for f in &spec.files {
            exec.write_file(&format!("{}/{}", stack_dir, f.path), &f.content, 0o644)
                .await?;
        }
        let git_check = exec
            .run(&Cmd::new(
                "git",
                &["-C", &repo, "rev-parse", "--git-dir"],
                20,
            ))
            .await?;
        if !git_check.success() {
            run_ok(exec, &Cmd::new("git", &["-C", &repo, "init", "-q"], 30)).await?;
            run_ok(
                exec,
                &Cmd::new(
                    "git",
                    &["-C", &repo, "config", "user.email", "host@homelab.local"],
                    20,
                ),
            )
            .await?;
            run_ok(
                exec,
                &Cmd::new(
                    "git",
                    &["-C", &repo, "config", "user.name", "homelab-host"],
                    20,
                ),
            )
            .await?;
        }
        run_ok(exec, &Cmd::new("git", &["-C", &repo, "add", "-A"], 30)).await?;
        // fix-141: names which commit went live and whether files in the
        // stack directory were uncommitted. Story: docs/deployment/REGISTER.md.
        let msg = match spec.source.as_ref() {
            Some(src) => format!(
                "deploy {} ({})\n\n{}",
                m.stack_name,
                src.summary(),
                src.commit_body()
            ),
            None => format!(
                "deploy {}\n\nsource not reported (the client is too old to report it)\n",
                m.stack_name
            ),
        };
        let commit = exec
            .run(&Cmd::new(
                "git",
                &["-C", &repo, "commit", "-q", "-m", &msg],
                30,
            ))
            .await?;
        if commit.success() {
            return Ok(StepOutcome::Changed);
        }
        // Hardening H15: ONLY the benign no-op case may pass silently. Any
        // other commit failure (dubious ownership, index lock, missing
        // identity) previously made every deploy green while the intent
        // history — the whole rollback + mirror story — stayed empty.
        let noise = format!("{}{}", commit.stdout, commit.stderr);
        if noise.contains("nothing to commit")
            || noise.contains("nothing added to commit")
            || noise.contains("no changes added")
        {
            Ok(StepOutcome::Unchanged)
        } else {
            Err(CoreError::Command {
                rendered: "git commit (intent history)".into(),
                detail: format!(
                    "intent commit failed — history/rollback/mirror would silently stop: {}",
                    noise.trim()
                ),
            })
        }
    });

    // ── D1: push files; env over the secrets channel (A5). ───────────────
    //
    // Which apps had a config file change is remembered, because pushing a
    // file is not the same as the service reading it. `docker compose up -d`
    // sees an unchanged compose definition and leaves the container running,
    // so an edit to a bind-mounted config takes effect at the next unrelated
    // restart — or never. On 2026-08-31 that cost a wrong conclusion: promtail
    // ran four more minutes on the old pipeline and the first verification
    // reported the fix as not working.
    let needs_restart: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
    let recreated: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
    let needs_restart_w = needs_restart.clone();
    let recreated_w = recreated.clone();
    // S2: every file this deploy actually wrote into the container, with the
    // hash it should now have. Checked once at the end of the step — a push
    // that reports success and lands somewhere else, or on top of something
    // else, is the shape F124 took.
    let pushed: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let pushed_w = pushed.clone();
    // fix-142 (nightly hash comparison, Kenny's go 2026-10-01): sha256 of
    // EVERY non-rootfs file this deploy sends, by its manifest path
    // ("<app>/<file>"), whether or not `push_content` found it changed —
    // an unchanged file is still ground truth for what should be sitting in
    // the container tonight. Kept apart from `pushed` on purpose: that one
    // exists to drive restarts and only needs the files that moved.
    let all_file_hashes: std::sync::Arc<
        std::sync::Mutex<std::collections::BTreeMap<String, String>>,
    > = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new()));
    let all_file_hashes_w = all_file_hashes.clone();
    // latch-files: the secret files whose content this deploy changed, for
    // the native step's restarts. Apart from `pushed` on purpose: that list
    // is hashed back from the container at the end of the step, and a
    // secret's hash is not something to echo.
    let secrets_changed: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let secrets_changed_w = secrets_changed.clone();
    // F129: what each rewritten app's compose said BEFORE it was pointed at
    // the cache. Kept so the pull step can put the real registry back when
    // the cache turns out not to serve — the fallback half of Kenny's C1.
    let cache_orig: CacheOriginals =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new()));
    let cache_orig_w = cache_orig.clone();
    stepv!(
        runner,
        exec,
        ctx,
        m,
        "push files",
        {
            let mut rootfs_changed: Vec<String> = Vec::new();
            for f in &spec.files {
                // A native unit file goes to /etc/systemd/system and nowhere
                // else. Pushing it here too cost almanac its binary: the stack
                // file's path is `<unit>/<unit>.service`, which lands on
                // /opt/almanac/almanac — and that WAS the binary. The garbage
                // collector removed it, the push then made a directory of the
                // same name, and the service kept running only because the
                // kernel holds a deleted file open. A restart would have found
                // nothing there.
                if m.natives
                    .iter()
                    .any(|u| f.path == format!("{}/{}.service", u, u))
                {
                    continue;
                }
                // ask-2: a `rootfs/` file lands at its absolute path (a unit,
                // a timer, a script on PATH); everything else under
                // /opt/<stack>/. Validated before the first push, so an
                // `Err` here cannot happen — it is mapped rather than
                // unwrapped so a future gap is a refusal, not a panic.
                let (dest, default_mode) = manifest::file_destination(&m.stack_name, &f.path)
                    .map_err(CoreError::Validation)?;
                let rootfs = f.path.starts_with(manifest::ROOTFS_PREFIX);
                let perms = format!("{:o}", f.mode.unwrap_or(default_mode));
                // D60: the file in the repository names the real origin; what
                // lands in the container names the cache, but only for the
                // upstreams that answered a moment ago and never for a registry
                // this stack signs into — that one is private, and the cache is
                // anonymous by design.
                let content = match (
                    ctx.registry_cache.as_ref(),
                    f.path.ends_with("docker-compose.yml"),
                ) {
                    (Some(cache), true) if !cache_up.is_empty() => {
                        let rewritten = crate::ops::registry_cache::rewrite_compose(
                            &f.content,
                            cache,
                            &cache_up,
                            m.registry_login.as_ref().map(|r| r.registry.as_str()),
                        );
                        // Only a compose the rewrite actually touched can fall
                        // back; recording the untouched ones would make the pull
                        // step re-push identical bytes for every app in the stack.
                        if rewritten != f.content
                            && let Some((app, _)) = f.path.split_once('/')
                            && let Ok(mut g) = cache_orig_w.lock()
                        {
                            g.insert(
                                app.to_string(),
                                (dest.clone(), f.content.clone(), perms.clone()),
                            );
                        }
                        rewritten
                    }
                    _ => f.content.clone(),
                };
                // fix-142: recorded unconditionally, before `changed` is
                // even known, and keyed by `f.path` (not `dest`) so it reads
                // the same as `evaluate_repo_drift`'s `StackDigest::files` —
                // both are "<app>/<file>" against /opt/<stack>.
                if !rootfs && let Ok(mut g) = all_file_hashes_w.lock() {
                    g.insert(f.path.clone(), manifest::sha256_hex(content.as_bytes()));
                }
                let changed = push_content(exec, m.vmid, &dest, &content, &perms).await?;
                if changed {
                    if let Ok(mut g) = pushed_w.lock() {
                        g.push((dest.clone(), manifest::sha256_hex(content.as_bytes())));
                    }
                    if rootfs {
                        rootfs_changed.push(dest.clone());
                    }
                    // The path is "<app>/<file>"; a file outside an app directory
                    // belongs to no service and needs nothing restarted. A
                    // rootfs file is not an app either: "rootfs" must never
                    // reach the restart step as a service name.
                    if let Some((app, name)) = f.path.split_once('/').filter(|_| !rootfs) {
                        if name == "docker-compose.yml" {
                            // compose up -d recreates this one by itself.
                            if let Ok(mut g) = recreated_w.lock() {
                                g.insert(app.to_string());
                            }
                        } else if let Ok(mut g) = needs_restart_w.lock() {
                            g.insert(app.to_string());
                        }
                    }
                }
                ctx.sink.emit(PipelineEvent::Bytes {
                    op: op.clone(),
                    label: dest,
                    done: f.content.len() as u64,
                    total: Some(f.content.len() as u64),
                });
            }
            // ask-2: systemd only reads a unit it has been told about, and a
            // timer file on disk fires nothing until it is enabled. A changed
            // unit or timer reloads the manager; a changed timer is enabled
            // and started — starting a TIMER is not starting a service.
            // Nothing here touches a `.service` beyond the reload; a native
            // unit whose drop-in or env file changed here is restarted by
            // the "native units" step (fix-159), health-checked.
            if rootfs_changed
                .iter()
                .any(|d| d.starts_with("/etc/systemd/system/"))
            {
                pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
                log_info("[rootfs] systemd reloaded for the unit files written above".to_string());
            }
            for dest in &rootfs_changed {
                if let Some(timer) = dest
                    .strip_prefix("/etc/systemd/system/")
                    .filter(|n| n.ends_with(".timer"))
                {
                    let out = pct_sh(
                        exec,
                        m.vmid,
                        &format!("systemctl enable --now {}", timer),
                        60,
                    )
                    .await?;
                    if !out.success() {
                        return Err(CoreError::Command {
                            rendered: format!("systemctl enable --now {}", timer),
                            detail: out.stderr.trim().to_string(),
                        });
                    }
                    log_info(format!("[rootfs] {} enabled and started", timer));
                }
            }
            for (app, env) in &spec.env {
                let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
                push_content(exec, m.vmid, &dest, env, "600").await?;
                // HOST-side vault copy for redeploys — outside the git repo.
                let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
                exec.write_file(&vault, env, 0o600).await?;
                log_info(format!("[vault] {} sealed (values not logged)", dest));
            }
            // latch-files (2026-09-30): secret files from latch, at their
            // absolute path, then sealed into the vault under the same key a
            // native unit's env file uses, so the native step's vault copy and
            // this one are one file. Values never logged.
            for f in &spec.secret_files {
                let changed = push_content(exec, m.vmid, &f.path, &f.content, &f.mode).await?;
                if let Some(owner) = &f.owner {
                    run_ok(
                        exec,
                        &Cmd::new("pct", &["exec", &vm, "--", "chown", owner, &f.path], 60),
                    )
                    .await?;
                }
                let vault = format!(
                    "{}/secrets/{}/{}",
                    ctx.state_dir,
                    m.stack_name,
                    vault_key(&f.path)
                );
                exec.write_file(&vault, &f.content, 0o600).await?;
                if changed {
                    if let Ok(mut g) = secrets_changed_w.lock() {
                        g.push(f.path.clone());
                    }
                    log_info(format!(
                        "[latch] {} written and sealed (values not logged)",
                        f.path
                    ));
                }
            }
            // A5/E3: apps whose env the client did NOT send fall back to the
            // vault — a wiped container gets its .env back on redeploy.
            for app in &m.apps {
                if spec.env.contains_key(app) {
                    continue;
                }
                let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
                let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
                if let Ok(env) = exec.read_file(&vault).await {
                    push_content(exec, m.vmid, &dest, &env, "600").await?;
                    log_info(format!("[vault] {} restored from vault", dest));
                } else {
                    // gap-27: an .env that exists only on the container (no
                    // client copy, no vault copy) is sealed into the vault,
                    // so a lost container gets it back. Read, never echoed.
                    let got = crate::executor::pct_sh_secret(
                        exec,
                        m.vmid,
                        &format!("cat {} 2>/dev/null || true", crate::ops::util::shq(&dest)),
                        60,
                    )
                    .await?;
                    if !got.stdout.trim().is_empty() {
                        exec.write_file(&vault, &got.stdout, 0o600).await?;
                        log_info(format!(
                            "[vault] {} sealed from the container (values not logged)",
                            dest
                        ));
                    }
                }
            }
            Ok(StepOutcome::Changed)
        },
        {
            // Ask the container what those files now hash to. One command for all
            // of them, because a round trip per file is what makes a check like
            // this get switched off later. `.env` files are deliberately absent
            // from the list: their content is not ours to echo back, and the
            // vault copy is the record that matters for them.
            let want = pushed.lock().map(|g| g.clone()).unwrap_or_default();
            if want.is_empty() {
                return Ok(());
            }
            let paths = want
                .iter()
                .map(|(p, _)| shq(p))
                .collect::<Vec<_>>()
                .join(" ");
            let out = pct_sh(exec, m.vmid, &format!("sha256sum {} 2>&1", paths), 120)
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let mut bad: Vec<String> = Vec::new();
            for (path, hash) in &want {
                let line = out.lines().find(|l| l.trim_end().ends_with(path.as_str()));
                match line {
                    Some(l) if l.split_whitespace().next() == Some(hash.as_str()) => {}
                    Some(_) => bad.push(format!("{} has different content", path)),
                    None => bad.push(format!("{} is not there", path)),
                }
            }
            if bad.is_empty() {
                Ok(())
            } else {
                Err(bad.join("; "))
            }
        }
    );

    step!(runner, exec, ctx, m, "start apps", {
        // A native-only container has no docker: nothing to network, nothing
        // to start. Without this the deploy created an empty docker network
        // on CT 112 — harmless, and exactly the kind of stray act that makes
        // a reader wonder what else it did.
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        // Sign in to a private registry before anything tries to pull from
        // it. The credentials ride in an app's ordinary .env, already pushed
        // above, so this adds no new secrets path — it only stops the login
        // from living in somebody's memory instead of in the manifest.
        //
        // Read with grep rather than by sourcing the file: a token with a
        // parenthesis or a space in it breaks `.` in dash, and that failure
        // looks like a wrong password.
        if let Some(reg) = &m.registry_login {
            let envf = format!("/opt/{}/{}/.env", m.stack_name, reg.app);
            let script = format!(
                "u=$(grep -m1 '^REGISTRY_USER=' {f} | cut -d= -f2-); \
                 t=$(grep -m1 '^REGISTRY_TOKEN=' {f} | cut -d= -f2-); \
                 [ -n \"$u\" ] && [ -n \"$t\" ] || {{ echo no REGISTRY_USER/REGISTRY_TOKEN in {f} >&2; exit 1; }}; \
                 printf %s \"$t\" | docker login {r} -u \"$u\" --password-stdin",
                f = shq(&envf),
                // Validated as a bare host name (`registry_login.registry`).
                r = reg.registry
            );
            let out = pct_sh(exec, m.vmid, &script, 60).await?;
            if !out.success() {
                return Err(CoreError::Command {
                    rendered: format!("docker login {}", reg.registry),
                    detail: out.stderr,
                });
            }
            log_info(format!(
                "[registry] signed in to {} as the credentials in {} say",
                reg.registry, reg.app
            ));
        }
        let net = format!("{}_net", m.stack_name);
        pct_sh(
            exec,
            m.vmid,
            &format!("docker network create {} 2>/dev/null || true", net),
            60,
        )
        .await?;
        for app in &m.apps {
            let dir = shq(&format!("/opt/{}/{}", m.stack_name, app));
            // F129 · the fallback half of C1. An app whose compose was
            // pointed at the cache gets a bounded first attempt; anything
            // else keeps the full step budget, because then there is nowhere
            // to fall back TO and cutting it short would only turn a slow
            // registry into a failed deploy.
            let fallback = cache_orig.lock().ok().and_then(|g| g.get(app).cloned());
            let budget = match (&fallback, ctx.registry_cache.as_ref()) {
                (Some(_), Some(c)) => c.pull_timeout_secs,
                _ => 900,
            };
            let cmd = format!("cd {} && docker compose pull -q", dir);
            // gap-12: a deploy fetches only what the container does not
            // have. Updating stays where the rollback is; a deploy deploys.
            // Story: docs/deployment/REGISTER.md.
            //
            // The container is asked per image the compose file names (after
            // the cache rewrite, so the name asked about is the name `up -d`
            // will start). Standing rule 12: no usable answer means pull —
            // the old behaviour — never "assume it is there".
            let present = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd {} && for i in $(docker compose config --images 2>/dev/null); do \
                     docker image inspect \"$i\" >/dev/null 2>&1 && echo \"present $i\" \
                     || echo \"missing $i\"; done",
                    dir
                ),
                60,
            )
            .await;
            let all_present = match &present {
                Ok(o) if o.success() => {
                    let lines: Vec<&str> = o.stdout.lines().map(str::trim).collect();
                    !lines.is_empty() && lines.iter().all(|l| l.starts_with("present "))
                }
                _ => false,
            };
            if all_present {
                log_info(format!(
                    "[images] {} :: every image is already on the container — not pulled \
                     (a deploy never replaces an image; `homelab update` does)",
                    app
                ));
            }
            // Not `?`: a timeout is an Err by the Executor contract, and a
            // timeout is precisely the case the fallback exists for. Taking
            // the error here would step over it.
            let first = if all_present {
                Ok(crate::executor::CmdOutput::ok(""))
            } else {
                pct_sh(exec, m.vmid, &cmd, budget).await
            };
            let mut pull = match first {
                Ok(o) if o.success() => o,
                other => {
                    if fallback.is_none() {
                        // Nothing to fall back to — the original behaviour,
                        // error and all.
                        let o = other?;
                        return Err(CoreError::Command {
                            rendered: format!("compose pull {}", app),
                            detail: o.stderr,
                        });
                    }
                    match other {
                        Ok(o) => o,
                        Err(e) => crate::executor::CmdOutput {
                            code: 1,
                            stdout: String::new(),
                            stderr: e.to_string(),
                        },
                    }
                }
            };
            if !pull.success() {
                // The cache did not deliver. Put the real registry back in
                // the file and pull that — and LEAVE it there, so the
                // `up -d` two lines down starts the image we actually
                // fetched instead of one the cache still cannot serve.
                if let Some((dest, original, perms)) = fallback {
                    ctx.sink.emit(PipelineEvent::Line {
                            level: Level::Warn,
                            source: "HOST".into(),
                            msg: format!(
                                "[cache] {} did not deliver within {}s — falling back to its own registry",
                                app, budget
                            ),
                        });
                    push_content(exec, m.vmid, &dest, &original, &perms).await?;
                    // A half-written layer from the abandoned attempt
                    // makes the direct pull fail on a digest mismatch,
                    // which reads like the image itself is broken. It is
                    // not: it is the truncated blob the cache left behind.
                    pct_sh(exec, m.vmid, "docker system prune -f", 300).await?;
                    pull = pct_sh(exec, m.vmid, &cmd, 900).await?;
                }
            }
            if !pull.success() {
                return Err(CoreError::Command {
                    rendered: format!("compose pull {}", app),
                    detail: pull.stderr,
                });
            }
            let up = pct_sh(
                exec,
                m.vmid,
                &format!("cd {} && docker compose up -d --remove-orphans", dir),
                300,
            )
            .await?;
            if !up.success() {
                return Err(CoreError::Command {
                    rendered: format!("compose up {}", app),
                    detail: up.stderr,
                });
            }
            // A config file changed under an app whose compose definition did
            // not: `up -d` left the container alone, so the running process is
            // still reading the old file. Restart is enough — the file is
            // bind-mounted, so the new content is already visible inside.
            // fix-40: a changed compose file is not proof the container was
            // recreated (a comment-only change recreates nothing). Only
            // compose's own "Recreated"/"Created" line skips the restart.
            let compose_recreated = recreated.lock().map(|g| g.contains(app)).unwrap_or(false)
                && compose_reports_created(&format!("{}\n{}", up.stdout, up.stderr));
            // fix-40, the invariant: a container that started before a
            // file in its app directory last changed runs an older copy of
            // that file, whether or not this deploy pushed it (a restart that
            // an earlier deploy missed is caught here too).
            let older_than_files = if compose_recreated {
                false
            } else {
                match pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "cd {} && for c in $(docker compose ps -q); do \
                         s=$(docker inspect -f '{{{{.State.StartedAt}}}}' \"$c\"); \
                         find . -maxdepth 3 -type f -newermt \"$s\" -print -quit | grep -q . \
                         && echo stale; done; true",
                        dir
                    ),
                    60,
                )
                .await
                {
                    Ok(o) if o.success() => o.stdout.contains("stale"),
                    // shell-strings-quoting (2026-09-27), standing rule 12: a
                    // probe that could not answer is not "not older". A
                    // restart is the safe side of not knowing.
                    Ok(o) => {
                        ctx.sink.emit(PipelineEvent::Line {
                            level: Level::Warn,
                            source: "HOST".into(),
                            msg: format!(
                                "[config] {} :: whether it runs older files could not be \
                                 read (rc={}) — restarting it to be sure",
                                app, o.code
                            ),
                        });
                        true
                    }
                    Err(e) => {
                        ctx.sink.emit(PipelineEvent::Line {
                            level: Level::Warn,
                            source: "HOST".into(),
                            msg: format!(
                                "[config] {} :: whether it runs older files could not be \
                                 read ({}) — restarting it to be sure",
                                app, e
                            ),
                        });
                        true
                    }
                }
            };
            let restart_this = (needs_restart
                .lock()
                .map(|g| g.contains(app))
                .unwrap_or(false)
                || older_than_files)
                && !compose_recreated;
            if restart_this {
                let r = pct_sh(
                    exec,
                    m.vmid,
                    &format!("cd {} && docker compose restart", dir),
                    300,
                )
                .await?;
                if !r.success() {
                    return Err(CoreError::Command {
                        rendered: format!("compose restart {}", app),
                        detail: r.stderr,
                    });
                }
                log_info(format!(
                    "[config] {} restarted — its config changed and compose would not have",
                    app
                ));
            }
        }
        Ok(StepOutcome::Changed)
    });

    // ── B3: no green light without proof. ────────────────────────────────
    // ── Storage ownership: can the app actually write its own data?
    //
    // This is the fifth version of this step in one afternoon, and the first
    // one that asks the right question. The four before it tried to work out
    // WHO SHOULD own the directory, and each was confidently wrong on a shape
    // nobody had thought of — an empty user field with PUID, the name
    // "nobody", an entrypoint that drops privileges after starting, and a
    // supervisor whose PID 1 is root while the application runs beside it as
    // a child. Every wrong answer was printed as a `chown` to copy, and each
    // one would have broken the service it was about.
    //
    // The question that has no shapes is whether the application can write.
    // It is also the only thing anyone actually cares about: Loki did not
    // crash because a number was wrong, it crashed because it could not open
    // its own database.
    //
    // So the container is asked to write one file and delete it again. No
    // uid arithmetic, no image conventions, no fifth special case — and when
    // it fails, the message reports what IS rather than prescribing a fix
    // that might be as wrong as the last four.
    step!(runner, exec, ctx, m, "storage ownership", {
        let mut wrong: Vec<String> = Vec::new();
        for mount in &m.storage {
            let Some(app) = mount.app.as_ref() else {
                continue;
            };
            if !m.apps.contains(app) {
                continue;
            }
            // Where does this host path appear inside the app's container?
            // Docker's own mapping, so no path convention has to be guessed.
            let dest = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "docker inspect {} -f '{{{{range .Mounts}}}}{{{{if eq .Source \"{}\"}}}}{{{{.Destination}}}}{{{{end}}}}{{{{end}}}}' 2>/dev/null || true",
                    app, mount.mount_point
                ),
                60,
            )
            .await
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
            if dest.is_empty() {
                // The app does not mount this directory. It belongs to a
                // sibling, or the container is not running — either way there
                // is nothing here to judge.
                continue;
            }
            // T81 (F289): establish that the probe CAN run before reading
            // anything into its result. A container that is restarting
            // fails `docker exec` before it reaches the directory, and the
            // first JobTracker deploy reported that as "cannot write … the
            // directory belongs to 110001" — a hypothesis printed as a
            // finding. Not running is reported as unmeasured, never as a
            // permissions fault.
            let status = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "docker inspect {} -f '{{{{.State.Status}}}}' 2>/dev/null || echo unknown",
                    app
                ),
                30,
            )
            .await
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_else(|_| "unknown".into());
            if status != "running" {
                ctx.sink.emit(PipelineEvent::Line {
                    level: Level::Warn,
                    source: "HOST".into(),
                    msg: format!(
                        "[storage] '{}' is {} — whether it can write {} could not be \
                         measured, so nothing about its ownership is claimed",
                        app,
                        if status.is_empty() {
                            "in an unknown state"
                        } else {
                            status.as_str()
                        },
                        mount.host_path
                    ),
                });
                continue;
            }
            let probe = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "docker exec {} sh -c 'touch {}/.homelab-write-probe && rm -f {}/.homelab-write-probe' 2>&1",
                    app, dest, dest
                ),
                120,
            )
            .await;
            // A probe that cannot RUN says nothing about the data. The
            // cloudflared image is distroless and has no shell at all, so the
            // exec fails before it ever reaches the directory — reporting
            // that as "cannot write" is the same mistake as counting a 403 as
            // healthy, in the opposite direction. Same rule as the service
            // checks: unreadable is reported, never treated as failure.
            let out = match &probe {
                Ok(o) => format!("{}{}", o.stdout, o.stderr),
                Err(e) => e.to_string(),
            };
            let unprobeable = out.contains("executable file not found")
                || out.contains("OCI runtime exec failed")
                || out.contains("no such file or directory: unknown")
                || out.contains("is restarting")
                || out.contains("is not running")
                || out.contains("is paused");
            if unprobeable {
                ctx.sink.emit(PipelineEvent::Line {
                    level: Level::Warn,
                    source: "HOST".into(),
                    msg: format!(
                        "[storage] '{}' has no shell to probe with, so whether it \
                         can write {} is unknown — not assumed to be fine",
                        app, mount.host_path
                    ),
                });
                continue;
            }
            let failed = match &probe {
                Ok(o) => !o.success(),
                Err(_) => true,
            };
            if failed {
                let owner = run_ok(
                    exec,
                    &Cmd::new("stat", &["-c", "%u:%g", &mount.host_path], 30),
                )
                .await
                .map(|o| o.stdout.trim().to_string())
                .unwrap_or_else(|_| "unreadable".into());
                wrong.push(format!(
                    "'{}' cannot write to {} (mounted at {} inside it); the \
                     directory belongs to {} on the host — compare that with \
                     the user the application runs as, which is not always \
                     the container's PID 1",
                    app, mount.host_path, dest, owner
                ));
            }
        }
        if wrong.is_empty() {
            Ok(StepOutcome::Unchanged)
        } else {
            Err(CoreError::Command {
                rendered: format!("storage ownership {}", m.stack_name),
                detail: wrong.join("; "),
            })
        }
    });

    step!(runner, exec, ctx, m, "verify health", {
        exec.sleep_ms(5000).await;
        for app in &m.apps {
            // fix-132/fix-133: `docker compose ps --format json` through the
            // Unknown-carrying parser, instead of matching the plain-text
            // table (`--status running --services`) and reading an empty
            // answer as "nothing running" — which is also what a probe that
            // could not even ask compose looks like.
            let running =
                crate::ops::util::compose_running_services(exec, m.vmid, &m.stack_name, app, "")
                    .await?;
            let reason = match &running {
                Some(r) if !r.is_empty() => None,
                Some(_) => Some("no running services".to_string()),
                None => Some("could not read `docker compose ps` for this app".to_string()),
            };
            if let Some(reason) = reason {
                let dir = shq(&format!("/opt/{}/{}", m.stack_name, app));
                let diag = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "cd {} && docker compose ps -a && docker compose logs --tail 20",
                        dir
                    ),
                    60,
                )
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
                return Err(CoreError::Command {
                    rendered: format!("verify {}", app),
                    detail: format!("{}\n{}", reason, diag),
                });
            }
            log_info(format!("[gate] {} :: running", app));
        }
        Ok(StepOutcome::Unchanged)
    });

    // ── H1: the single allowed cross-stack write. ────────────────────────
    // fix-91 (routes-outside-repo-unvalidated, 2026-09-27): the stack's
    // `extra_routes` go the same way, under the name each already has on the
    // gateway, so a file brought into the repository is rewritten with the
    // bytes it already holds.
    let routes: Vec<&manifest::GatewayRoute> = spec
        .gateway_route
        .iter()
        .chain(spec.extra_routes.iter())
        .collect();
    if !routes.is_empty() {
        step!(runner, exec, ctx, m, "gateway route", {
            let mut changed = false;
            for route in &routes {
                let dest =
                    safety::check_gateway_route(&ctx.safety, route.gateway_vmid, &route.filename)?;
                // H6 hardening: when the routes dir lives under /appdata/ it is a
                // HOST path bind-mounted into the gateway — write it host-side so
                // route fragments survive gateway recreation. The legacy /opt
                // path keeps the pct-push behavior until the platform migration.
                if ctx.safety.gateway_routes_dir.starts_with("/appdata/") {
                    exec.write_file(&dest, &route.content, 0o644).await?;
                    changed = true;
                } else if push_content(exec, route.gateway_vmid, &dest, &route.content, "644")
                    .await?
                {
                    changed = true;
                }
                log_info(format!("[route] {} (file-provider watch reloads)", dest));
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    } else {
        runner.skip("gateway route");
    }

    // ── step-22: a route the stack no longer declares goes. The file is
    // named `<vmid>-app-<stack>.yml`, so dropping `gateway_route:` or moving
    // to another vmid left the old one behind: still routing a hostname to a
    // container that is gone or no longer this stack. Only when the stack had
    // a record: a stack deployed for the first time has nothing of its own to
    // take away.
    // fix-41: only the route file a deploy of this stack recorded writing is
    // retired, when the stack no longer declares it or now writes another
    // name (a changed vmid). A route written by hand on the gateway is never
    // this stack's to remove.
    // fix-91: the recorded `extra_routes` files follow the same rule — every
    // file this stack's last deploy recorded and this deploy does not write.
    // fix-199: same field-keeping rule as "retire dropped" — an older
    // client's spec may simply have no route this stack always declared,
    // because its struct predates routes being removable at all; its
    // absence must not read as "retire every route this stack ever wrote".
    let routes_removal = manifest::client_knows(spec.client_schema, "routes");
    if !routes_removal {
        log_info(format!(
            "[retire gateway route] skipped — the deploying client (schema {}) predates this \
             declared-routes list; nothing was retired on its behalf",
            spec.client_schema
        ));
    }
    let declared: Vec<&str> = routes.iter().map(|r| r.filename.as_str()).collect();
    let stale_routes: Vec<String> = if routes_removal {
        prior
            .as_ref()
            .map(|p| {
                p.route_file
                    .iter()
                    .chain(p.extra_route_files.iter())
                    .filter(|f| !declared.contains(&f.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !stale_routes.is_empty() {
        step!(runner, exec, ctx, m, "retire gateway route", {
            let mut changed = false;
            for f in &stale_routes {
                let dest = format!("{}/{}", ctx.safety.gateway_routes_dir, f);
                let q = crate::ops::util::shq(&dest);
                // Same machine and path destroy removes it from.
                let out = pct_sh(
                    exec,
                    ctx.safety.gateway_vmid,
                    &format!("if [ -f {q} ]; then rm -f {q} && echo removed; fi"),
                    30,
                )
                .await?;
                if out.stdout.contains("removed") {
                    log_info(format!(
                        "[route] {} removed — the stack file no longer declares it",
                        dest
                    ));
                    changed = true;
                }
            }
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    } else {
        runner.skip("retire gateway route");
    }

    // ── D3: garbage-collect apps removed from intent — stop + remove their
    // compose project and /opt dir; /appdata config dirs are kept.
    //
    // Files the container has and the repository does not. The garbage
    // collection below works per APP DIRECTORY; a FILE that leaves an app
    // used to take nothing with it — thirteen Grafana dashboards deleted from
    // the repository on 2026-09-01 were still on CT 104 two deploys later,
    // one with the same uid as its replacement (F161).
    //
    // D84 reported them and left the removing to `homelab prune-orphans`.
    // Kenny's ask-8 (`Automatisch bij deploy`, 2026-09-27) reverses that:
    // what the files no longer declare, the deploy removes — one transcript
    // line per file, so the removal is as visible as the report was. `.env`
    // files come from the vault, never from the repository, and directories
    // the orchestrator fills on behalf of other stacks (the gateway's
    // generated dashboards and routes) are not this stack's files at all.
    // fix-199: same field-keeping rule — a client whose manifest schema
    // predates the stack's `apps:` list being diffable for departures must
    // not have every app it cannot name treated as "left the stack".
    let apps_removal = manifest::client_knows(spec.client_schema, "apps");
    if !apps_removal {
        log_info(format!(
            "[orphan files] skipped — the deploying client (schema {}) predates the \
             declared-apps list; nothing was retired on its behalf",
            spec.client_schema
        ));
    }
    step!(runner, exec, ctx, m, "orphan files", {
        if m.native_only || !apps_removal {
            return Ok(StepOutcome::Unchanged);
        }
        let keep = generated_dirs(&ctx.safety, m.vmid);
        // An app that left the stack is the garbage collector's below: its
        // compose file has to still be there when `docker compose down` runs
        // in its directory, or its containers keep running with nothing left
        // to stop them by.
        let leaving: Vec<&String> = prior
            .as_ref()
            .map(|p| p.apps.iter().filter(|a| !m.apps.contains(a)).collect())
            .unwrap_or_default();
        let orphans: Vec<String> = orphan_files_keeping(exec, m, spec, &keep)
            .await
            .into_iter()
            .filter(|o| {
                let app = o.split('/').next().unwrap_or("");
                !leaving.iter().any(|a| a.as_str() == app)
            })
            .collect();
        if orphans.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for o in &orphans {
            let path = format!("/opt/{}/{}", m.stack_name, o);
            // -f, never -r: FILES the repository dropped. A directory would
            // take whatever else is under it.
            run_ok(
                exec,
                &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &path], 60),
            )
            .await?;
            log_info(format!(
                "[orphans] removed {} — no longer in the stack's files",
                path
            ));
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, exec, ctx, m, "garbage collect", {
        // Nor is there anything to garbage-collect: an app directory under
        // /opt on a native-only container was never put there by a deploy.
        // fix-199: nor while the deploying client predates the declared-apps
        // list (`apps_removal`, above) — the same field-keeping rule.
        if m.native_only || !apps_removal {
            return Ok(StepOutcome::Unchanged);
        }
        let store = StateStore::new(exec, &ctx.state_dir);
        let state = store.load().await?;
        let removed: Vec<String> = state
            .stacks
            .get(&m.stack_name)
            .map(|s| {
                s.apps
                    .iter()
                    .filter(|a| !m.apps.contains(a))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if removed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for app in &removed {
            // shell-strings-quoting (2026-09-27): the exit code was thrown
            // away and "removed" logged regardless. It is `rm`'s (the last
            // command); a failure fails the step, which leaves the app on
            // record so the next deploy tries again.
            let out = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd {d} && docker compose down --remove-orphans; rm -rf {d}",
                    d = shq(&format!("/opt/{}/{}", m.stack_name, app))
                ),
                300,
            )
            .await?;
            if !out.success() {
                return Err(CoreError::Command {
                    rendered: format!("remove /opt/{}/{}", m.stack_name, app),
                    detail: format!("rc={} :: {}", out.code, out.stderr.trim()),
                });
            }
            log_info(format!("[gc] app '{}' removed (config dirs kept)", app));
        }
        Ok(StepOutcome::Changed)
    });

    // Kenny's N1 (2026-08-31): a native service gets the same cycle a docker
    // app gets. The unit file comes from the repository — it did not exist
    // there before, so if CT 109 had been lost, nobody would have had the
    // four files that make its services exist.
    //
    // Deliberately gentle: the unit is written and reloaded, and the service
    // is started only if it is not already running. Adoption's rule holds —
    // a running production service is not restarted to take ownership of it.
    // Changing a unit file that is already in place is a deliberate act, not
    // a side effect of a deploy.
    // C1/C2 (Kenny, 2026-09-02): one log shipper per container, and it is
    // Alloy, because promtail reached end of life on 2026-03-02.
    //
    // Runs for EVERY stack, native or compose. The old arrangement gave a
    // promtail sidecar to stacks that run containers and nothing at all to
    // the two that do not — so kyu, the hub every notification travels
    // through, shipped no logs for as long as it has existed (F245).
    if let Some(loki) = ctx.loki_url.as_deref() {
        step!(runner, exec, ctx, m, "log shipper", {
            let out = pct_sh(exec, m.vmid, &crate::ops::logshipper::install_script(), 900).await?;
            if !out.success() {
                // Not fatal: a stack that cannot install a log shipper is
                // still a stack that should come up. Silence about it would
                // be the fault this whole migration is about, so it is loud.
                log_warn(format!(
                    "[logs] Alloy could not be installed on {} ({}) — this container \
                     is shipping NO logs until that is fixed",
                    m.hostname,
                    out.stderr.trim()
                ));
                return Ok(StepOutcome::Unchanged);
            }
            let fresh = out.stdout.contains("installed") && !out.stdout.contains("already");
            let body = crate::ops::logshipper::config(
                &m.stack_name,
                &m.hostname,
                loki,
                &m.syslog_receivers,
                &m.log_files,
            );
            let changed = push_content(
                exec,
                m.vmid,
                crate::ops::logshipper::CONFIG_PATH,
                &body,
                "644",
            )
            .await?;
            // gap-11: Alloy reads ONE file, the one just pushed. A directory
            // mode left behind by hand, or a stray `.alloy` beside the
            // rendered one, is undone here and said out loud — every line the
            // script prints is a change the operator did not make through
            // the repository. Before the restart, because with the receiver
            // rendered above a stray copy of it would make Alloy refuse to
            // start on a duplicate component.
            let swept = pct_sh(
                exec,
                m.vmid,
                &crate::ops::logshipper::single_file_mode_script(),
                60,
            )
            .await?;
            let mut drift = false;
            for line in swept
                .stdout
                .lines()
                .filter(|l| !l.trim().is_empty() && *l != "done")
            {
                drift = true;
                log_warn(format!(
                    "[logs] Alloy on {} was not reading the rendered file alone: {}",
                    m.hostname, line
                ));
            }
            // Every deploy, not only when the config moved: the read access
            // Alloy needs for docker's 0710 directory was missing on every
            // container for 24 days while their configs never changed
            // (expert panel, container-logs-missing-in-loki).
            let perms = pct_sh(
                exec,
                m.vmid,
                &crate::ops::logshipper::permissions_script(),
                60,
            )
            .await?;
            let access = perms
                .stdout
                .contains(crate::ops::logshipper::DROPIN_WRITTEN);
            if access {
                log_info(format!(
                    "[logs] gave Alloy read access to the container logs on {}",
                    m.hostname
                ));
            }
            let changed = changed || drift || access;
            if fresh || changed {
                let r = pct_sh(
                    exec,
                    m.vmid,
                    "systemctl enable alloy >/dev/null 2>&1; systemctl restart alloy",
                    120,
                )
                .await?;
                if !r.success() {
                    log_warn(format!(
                        "[logs] Alloy did not start on {} ({})",
                        m.hostname,
                        r.stderr.trim()
                    ));
                    return Ok(StepOutcome::Unchanged);
                }
                // "It started" is not "it is shipping". The first live run
                // of this step printed exactly that line while every batch
                // was dropped with a 404 and nothing reached Loki, so the
                // step now asks Alloy what it managed to deliver.
                exec.sleep_ms(12_000).await;
                let metrics = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "curl -s -m 5 {} 2>/dev/null || true",
                        crate::ops::logshipper::METRICS_URL
                    ),
                    30,
                )
                .await?;
                match crate::ops::logshipper::delivery(&metrics.stdout) {
                    crate::ops::logshipper::Delivery::Shipping { sent } => log_info(format!(
                        "[logs] Alloy delivered {} bytes to Loki for {}",
                        sent, m.stack_name
                    )),
                    crate::ops::logshipper::Delivery::Dropping { dropped } => log_warn(format!(
                        "[logs] Alloy is DROPPING {}'s logs ({} bytes) — Loki refused them; \
                         check loki_url in host.toml and `journalctl -u alloy` on {}",
                        m.stack_name, dropped, m.hostname
                    )),
                    crate::ops::logshipper::Delivery::Quiet => log_info(format!(
                        "[logs] Alloy started on {} and has shipped nothing yet — normal on a \
                         quiet container, wrong on a busy one; check Loki for stack=\"{}\"",
                        m.hostname, m.stack_name
                    )),
                    crate::ops::logshipper::Delivery::Unknown(why) => log_warn(format!(
                        "[logs] Alloy on {} could not be asked whether it is shipping ({})",
                        m.hostname, why
                    )),
                }
            }
            // Asked on every deploy: "Alloy is running" said nothing for 24
            // days while it could not open a single container log.
            let probe = pct_sh(
                exec,
                m.vmid,
                &crate::ops::logshipper::readability_script(),
                30,
            )
            .await?;
            match crate::ops::logshipper::readability(&probe.stdout) {
                crate::ops::logshipper::Readability::Denied => log_warn(format!(
                    "[logs] Alloy on {} cannot read /var/lib/docker/containers — NO container \
                     log from {} reaches Loki; check {} and `systemctl status alloy`",
                    m.hostname,
                    m.stack_name,
                    crate::ops::logshipper::READ_DROPIN_PATH
                )),
                crate::ops::logshipper::Readability::Unknown(why) => log_warn(format!(
                    "[logs] could not ask whether Alloy on {} can read the container logs ({})",
                    m.hostname, why
                )),
                _ => {}
            }
            Ok(if fresh || changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    } else {
        runner.skip("log shipper");
    }

    // A5 (Kenny, 2026-09-02): prepare, then start — in that order.
    //
    // The G13 drill measured what the old order did: it ran
    // `systemctl enable --now kyu` on a container where /usr/local/bin was
    // empty, the user did not exist and the env file was not there, and
    // systemd tried thirteen times before giving up. That worked everywhere
    // else only because every native container had been built by hand and
    // adopted afterwards — which means a lost one could not be rebuilt.
    step!(runner, exec, ctx, m, "native units", {
        if m.natives.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut changed = false;
        // fix-159: what the push step wrote into the container this run; a
        // running unit whose drop-in or env file is among it restarts.
        let mut pushed_paths: Vec<String> = pushed
            .lock()
            .map(|g| g.iter().map(|(p, _)| p.clone()).collect())
            .unwrap_or_default();
        // latch-files: a changed secret file counts as written too, so a unit
        // reading it with EnvironmentFile= restarts (fix-159).
        let secret_paths: Vec<String> = secrets_changed
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        pushed_paths.extend(secret_paths.iter().cloned());
        // fix-37: file names two or more units read. A flat vault copy
        // written by an older deploy under such a name belongs to nobody in
        // particular and is never restored.
        let mut seen: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for unit in &m.natives {
            if let Some(blob) = spec
                .files
                .iter()
                .find(|f| f.path == format!("{}/{}.service", unit, unit))
            {
                let need = crate::native::unit_prereqs(&blob.content);
                for f in need.env_files.iter().chain(need.credentials.iter()) {
                    seen.entry(legacy_vault_key(f))
                        .or_default()
                        .insert(f.clone());
                }
            }
        }
        let ambiguous = |f: &str| {
            seen.get(&legacy_vault_key(f))
                .map(|paths| paths.len() > 1)
                .unwrap_or(false)
        };
        for unit in &m.natives {
            let Some(blob) = spec
                .files
                .iter()
                .find(|f| f.path == format!("{}/{}.service", unit, unit))
            else {
                return Err(CoreError::SafetyAbort(format!(
                    "stack declares native unit '{}' but {}/{}.service is not in the stack \
                     directory :: without it a rebuild cannot recreate the service, which is \
                     the whole reason the unit file was brought into the repository",
                    unit, unit, unit
                )));
            };
            let dest = format!("/etc/systemd/system/{}.service", unit);
            // fix-159: every file this unit reads that the deploy wrote.
            let mut written = pushed_paths.clone();
            if push_content(exec, m.vmid, &dest, &blob.content, "644").await? {
                log_info(format!("[native] {} written", dest));
                pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
                written.push(dest.clone());
                changed = true;
            }

            let need = crate::native::unit_prereqs(&blob.content);

            // 1 · the account systemd will run it as.
            if let Some(user) = &need.user {
                let has = pct_sh(exec, m.vmid, &format!("id -u {} 2>/dev/null", user), 30).await?;
                if has.stdout.trim().is_empty() {
                    log_info(format!("[native] creating system user {}", user));
                    pct_sh(
                        exec,
                        m.vmid,
                        &format!(
                            "useradd --system --no-create-home --shell /usr/sbin/nologin {}",
                            crate::ops::util::shq(user)
                        ),
                        60,
                    )
                    .await?;
                    changed = true;
                }
            }

            // 2 · the program. Staged by the client from the service's own
            // release; absent when the stack has no release_repo or GitHub
            // could not be reached, and that is reported rather than fatal.
            if let Some(b64) = spec.native_binaries.get(unit) {
                let path = need
                    .binary
                    .clone()
                    .unwrap_or_else(|| format!("/usr/local/bin/{}", unit));
                // fix-28: a deploy puts a program where there is none; it
                // never replaces one. The client stages the NEWEST release,
                // so replacing here upgraded every native on every deploy,
                // whatever its update_policy — an upgrade is the updater's
                // job, which reads the policy and checks the signature.
                let present = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "test -f {} && echo yes || true",
                        crate::ops::util::shq(&path)
                    ),
                    30,
                )
                .await?;
                let b64 = if present.stdout.trim() == "yes" {
                    log_info(format!(
                        "[native] {} already installed, not shipped — upgrades go through the \
                         nightly updater or `homelab release-update-native`",
                        path
                    ));
                    ""
                } else {
                    b64.as_str()
                };
                let b64_path = format!("{}.homelab-b64", path);
                // Staged beside the target, then moved: a transfer that dies
                // half way must never leave a truncated program in place.
                if !b64.is_empty() && push_content(exec, m.vmid, &b64_path, b64, "600").await? {
                    let script = format!(
                        "base64 -d {b} > {p}.new && chmod 755 {p}.new && rm -f {b} \
                         && test -s {p}.new && mv {p}.new {p}",
                        b = crate::ops::util::shq(&b64_path),
                        p = crate::ops::util::shq(&path)
                    );
                    let out = pct_sh(exec, m.vmid, &script, 300).await?;
                    if !out.success() {
                        return Err(CoreError::Other(format!(
                            "could not place {} on the container ({}) — nothing was replaced",
                            path,
                            out.stderr.trim()
                        )));
                    }
                    log_info(format!("[native] {} installed", path));
                    changed = true;
                }
            }

            // stale-path-binary (2026-09-30): a unit whose program lives
            // elsewhere (/opt/kyu/bin/kyu) left an old copy at
            // /usr/local/bin/<unit> from before the move; CT 109 ran kyu
            // 4.1.0 while `kyu` on PATH answered 2.4.1. That copy becomes a
            // link to the real program, so the name on PATH is always the
            // version that runs. A link, not a removal: scripts may call it.
            let path_copy = format!("/usr/local/bin/{}", unit);
            if let Some(real) = need.binary.as_deref().filter(|b| *b != path_copy) {
                let out = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "if [ -f {p} ] && [ ! -L {p} ] && [ -x {r} ]; then \
                         ln -sfn {r} {p} && echo linked; fi",
                        p = shq(&path_copy),
                        r = shq(real)
                    ),
                    30,
                )
                .await?;
                if out.stdout.trim() == "linked" {
                    log_info(format!(
                        "[native] {} was a stale copy; now a link to {}",
                        path_copy, real
                    ));
                    changed = true;
                }
            }

            // 3 · the files systemd reads before the first line of the
            // program runs. They cannot be invented: what the vault holds
            // from an earlier deploy is restored, and anything else is named.
            let mut missing: Vec<String> = Vec::new();
            for f in need.env_files.iter().chain(need.credentials.iter()) {
                let there = pct_sh(
                    exec,
                    m.vmid,
                    &format!("test -s {} && echo yes || true", crate::ops::util::shq(f)),
                    30,
                )
                .await?;
                if there.stdout.trim() == "yes" {
                    continue;
                }
                let vault = format!(
                    "{}/secrets/{}/{}",
                    ctx.state_dir,
                    m.stack_name,
                    vault_key(f)
                );
                let legacy = format!(
                    "{}/secrets/{}/{}",
                    ctx.state_dir,
                    m.stack_name,
                    legacy_vault_key(f)
                );
                let from_vault = match exec.read_file(&vault).await {
                    Ok(c) if !c.trim().is_empty() => Ok(c),
                    _ if !ambiguous(f) => exec.read_file(&legacy).await,
                    other => other,
                };
                match from_vault {
                    Ok(content) if !content.trim().is_empty() => {
                        push_content(exec, m.vmid, f, &content, "600").await?;
                        log_info(format!("[native] {} restored from the vault", f));
                        written.push(f.clone());
                        changed = true;
                    }
                    _ => missing.push(f.clone()),
                }
            }

            // 4 · does the program exist at all?
            if let Some(bin) = &need.binary {
                let there = pct_sh(
                    exec,
                    m.vmid,
                    &format!("test -x {} && echo yes || true", crate::ops::util::shq(bin)),
                    30,
                )
                .await?;
                if there.stdout.trim() != "yes" {
                    missing.push(bin.clone());
                }
            }

            // 5 · only now. Starting a unit whose prerequisites are absent
            // produces a restart loop and an error about "resources" that
            // says nothing about the actual cause.
            if !missing.is_empty() {
                log_warn(format!(
                    "[native] {} NOT started — these are missing on the container: {}. \
                     A binary comes from `homelab install-native stacks/{}/{}`; an env or \
                     credential file has never been on this host and cannot be invented.",
                    unit,
                    missing.join(", "),
                    m.stack_name,
                    unit
                ));
                continue;
            }

            let active = pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
            let running = active.stdout.trim() == "active";
            // fix-146 (native-empty-rebuild, 2026-09-27): a unit that is not
            // running and whose data is empty gets its newest snapshot back
            // before it starts. A running unit is never written under.
            if !running && let Some(nm) = spec.native_manifests.get(unit) {
                use crate::ops::native::EmptyUnit;
                let found = crate::ops::native::restore_empty_unit(exec, &ctx.backup, nm).await?;
                // fix-147: a check that could not be made is remembered
                // until a later one of the same unit succeeds.
                match &found {
                    EmptyUnit::HasData => {}
                    EmptyUnit::CheckFailed(why) => {
                        record_restore_checks(
                            ctx,
                            &m.stack_name,
                            &[(unit.clone(), why.clone())],
                            &[],
                        )
                        .await
                    }
                    _ => {
                        record_restore_checks(ctx, &m.stack_name, &[], std::slice::from_ref(unit))
                            .await
                    }
                }
                match found {
                    EmptyUnit::HasData => {}
                    EmptyUnit::Fresh => log_info(format!(
                        "[native] {}: data is empty and has no snapshot — fresh",
                        unit
                    )),
                    EmptyUnit::Restored => {
                        log_info(format!(
                            "[native] {}: data was empty — the newest snapshot is back",
                            unit
                        ));
                        changed = true;
                    }
                    EmptyUnit::RestoredHold(why) => {
                        log_warn(format!(
                            "[native] {} NOT started: its data was empty and the newest \
                                 snapshot was unpacked, but {}",
                            unit, why
                        ));
                        changed = true;
                        continue;
                    }
                    EmptyUnit::CheckFailed(why) => log_warn(format!(
                        "[native] {}: data is empty and its backup could not be checked \
                             ({}) — starting it EMPTY; if it held data, restore it by hand \
                             (op-11) before the next nightly backup",
                        unit, why
                    )),
                    EmptyUnit::RestoreFailed(why) => {
                        log_warn(format!(
                            "[native] AUTO-RESTORE FAILED for {} ({}) — NOT started, so it \
                                 does not begin again empty; restore it by hand (op-11), then \
                                 deploy again",
                            unit, why
                        ));
                        continue;
                    }
                }
            }
            if running {
                // fix-159: a running unit whose unit file, drop-in or env
                // file this deploy changed restarts, through the health
                // check updates and rollbacks use (restart, wait for
                // active, NRestarts across a window), said before it
                // happens. Unchanged: left running, as adoption leaves it.
                // Story: docs/deployment/REGISTER.md.
                // latch-files: a secret file that names this unit in
                // `restarts` (a config it reads by path, not by systemd).
                let declared = spec.secret_files.iter().any(|f| {
                    f.restarts.as_deref() == Some(unit.as_str()) && secret_paths.contains(&f.path)
                });
                let reason = match crate::native::restart_reason(unit, &blob.content, &written) {
                    Some(r) if declared => Some(format!("{} and secret file changed", r)),
                    None if declared => Some("secret file changed".to_string()),
                    other => other,
                };
                if let Some(reason) = reason {
                    log_info(format!("[native] restarts {}: {}", unit, reason));
                    let svc = format!("{}.service", unit);
                    let out =
                        pct_sh(exec, m.vmid, &crate::ops::native::health_script(&svc), 180).await?;
                    if !out.success() {
                        let tail = pct_sh(
                            exec,
                            m.vmid,
                            &format!("journalctl -u {} -n 20 --no-pager 2>&1 | tail -20", svc),
                            60,
                        )
                        .await
                        .map(|o| o.stdout.trim().to_string())
                        .unwrap_or_default();
                        return Err(CoreError::Other(format!(
                            "{} did not come up healthy after its restart ({}; the check read \
                             {}) :: the new files are in place and the service does not run on \
                             them; correct the stack file and deploy again, or put the previous \
                             version back and deploy that. Last log lines: {}",
                            unit,
                            reason,
                            match out.stdout.trim() {
                                "" => "nothing",
                                r => r,
                            },
                            tail
                        )));
                    }
                    log_info(format!(
                        "[native] {} restarted and healthy ({})",
                        unit, reason
                    ));
                    changed = true;
                }
                // 6 · keep the vault current, so the NEXT rebuild can restore
                // what this container has. The same reasoning as the per-app
                // .env copies: a file that exists in exactly one place is one
                // disk failure from being gone.
                for f in need.env_files.iter().chain(need.credentials.iter()) {
                    // fix-39: a secret, read without echoing it.
                    let got = crate::executor::pct_sh_secret(
                        exec,
                        m.vmid,
                        &format!("cat {} 2>/dev/null || true", crate::ops::util::shq(f)),
                        60,
                    )
                    .await;
                    if let Ok(out) = got
                        && !out.stdout.trim().is_empty()
                    {
                        let vault = format!(
                            "{}/secrets/{}/{}",
                            ctx.state_dir,
                            m.stack_name,
                            vault_key(f)
                        );
                        let _ = exec.write_file(&vault, &out.stdout, 0o600).await;
                    }
                }
                continue;
            }
            log_info(format!(
                "[native] {} is not running — enabling and starting",
                unit
            ));
            run_ok(
                exec,
                &Cmd::new(
                    "pct",
                    &[
                        "exec",
                        &vm,
                        "--",
                        "sh",
                        "-c",
                        &format!("systemctl enable --now {}", unit),
                    ],
                    120,
                ),
            )
            .await?;
            let after = pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
            if after.stdout.trim() != "active" {
                return Err(CoreError::Command {
                    rendered: format!("systemctl enable --now {}", unit),
                    detail: format!("{} did not come up: {}", unit, after.stdout.trim()),
                });
            }
            changed = true;
        }
        // Expert panel 2026-09-27 (stale-secret-residue-on-pve): flat vault
        // copies from before fix-37 stayed on pve with live tokens nothing
        // restores. A flat copy is kept only while it is the one a unit would
        // still be restored from: a single unit reads that name and its
        // per-unit copy does not exist yet. Anything else is removed, by name.
        let dir = format!("{}/secrets/{}", ctx.state_dir, m.stack_name);
        let listing = exec
            .run(&Cmd::new(
                "find",
                &[dir.as_str(), "-maxdepth", "1", "-type", "f"],
                30,
            ))
            .await;
        if let Ok(out) = listing {
            for flat in out.stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let name = flat.rsplit('/').next().unwrap_or(flat);
                let still_needed = match seen.get(name) {
                    Some(paths) if paths.len() == 1 => {
                        let only = paths.iter().next().cloned().unwrap_or_default();
                        let per_unit = format!("{}/{}", dir, vault_key(&only));
                        !matches!(exec.read_file(&per_unit).await, Ok(c) if !c.trim().is_empty())
                    }
                    _ => false,
                };
                if still_needed {
                    continue;
                }
                run_ok(exec, &Cmd::new("rm", &["-f", flat], 30)).await?;
                log_warn(format!(
                    "[native] removed stale flat vault copy {} — no unit restores from it",
                    flat
                ));
                changed = true;
            }
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    step!(runner, exec, ctx, m, "record state", {
        let store = StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await?;
        // Preserve last_backup and the H8 enabled flag across redeploys —
        // parking is an explicit operator choice; refresh everything else.
        let prior = state
            .stacks
            .get(&m.stack_name)
            .map(|s| (s.last_backup, s.enabled));
        // C7: the native services registered on this stack are NOT the
        // deploy's to forget. Writing an empty list here unregistered kyu,
        // kyu-runner, http-switchboard and almanac the moment their
        // containers got a manifest — so the nightly backup of four services
        // would simply have stopped, quietly, with nothing to see.
        //
        // ask-8: except a unit the stack FILE dropped from `natives:` — the
        // step "retire dropped" stopped it, and it leaves the record here.
        let prior_natives: Vec<crate::native::NativeServiceManifest> = state
            .stacks
            .get(&m.stack_name)
            .map(|s| {
                s.natives
                    .iter()
                    .filter(|n| !dropped_natives.contains(&n.unit))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // ask-8 / ask-9: an app or unit that left the files leaves its
        // backups, /appdata and vault copies behind, KEPT until `homelab
        // wipe`; recorded here with the date it left. A stack or app that is
        // back is no longer retired.
        crate::ops::retired::unretire(&mut state, m);
        if let Some(pm) = prior_manifest.as_ref() {
            let before = state
                .stacks
                .get(&m.stack_name)
                .map(|s| s.apps.clone())
                .unwrap_or_default();
            // fix-186: an app this deploy still declares is obviously not
            // dropped (`m.apps.contains`) — but neither is one that moved to
            // `natives:` under the same name (a docker-to-native conversion,
            // exactly what the 3.70.0 admin/almanac deploy did). Only a name
            // declared NOWHERE in the new manifest actually left the stack.
            for app in before
                .iter()
                .filter(|a| !m.apps.contains(a) && !m.natives.contains(a))
            {
                crate::ops::retired::retire_app(&mut state, pm, app, &ctx.state_dir, ctx.now_unix);
            }
        }
        for unit in &dropped_natives {
            crate::ops::retired::retire_unit(
                &mut state,
                &m.stack_name,
                m.vmid,
                unit,
                unit_vault.get(unit).cloned().unwrap_or_default(),
                ctx.now_unix,
            );
        }
        let (mut last_backup, enabled) = prior.unwrap_or((0, true));
        // A C4 replacement destroys the record along with the container, so
        // there is nothing left to preserve and the rebuilt stack claims it
        // has never been backed up — which the fleet check then reports as
        // broken while the snapshots sit untouched in the repository. The
        // same happens to every stack at once on a rebuilt host. Ask the
        // repository instead: it is the thing that actually knows.
        if prior.is_none()
            && let Some(t) = crate::ops::backup::newest_snapshot_unix(exec, m, &ctx.backup).await
        {
            last_backup = t;
            log_info(format!(
                "[state] no record for '{}' — last backup recovered from the repository",
                m.stack_name
            ));
        }
        state.stacks.insert(
            m.stack_name.clone(),
            StackState {
                // fix-141: which commit this is, for `homelab status`.
                applied_source: spec.source.as_ref().map(|s| s.summary()),
                vmid: m.vmid,
                hostname: m.hostname.clone(),
                apps: m.apps.clone(),
                applied_at: ctx.now_unix,
                last_backup,
                applied_hash: manifest::intent_hash(spec),
                manifest: Some(m.clone()),
                enabled,
                natives: prior_natives,
                incomplete_step: None,
                // fix-41: the route this deploy wrote, the only one a later
                // deploy may retire.
                route_file: spec.gateway_route.as_ref().map(|r| r.filename.clone()),
                // fix-91: the extra route files it wrote, for the same reason.
                extra_route_files: spec
                    .extra_routes
                    .iter()
                    .map(|r| r.filename.clone())
                    .collect(),
                // fix-142: ground truth for the nightly in-container hash
                // comparison (`evaluate_container_drift`).
                pushed_file_hashes: all_file_hashes
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default(),
                // fix-192: recorded from the same `spec` `intent_hash` just
                // hashed above, so the two can never disagree about what
                // this deploy's intent actually was.
                component_digests: manifest::component_digests(spec),
            },
        );
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    // ── S2 · the final reconciliation. Every step above has now said it
    // succeeded. This asks the container itself whether that is true, and it
    // asks about the whole stack rather than about one step's own work.
    //
    // The reason it is a separate pass and not more assertions inside the
    // steps: a step can only check what it did. What hurt on 2026-09-01 was
    // never one step being wrong — it was the deploy stopping halfway and
    // everything after it silently not happening. Only something that looks
    // at the finished whole can notice that.
    step!(runner, exec, ctx, m, "reconcile", {
        let mut wrong: Vec<String> = Vec::new();
        let cfg = pct_sh(exec, m.vmid, "true", 30).await;
        if cfg.is_err() {
            wrong.push("the container did not answer".into());
        }
        let live = run_ok(exec, &Cmd::new("pct", &["config", &vm], 60))
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();

        // Hostname: A2 refuses a mismatch before touching anything, so a
        // mismatch here means the container was swapped under us.
        if !live.contains(&format!("hostname: {}", m.hostname)) {
            wrong.push(format!("hostname is not {}", m.hostname));
        }
        // Every declared mount, storage and borrowed alike. F118 lost these
        // by dropping a field it did not understand, and the deploy reported
        // success while the downloader came up with no disks.
        for st in &m.storage {
            if !live.contains(&format!("{},mp=", st.host_path)) {
                wrong.push(format!("storage {} is not mounted", st.host_path));
            }
        }
        for dm in &m.data_mounts {
            if !live.contains(&format!("{},mp={}", dm.host_path, dm.mount_point)) {
                wrong.push(format!("data mount {} is not mounted", dm.host_path));
            }
        }
        // Boot policy, read with W3's own parser rather than by looking for a
        // line. `pct config` prints nothing at all when onboot is 0, so a
        // text match on "onboot: 0" can never succeed and a stack that asks
        // for onboot: false would be reported wrong forever.
        // Only when the value can actually be read. W3 leaves an unreadable
        // boot policy alone, and a check that demands more than the deploy is
        // willing to enforce turns into a deploy that can never succeed.
        let live_boot = crate::ops::reconcile::parse(&live);
        if let Some(on) = live_boot.onboot
            && on != m.boot.onboot
        {
            wrong.push(format!(
                "boot policy is {} where the stack file says {}",
                on, m.boot.onboot
            ));
        }
        // Every app actually running. `docker compose up -d` returning 0 is
        // not the same as a container that stayed up: a crash loop exits 0
        // on the way in.
        //
        // Asked per app directory rather than by container name. An app is a
        // compose project, and a project may define several services with
        // names of their own: `stacks/registry/registry` runs cache-dockerhub,
        // cache-gcr, cache-ghcr and cache-lscr, and not one of them is called
        // `registry`. Matching names reported that healthy stack as broken.
        if !m.native_only {
            for app in &m.apps {
                // fix-132/fix-133: JSON + the Unknown state, so a probe that
                // could not be read is its own finding rather than silently
                // the same "no running service" as an actually dead app.
                match crate::ops::util::compose_running_services(
                    exec,
                    m.vmid,
                    &m.stack_name,
                    app,
                    "",
                )
                .await
                .unwrap_or(None)
                {
                    Some(running) if !running.is_empty() => {}
                    Some(_) => wrong.push(format!("app '{}' has no running service", app)),
                    None => wrong.push(format!(
                        "app '{}' :: could not read `docker compose ps`",
                        app
                    )),
                }
            }
        }
        // Every native unit actually active.
        for n in &m.natives {
            let act = pct_sh(
                exec,
                m.vmid,
                &format!("systemctl is-active {} 2>/dev/null || true", n),
                60,
            )
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
            if !act.trim().starts_with("active") {
                wrong.push(format!("native unit '{}' is not active", n));
            }
        }

        if wrong.is_empty() {
            log_info(format!(
                "[reconcile] the container matches the stack file on {} point(s)",
                2 + m.storage.len() + m.data_mounts.len() + m.apps.len() + m.natives.len()
            ));
            Ok(StepOutcome::Unchanged)
        } else {
            Err(CoreError::Command {
                rendered: format!("reconcile {}", m.stack_name),
                detail: format!(
                    "the deploy reported success but the container does not match \
                     the stack file: {}",
                    wrong.join("; ")
                ),
            })
        }
    });

    // ── J1: the AFTER half. Same commands, same container, minutes later.
    step!(runner, exec, ctx, m, "service checks", {
        if spec.checks.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut all: Vec<crate::checks::Reading> = Vec::new();
        // fix-27: which app and command each reading came from, so a reading
        // of an app this deploy just restarted can be taken again.
        let mut origin: Vec<(String, String)> = Vec::new();
        for (app, sc) in &spec.checks {
            let before = baseline.get(app);
            for c in &sc.checks {
                origin.push((app.clone(), c.command.clone()));
                let after = pct_sh(exec, m.vmid, &c.command, 120)
                    .await
                    .map(|o| o.stdout.trim().to_string())
                    .unwrap_or_default();
                let b = before
                    .and_then(|rs| rs.iter().find(|(n, _)| n == &c.name))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                all.push(crate::checks::Reading {
                    name: format!("{} · {}", app, c.name),
                    before: b,
                    after,
                    expect: c.expect,
                    blind_spot: c.blind_spot.clone(),
                });
            }
        }
        let (mut verdicts, blind) = crate::checks::judge_all(&all);
        // fix-27: an app this deploy restarted or recreated may still be
        // warming up. On 2026-09-26 Loki answered "Ingester not ready:
        // waiting for 15s after being ready" to the first reading after its
        // restart, and the deploy asked Kenny about a change that fixed
        // itself fifteen seconds later. Such a reading is taken again, every
        // 5 s for up to 60 s, before it counts as a change. An app the deploy
        // did not touch is judged on its first reading, as before.
        let touched: std::collections::BTreeSet<String> = {
            let mut t = needs_restart.lock().map(|g| g.clone()).unwrap_or_default();
            t.extend(recreated.lock().map(|g| g.clone()).unwrap_or_default());
            t
        };
        for i in 0..all.len() {
            if !matches!(verdicts[i], crate::checks::Verdict::Regressed(_))
                || !touched.contains(&origin[i].0)
            {
                continue;
            }
            for _ in 0..12 {
                let again = pct_sh(exec, m.vmid, &format!("sleep 5; {}", origin[i].1), 120)
                    .await
                    .map(|o| o.stdout.trim().to_string())
                    .unwrap_or_default();
                all[i].after = again;
                verdicts[i] = crate::checks::judge(&all[i]);
                if !matches!(verdicts[i], crate::checks::Verdict::Regressed(_)) {
                    log_info(format!(
                        "[check] {} settled after its restart :: {}",
                        all[i].name, all[i].after
                    ));
                    break;
                }
            }
        }
        for (r, v) in all.iter().zip(verdicts.iter()) {
            match v {
                crate::checks::Verdict::Ok => {
                    log_info(format!("[check] {} :: {} → {}", r.name, r.before, r.after))
                }
                crate::checks::Verdict::Unreadable(why) => ctx.sink.emit(PipelineEvent::Line {
                    level: Level::Warn,
                    source: "HOST".into(),
                    msg: format!("[check] {}", why),
                }),
                crate::checks::Verdict::Regressed(_) => {}
            }
        }
        // Reported even when everything passed: a green result is exactly
        // when nobody asks what was not checked.
        for b in &blind {
            log_info(format!("[check] does not prove: {}", b));
        }
        let bad = crate::checks::regressions(&verdicts);
        if bad.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        // T69 (Kenny, form H1). A reading that went down is not always
        // damage. The case that raised this: routes 29 → 28 after a route
        // was removed on purpose — where failing the deploy and bundling an
        // incident is the wrong answer, and nobody learns anything from it.
        //
        // So the step stops and asks, instead of deciding for the operator.
        // If nobody is watching — the nightly round at 04:00 — the answer is
        // `Unattended` and the deploy fails exactly as it did before. Silence
        // never reads as permission.
        let answer = ctx
            .asker
            .ask(&crate::ask::Question {
                op: format!("deploy-{}", m.stack_name),
                step: "service checks".into(),
                what: bad.join("; "),
                // fix-107: in the language of the client that shows it.
                if_allowed: "the deploy goes on and records the new value as the normal \
                             one"
                .into(),
                if_stopped: "the deploy fails here and bundles an incident".into(),
            })
            .await;
        match answer {
            crate::ask::Answer::Allow => {
                log_info(format!(
                    "[check] {} :: allowed by the operator — the new reading is now the \
                     baseline",
                    bad.join("; ")
                ));
                Ok(StepOutcome::Changed)
            }
            other => Err(CoreError::Command {
                rendered: format!("service checks {}", m.stack_name),
                detail: format!(
                    "{}{}",
                    bad.join("; "),
                    match other {
                        crate::ask::Answer::Stop => " :: stopped by the operator".to_string(),
                        crate::ask::Answer::Unattended(why) => format!(" :: {}", why),
                        crate::ask::Answer::Allow => String::new(),
                    }
                ),
            }),
        }
    });

    // The list no measurement can settle. It goes out as a notification Kenny
    // has to acknowledge (form I2) rather than a page he has to go and find.
    // G17: registering them is the half that was missing. Printing a
    // question at the end of a transcript is not asking anybody anything.
    let questions = crate::ops::manualchecks::questions_of(&spec.checks);
    // Registered even when the list is empty: that is how a stack that
    // dropped its last question loses it (declarative, step-22).
    {
        let store = StateStore::new(ctx.exec, &ctx.state_dir);
        if let Ok(mut st) = store.load().await {
            let before = st.manual_checks.len();
            crate::ops::manualchecks::register(
                &mut st,
                &spec.manifest.stack_name,
                &questions,
                ctx.now_unix,
            );
            // checks-automate: the stack's probes, as this deploy declares them.
            let probes_before = st.probes.clone();
            crate::ops::probes::register(
                &mut st,
                &spec.manifest.stack_name,
                spec.manifest.vmid,
                &spec.checks,
            );
            let busy_before = st.busy_checks.clone();
            crate::ops::busy::register(&mut st, &spec.manifest.stack_name, &spec.checks);
            if !questions.is_empty()
                || st.manual_checks.len() != before
                || st.probes != probes_before
                || st.busy_checks != busy_before
            {
                let _ = store.save(st).await;
            }
        }
    }
    if !questions.is_empty() {
        let lines: Vec<String> = questions
            .iter()
            .map(|q| {
                format!(
                    "{}  {}",
                    crate::ops::manualchecks::id_for(&spec.manifest.stack_name, &q.app, &q.text),
                    q.text
                )
            })
            .collect();
        runner.log(
            Level::Warn,
            format!(
                "[check] {} thing(s) only a person can confirm — answer one with \
                 `homelab checks answer <id> ok|nok`:\n  - {}",
                lines.len(),
                lines.join("\n  - ")
            ),
        );
    }

    runner.log(
        Level::Info,
        format!(
            "[sync] Sync complete — {} {} · {} app(s) verified",
            if created { "provisioned" } else { "updated" },
            m.hostname,
            m.apps.len()
        ),
    );
    runner.finish_ok()
}
