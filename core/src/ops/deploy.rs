//! The deploy operation (D1): validate → safety gates → storage → provision →
//! bootstrap → guards → intent to repo → push files/env → start apps →
//! verify → gateway route → record state. Every step runs through the shared
//! runner (AR3); every side effect through the Executor (AR2).

use crate::error::CoreError;
use crate::executor::{pct_sh, run_ok, Cmd};
use crate::manifest::{self, DeploySpec};
use crate::ops::{guards, util::push_content, OpCtx};
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
    ($runner:expr, $exec:expr, $ctx:expr, $m:expr, $name:expr, $body:expr) => {
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
    ($runner:expr, $exec:expr, $ctx:expr, $m:expr, $name:expr, $body:expr, $verify:expr) => {
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
                    vmid: m.vmid,
                    hostname: m.hostname.clone(),
                    apps: m.apps.clone(),
                    applied_at: ctx.now_unix,
                    last_backup: 0,
                    applied_hash: String::new(),
                    manifest: Some(m.clone()),
                    enabled: true,
                    native: None,
                    natives: Vec::new(),
                    incomplete_step: Some(step.to_string()),
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
    let sent: std::collections::BTreeSet<&str> =
        spec.files.iter().map(|f| f.path.as_str()).collect();
    let out = match pct_sh(
        exec,
        m.vmid,
        &format!(
            "cd '/opt/{0}' 2>/dev/null && find . -type f -printf '%P\\n' 2>/dev/null || true",
            m.stack_name
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
        .map(str::to_string)
        .collect()
}

/// A5: the vault filename for a file a unit reads.
///
/// `/appdata/kyu/kyu-config/kyu.env` becomes `kyu.env`, so the vault holds
/// `<state_dir>/secrets/<stack>/kyu.env` beside the per-app `.env` copies
/// that compose stacks already get. Flat on purpose: two units on one stack
/// cannot name the same file, because the paths are `<unit>-config/...`.
fn vault_key(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub async fn deploy(ctx: &OpCtx<'_>, spec: &DeploySpec) -> OperationReport {
    let m = &spec.manifest;
    let op = format!("deploy-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
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
        Ok(StepOutcome::Unchanged)
    });

    // ── A1 + A2: refuse before anything mutates. ─────────────────────────
    step!(runner, exec, ctx, m, "safety gates", {
        exists = safety::check_deploy_target(exec, &ctx.safety, m).await?;
        Ok(StepOutcome::Unchanged)
    });

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
    if let Some(cache) = ctx.registry_cache.as_ref() {
        if !m.native_only {
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
        for mount in &m.storage {
            // An app that declares it keeps nothing is empty BY DESIGN, so
            // "empty, therefore restore it" is exactly the wrong conclusion.
            // Without this the gateway asked Google Drive about
            // cloudflared-config on every single deploy — and a stale
            // snapshot would have been restored into a directory whose whole
            // point is that it stays empty (F154, seen live 2026-09-01).
            if mount.no_data {
                continue;
            }
            let probe = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!("ls -A '{}' 2>/dev/null | head -1", mount.host_path),
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
            let usable = matches!(&has_snapshot, Ok(out)
                if out.success() && out.stdout.trim() != "[]" && !out.stdout.trim().is_empty());
            if !usable {
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

    // ── C1: create or reuse the container; C3 boot policy at create. ─────
    step!(runner, exec, ctx, m, "provision container", {
        if !exists {
            let mut net = format!(
                "name=eth0,bridge={},firewall=0,ip={},gw={}",
                m.network.bridge, m.network.ip, m.network.gateway
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
                        if tpl_unpriv { "unprivileged" } else { "privileged" },
                        m.stack_name,
                        if m.lxc.unprivileged { "unprivileged" } else { "privileged" },
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
                    "--timezone".into(),
                    "host".into(),
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
                // Managed containers are recognizable in the Proxmox UI and
                // inherit the host timezone.
                "--description".into(),
                format!("managed by homelab v2 :: stack {}", m.stack_name),
                "--tags".into(),
                "homelab".into(),
                "--timezone".into(),
                "host".into(),
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
                let missing: Vec<(String, String)> = want
                    .into_iter()
                    .filter(|(key, val)| {
                        let line = format!("{}: {}", key.trim_start_matches('-'), val);
                        !cfg.stdout.lines().any(|l| l.trim() == line)
                    })
                    .collect();
                if !missing.is_empty() {
                    let protected = cfg.stdout.lines().any(|l| l.trim() == "protection: 1");
                    if protected {
                        run_ok(
                            exec,
                            &Cmd::new("pct", &["set", &vm, "--protection", "0"], 60),
                        )
                        .await?;
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
                        "[mounts] attached — a running container sees them after a reboot".into(),
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

    // ── D4: intent into the host-local git repo (never secrets, A5). ─────
    step!(runner, exec, ctx, m, "commit intent", {
        let repo = format!("{}/repo", ctx.state_dir);
        let stack_dir = format!("{}/stacks/{}", repo, m.stack_name);
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
        let msg = format!("deploy {}", m.stack_name);
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
                let dest = format!("/opt/{}/{}", m.stack_name, f.path);
                let perms = format!("{:o}", f.mode.unwrap_or(0o644));
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
                        if rewritten != f.content {
                            if let Some((app, _)) = f.path.split_once('/') {
                                if let Ok(mut g) = cache_orig_w.lock() {
                                    g.insert(
                                        app.to_string(),
                                        (dest.clone(), f.content.clone(), perms.clone()),
                                    );
                                }
                            }
                        }
                        rewritten
                    }
                    _ => f.content.clone(),
                };
                let changed = push_content(exec, m.vmid, &dest, &content, &perms).await?;
                if changed {
                    if let Ok(mut g) = pushed_w.lock() {
                        g.push((dest.clone(), manifest::sha256_hex(content.as_bytes())));
                    }
                    // The path is "<app>/<file>"; a file outside an app directory
                    // belongs to no service and needs nothing restarted.
                    if let Some((app, name)) = f.path.split_once('/') {
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
            for (app, env) in &spec.env {
                let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
                push_content(exec, m.vmid, &dest, env, "600").await?;
                // HOST-side vault copy for redeploys — outside the git repo.
                let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
                exec.write_file(&vault, env, 0o600).await?;
                log_info(format!("[vault] {} sealed (values not logged)", dest));
            }
            // A5/E3: apps whose env the client did NOT send fall back to the
            // vault — a wiped container gets its .env back on redeploy.
            for app in &m.apps {
                if spec.env.contains_key(app) {
                    continue;
                }
                let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
                if let Ok(env) = exec.read_file(&vault).await {
                    let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
                    push_content(exec, m.vmid, &dest, &env, "600").await?;
                    log_info(format!("[vault] {} restored from vault", dest));
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
                .map(|(p, _)| format!("'{}'", p))
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
                "u=$(grep -m1 '^REGISTRY_USER=' '{f}' | cut -d= -f2-); \
                 t=$(grep -m1 '^REGISTRY_TOKEN=' '{f}' | cut -d= -f2-); \
                 [ -n \"$u\" ] && [ -n \"$t\" ] || {{ echo 'no REGISTRY_USER/REGISTRY_TOKEN in {f}' >&2; exit 1; }}; \
                 printf %s \"$t\" | docker login {r} -u \"$u\" --password-stdin",
                f = envf,
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
            let dir = format!("/opt/{}/{}", m.stack_name, app);
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
            let cmd = format!("cd '{}' && docker compose pull -q", dir);
            // Not `?`: a timeout is an Err by the Executor contract, and a
            // timeout is precisely the case the fallback exists for. Taking
            // the error here would step over it.
            let first = pct_sh(exec, m.vmid, &cmd, budget).await;
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
                &format!("cd '{}' && docker compose up -d --remove-orphans", dir),
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
            let restart_this = needs_restart
                .lock()
                .map(|g| g.contains(app))
                .unwrap_or(false)
                && !recreated.lock().map(|g| g.contains(app)).unwrap_or(false);
            if restart_this {
                let r = pct_sh(
                    exec,
                    m.vmid,
                    &format!("cd '{}' && docker compose restart", dir),
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
                || out.contains("no such file or directory: unknown");
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
            let dir = format!("/opt/{}/{}", m.stack_name, app);
            let out = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd '{}' && docker compose ps --status running --services",
                    dir
                ),
                60,
            )
            .await?;
            if out.stdout.trim().is_empty() {
                let diag = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "cd '{}' && docker compose ps -a && docker compose logs --tail 20",
                        dir
                    ),
                    60,
                )
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
                return Err(CoreError::Command {
                    rendered: format!("verify {}", app),
                    detail: format!("no running services\n{}", diag),
                });
            }
            log_info(format!("[gate] {} :: running", app));
        }
        Ok(StepOutcome::Unchanged)
    });

    // ── H1: the single allowed cross-stack write. ────────────────────────
    if let Some(route) = &spec.gateway_route {
        step!(runner, exec, ctx, m, "gateway route", {
            let dest =
                safety::check_gateway_route(&ctx.safety, route.gateway_vmid, &route.filename)?;
            // H6 hardening: when the routes dir lives under /appdata/ it is a
            // HOST path bind-mounted into the gateway — write it host-side so
            // route fragments survive gateway recreation. The legacy /opt
            // path keeps the pct-push behavior until the platform migration.
            let changed = if ctx.safety.gateway_routes_dir.starts_with("/appdata/") {
                exec.write_file(&dest, &route.content, 0o644).await?;
                true
            } else {
                push_content(exec, route.gateway_vmid, &dest, &route.content, "644").await?
            };
            log_info(format!("[route] {} (file-provider watch reloads)", dest));
            Ok(if changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
    }

    // ── T2: the stack brings its own dashboard. Written into Grafana's
    // provisioning directory on the gateway, where the watcher picks it up
    // within ten seconds. Provisioned dashboards are files, not database
    // rows: they survive a rebuild of that container and they diff in review.
    // Best-effort, like the discovery file — Grafana being down is not a
    // reason to fail a deploy.
    if let Some(dir) = ctx.grafana_dashboards_dir.as_deref() {
        step!(runner, exec, ctx, m, "grafana dashboard", {
            let dest = crate::ops::dashboard::dashboard_file(dir, &m.stack_name);
            // B3: a stack with no compose apps runs systemd units, and the
            // cadvisor panels can only ever be empty there.
            let native = m.apps.is_empty() && !m.natives.is_empty();
            let body = crate::ops::dashboard::dashboard_json_for(
                &m.stack_name,
                if native { &m.natives } else { &m.apps },
                native,
            );
            match push_content(exec, ctx.safety.gateway_vmid, &dest, &body, "644").await {
                Ok(changed) => {
                    if changed {
                        log_info(format!("[t2] {} (provisioning watcher reloads)", dest));
                    }
                    // Kenny, 2026-09-02: two dashboards with the same uid, one
                    // generated and one a stale copy of an earlier generation,
                    // and Grafana keeps whichever provider loaded first. That
                    // is why six stacks appeared in "Homelab (generated)" and
                    // seven did not — nondeterministically. The generated one
                    // is the truth, so its predecessor goes with it.
                    if let Some(parent) = dest.rsplit_once('/').map(|(d, _)| d) {
                        if let Some(grandparent) = parent.rsplit_once('/').map(|(d, _)| d) {
                            let old =
                                format!("{}/dashboards/homelab-{}.json", grandparent, m.stack_name);
                            if old != dest {
                                let _ = pct_sh(
                                    exec,
                                    ctx.safety.gateway_vmid,
                                    &format!("rm -f {}", crate::ops::util::shq(&old)),
                                    30,
                                )
                                .await;
                            }
                        }
                    }
                    Ok(if changed {
                        StepOutcome::Changed
                    } else {
                        StepOutcome::Unchanged
                    })
                }
                Err(e) => {
                    log_info(format!(
                        "[t2] could not write {} ({}) — this stack has no generated dashboard yet",
                        dest, e
                    ));
                    Ok(StepOutcome::Unchanged)
                }
            }
        });
    }

    // ── T51: the front page, rendered from the routes this orchestrator
    // has already written for the whole fleet.
    //
    // Fleet-wide rather than per stack, because Homepage keeps one file — so
    // this step reads every route fragment in the gateway's route directory
    // rather than only the one it just wrote. Best-effort, like the discovery
    // file and the dashboard: a front page is a convenience, and a deploy
    // that fails over it would be a deploy that fails over nothing.
    if let Some(dest) = ctx.homepage_services_file.as_deref() {
        step!(runner, exec, ctx, m, "homepage services", {
            let dir = &ctx.safety.gateway_routes_dir;
            let listing = pct_sh(
                exec,
                ctx.safety.gateway_vmid,
                &format!("ls -1 '{}'/*.yml 2>/dev/null || true", dir),
                60,
            )
            .await?;
            let mut stacks: Vec<(String, Vec<crate::ops::homepage::Entry>)> = Vec::new();
            for path in listing
                .stdout
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
            {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                // `112-app-almanac` → `almanac`; a hand-written
                // `manual-…` fragment keeps its own name.
                let stack = name
                    .split_once("-app-")
                    .map(|(_, s)| s.to_string())
                    .unwrap_or(name);
                let body = pct_sh(
                    exec,
                    ctx.safety.gateway_vmid,
                    &format!("cat '{}'", path),
                    60,
                )
                .await?;
                let entries = crate::ops::homepage::entries_from_route(&body.stdout);
                if !entries.is_empty() {
                    stacks.push((stack, entries));
                }
            }
            stacks.sort_by(|a, b| a.0.cmp(&b.0));
            // V6: the overlay is intent, not runtime config, so it lives in
            // the intent repo next to the stack file that ships it — where
            // `git log` records who changed the front page and why. Found by
            // its name rather than by a new setting: there is exactly one,
            // and a config knob for it would be one more line nobody
            // remembers to add (which is how T51 sat switched off for two
            // days — F186).
            let found = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "ls -1 {}/repo/stacks/*/*/services-overlay.yml 2>/dev/null | head -1",
                            ctx.state_dir
                        ),
                    ],
                    30,
                ))
                .await?;
            let overlay_path = found.stdout.trim().to_string();
            let overlay = match exec.read_file(&overlay_path).await {
                Ok(text) => {
                    let ov = crate::ops::homepage::parse_overlay(&text);
                    log_info(format!(
                        "[t51] overlay: {} entr(y/ies) from {}",
                        ov.blocks.len(),
                        overlay_path
                    ));
                    Some(ov)
                }
                Err(_) => None,
            };
            // V6b: read each widget's API key from the application itself.
            //
            // Through ctx.exec rather than the tracing executor on purpose:
            // the tracing one echoes stdout into the transcript, and these
            // are live keys (standing rule 10 — a hash may appear there,
            // plaintext never). Best-effort per app: a key that cannot be
            // read leaves the widget pointing at Homepage's own variable,
            // which fails visibly rather than silently.
            let mut widget_keys: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            {
                let store = crate::state::StateStore::new(exec, &ctx.state_dir);
                let st = store.load().await.unwrap_or_default();
                for (stack, entries) in &stacks {
                    let Some(vmid) = st
                        .stacks
                        .get(stack)
                        .and_then(|s| s.manifest.as_ref())
                        .map(|mf| mf.vmid)
                    else {
                        continue;
                    };
                    for e in entries {
                        let Some(spec) = crate::ops::homepage::widget_for(&e.app) else {
                            continue;
                        };
                        let Some(cmd) = spec.key_cmd else { continue };
                        // O7 makes this path the only legal one, so it is
                        // derived rather than looked up.
                        let dir = format!("/appdata/{}/{}-config", stack, e.app);
                        let script = cmd.replace("{dir}", &dir);
                        if let Ok(out) = pct_sh(ctx.exec, vmid, &script, 30).await {
                            let k = out.stdout.trim().to_string();
                            if out.success() && !k.is_empty() {
                                widget_keys.insert(e.app.clone(), k);
                            }
                        }
                    }
                }
            }
            log_info(format!(
                "[t51] widget keys read from the applications themselves: {}",
                widget_keys.len()
            ));
            let body = crate::ops::homepage::services_yaml(&stacks, overlay.as_ref(), &widget_keys);
            match crate::ops::util::write_file_owned_like_dir(exec, dest, &body, 0o644).await {
                Ok(()) => {
                    log_info(format!(
                        "[t51] {} — {} stack(s) on the front page",
                        dest,
                        stacks.len()
                    ));
                    Ok(StepOutcome::Changed)
                }
                Err(e) => {
                    log_info(format!("[t51] could not write {} ({})", dest, e));
                    Ok(StepOutcome::Unchanged)
                }
            }
        });
    }

    // ── T49: the watch list, rendered from the fleet the same way T51
    // renders the front page and T1 the scrape targets.
    //
    // Fleet-wide for the same reason: Uptime Kuma has one monitor set, so
    // this reads every stack in host state rather than only the one just
    // deployed. It writes a FILE and stops there — the seeder in the uptime
    // stack is what talks to Uptime Kuma, because the protocol behind that
    // API is not one its authors offer as a public interface, and
    // reimplementing it in Rust is exactly the thing that breaks silently on
    // an update (Kenny, forms R13 and V2 — D87).
    //
    // Best-effort like its two siblings: a monitor list is a convenience,
    // and a deploy that fails over it would be a deploy that fails over
    // nothing.
    if let Some(dest) = ctx.kuma_monitors_file.as_deref() {
        step!(runner, exec, ctx, m, "uptime monitors", {
            let store = crate::state::StateStore::new(exec, &ctx.state_dir);
            let state = store.load().await?;
            let mut fleet: Vec<(String, String)> = state
                .stacks
                .iter()
                .filter_map(|(name, st)| {
                    st.manifest
                        .as_ref()
                        .map(|mf| (name.clone(), mf.network.ip.clone()))
                })
                .collect();
            // The stack being deployed is in state by now, but on a FIRST
            // deploy the state write happens after this step — so add it
            // here rather than let a brand-new stack wait a whole deploy
            // for the monitor that says whether it came up.
            if !fleet.iter().any(|(n, _)| n == &m.stack_name) {
                fleet.push((m.stack_name.clone(), m.network.ip.clone()));
            }
            let monitors = crate::ops::monitors::host_monitors(&fleet);
            let body = crate::ops::monitors::monitors_json(&monitors);
            match crate::ops::util::write_file_owned_like_dir(exec, dest, &body, 0o644).await {
                Ok(()) => {
                    log_info(format!(
                        "[t49] {} — {} host monitor(s) for the seeder",
                        dest,
                        monitors.len()
                    ));
                    Ok(StepOutcome::Changed)
                }
                Err(e) => {
                    log_info(format!("[t49] could not write {} ({})", dest, e));
                    Ok(StepOutcome::Unchanged)
                }
            }
        });
    }

    // ── D3: garbage-collect apps removed from intent — stop + remove their
    // compose project and /opt dir; /appdata config dirs are kept.
    // Files the container has and the repository does not.
    //
    // The garbage collection below works per APP DIRECTORY: an app that
    // leaves the stack file takes its directory with it. A FILE that leaves
    // an app takes nothing — the deploy puts files down and has never picked
    // one up. Thirteen Grafana dashboards deleted from the repository on
    // 2026-09-01 were still on CT 104 two deploys later, one of them with the
    // same uid as its replacement (F161).
    //
    // Reporting only, deliberately (Kenny, form H2b). Deleting is the
    // irreversible direction and a deploy runs when nobody is looking;
    // `homelab prune-orphans` does the removing, after showing the same list
    // and asking.
    step!(runner, exec, ctx, m, "orphan files", {
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        let orphans = orphan_files(exec, m, spec).await;
        if orphans.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        log_info(format!(
            "[orphans] {} file(s) on the container that the repository no longer has — \
             `homelab prune-orphans stacks/{}` removes them after showing the list:",
            orphans.len(),
            m.stack_name
        ));
        for o in orphans.iter().take(20) {
            log_info(format!("[orphans]   {}", o));
        }
        if orphans.len() > 20 {
            log_info(format!("[orphans]   … and {} more", orphans.len() - 20));
        }
        // Never a failure: a file too many breaks nothing today, and a deploy
        // that goes red over it would be one people learn to ignore.
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, exec, ctx, m, "garbage collect", {
        // Nor is there anything to garbage-collect: an app directory under
        // /opt on a native-only container was never put there by a deploy.
        if m.native_only {
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
            let _ = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd '/opt/{stack}/{app}' && docker compose down --remove-orphans; rm -rf '/opt/{stack}/{app}'",
                    stack = m.stack_name,
                    app = app
                ),
                300,
            )
            .await?;
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
            let body = crate::ops::logshipper::config(&m.stack_name, &m.hostname, loki);
            let changed = push_content(
                exec,
                m.vmid,
                crate::ops::logshipper::CONFIG_PATH,
                &body,
                "644",
            )
            .await?;
            if fresh || changed {
                pct_sh(
                    exec,
                    m.vmid,
                    &crate::ops::logshipper::permissions_script(),
                    60,
                )
                .await?;
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
            Ok(if fresh || changed {
                StepOutcome::Changed
            } else {
                StepOutcome::Unchanged
            })
        });
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
            if push_content(exec, m.vmid, &dest, &blob.content, "644").await? {
                log_info(format!("[native] {} written", dest));
                pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
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
                let b64_path = format!("{}.homelab-b64", path);
                // Staged beside the target, then moved: a transfer that dies
                // half way must never leave a truncated program in place.
                if push_content(exec, m.vmid, &b64_path, b64, "600").await? {
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
                match exec.read_file(&vault).await {
                    Ok(content) if !content.trim().is_empty() => {
                        push_content(exec, m.vmid, f, &content, "600").await?;
                        log_info(format!("[native] {} restored from the vault", f));
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
            if active.stdout.trim() == "active" {
                // 6 · keep the vault current, so the NEXT rebuild can restore
                // what this container has. The same reasoning as the per-app
                // .env copies: a file that exists in exactly one place is one
                // disk failure from being gone.
                for f in need.env_files.iter().chain(need.credentials.iter()) {
                    let got = pct_sh(
                        exec,
                        m.vmid,
                        &format!("cat {} 2>/dev/null || true", crate::ops::util::shq(f)),
                        60,
                    )
                    .await;
                    if let Ok(out) = got {
                        if !out.stdout.trim().is_empty() {
                            let vault = format!(
                                "{}/secrets/{}/{}",
                                ctx.state_dir,
                                m.stack_name,
                                vault_key(f)
                            );
                            let _ = exec.write_file(&vault, &out.stdout, 0o600).await;
                        }
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
        let (prior_native, prior_natives) = state
            .stacks
            .get(&m.stack_name)
            .map(|s| (s.native.clone(), s.natives.clone()))
            .unwrap_or((None, Vec::new()));
        let (mut last_backup, enabled) = prior.unwrap_or((0, true));
        // A C4 replacement destroys the record along with the container, so
        // there is nothing left to preserve and the rebuilt stack claims it
        // has never been backed up — which the fleet check then reports as
        // broken while the snapshots sit untouched in the repository. The
        // same happens to every stack at once on a rebuilt host. Ask the
        // repository instead: it is the thing that actually knows.
        if prior.is_none() {
            if let Some(t) = crate::ops::backup::newest_snapshot_unix(exec, m, &ctx.backup).await {
                last_backup = t;
                log_info(format!(
                    "[state] no record for '{}' — last backup recovered from the repository",
                    m.stack_name
                ));
            }
        }
        state.stacks.insert(
            m.stack_name.clone(),
            StackState {
                vmid: m.vmid,
                hostname: m.hostname.clone(),
                apps: m.apps.clone(),
                applied_at: ctx.now_unix,
                last_backup,
                applied_hash: manifest::intent_hash(spec),
                manifest: Some(m.clone()),
                enabled,
                native: prior_native,
                natives: prior_natives,
                incomplete_step: None,
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
        if let Some(on) = live_boot.onboot {
            if on != m.boot.onboot {
                wrong.push(format!(
                    "boot policy is {} where the stack file says {}",
                    on, m.boot.onboot
                ));
            }
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
                let out = pct_sh(
                    exec,
                    m.vmid,
                    &format!(
                        "cd '/opt/{}/{}' && docker compose ps --status running --services",
                        m.stack_name, app
                    ),
                    120,
                )
                .await
                .map(|o| o.stdout.trim().to_string())
                .unwrap_or_default();
                if out.is_empty() {
                    wrong.push(format!("app '{}' has no running service", app));
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
        for (app, sc) in &spec.checks {
            let before = baseline.get(app);
            for c in &sc.checks {
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
        let (verdicts, blind) = crate::checks::judge_all(&all);
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
                if_allowed: "de uitrol gaat door en legt de nieuwe waarde vast als de \
                             normale stand"
                    .into(),
                if_stopped: "de uitrol faalt hier en bundelt een incident".into(),
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
    if !questions.is_empty() {
        let store = StateStore::new(ctx.exec, &ctx.state_dir);
        if let Ok(mut st) = store.load().await {
            crate::ops::manualchecks::register(
                &mut st,
                &spec.manifest.stack_name,
                &questions,
                ctx.now_unix,
            );
            let _ = store.save(st).await;
        }
        let lines: Vec<String> = questions
            .iter()
            .map(|(app, text)| {
                format!(
                    "{}  {}",
                    crate::ops::manualchecks::id_for(&spec.manifest.stack_name, app, text),
                    text
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
