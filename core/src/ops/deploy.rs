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
macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(outcome) => outcome,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
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

    // ── D10: never trust the client — validate host-side too. ────────────
    step!(runner, "validate", {
        manifest::validate(spec)?;
        Ok(StepOutcome::Unchanged)
    });

    // ── A1 + A2: refuse before anything mutates. ─────────────────────────
    step!(runner, "safety gates", {
        exists = safety::check_deploy_target(exec, &ctx.safety, m).await?;
        Ok(StepOutcome::Unchanged)
    });

    // ── D60: which upstreams does the cache answer for, right now?
    // Asked from inside the container that is about to pull, because that is
    // the only place the answer means anything. A cache that does not answer
    // is not an error: the image keeps naming its own origin and the pull
    // goes out to the internet exactly as it did before there was a cache.
    let mut cache_up: Vec<String> = Vec::new();
    step!(runner, "registry cache", {
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
    step!(runner, "hardware readiness", {
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
    step!(runner, "host storage", {
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
    step!(runner, "auto-restore check", {
        if m.storage.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let bcfg = ctx.backup.clone();
        let mut restored_any = false;
        let mut failed_any = false;
        for mount in &m.storage {
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
    step!(runner, "metrics discovery", {
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
    step!(runner, "provision container", {
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

    // H2: register the static IP as a Kea reservation (fail-open — a DHCP
    // nicety must never block a deploy).
    step!(runner, "dhcp reservation", {
        let Some(kea_cfg) = ctx.kea.as_ref() else {
            return Ok(StepOutcome::Unchanged);
        };
        if !created {
            return Ok(StepOutcome::Unchanged);
        }
        let cfg_out = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        let mac = cfg_out
            .stdout
            .lines()
            .find(|l| l.starts_with("net0:"))
            .and_then(|l| l.split("hwaddr=").nth(1))
            .map(|s| s.split(',').next().unwrap_or("").to_string())
            .unwrap_or_default();
        let ip = m.network.ip.split('/').next().unwrap_or("").to_string();
        if mac.is_empty() || ip.is_empty() {
            log_info("[kea] no mac/ip found — reservation skipped".into());
            return Ok(StepOutcome::Unchanged);
        }
        match crate::ops::kea::reserve(exec, kea_cfg, &ip, &mac, &m.hostname).await {
            Ok(()) => {
                log_info(format!("[kea] reserved {} for {}", ip, mac));
                Ok(StepOutcome::Changed)
            }
            Err(e) => {
                ctx.sink.emit(PipelineEvent::Line {
                    level: Level::Warn,
                    source: "HOST".into(),
                    msg: format!("[kea] reservation FAILED (deploy continues): {}", e),
                });
                Ok(StepOutcome::Unchanged)
            }
        }
    });

    step!(runner, "wait for systemd", {
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

    step!(runner, "bootstrap docker", {
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
    step!(runner, "runaway guards", {
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
    step!(runner, "commit intent", {
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
    step!(runner, "push files", {
        for f in &spec.files {
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
                    crate::ops::registry_cache::rewrite_compose(
                        &f.content,
                        cache,
                        &cache_up,
                        m.registry_login.as_ref().map(|r| r.registry.as_str()),
                    )
                }
                _ => f.content.clone(),
            };
            let changed = push_content(exec, m.vmid, &dest, &content, &perms).await?;
            if changed {
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
    });

    step!(runner, "start apps", {
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
            let pull = pct_sh(
                exec,
                m.vmid,
                &format!("cd '{}' && docker compose pull -q", dir),
                900,
            )
            .await?;
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
    step!(runner, "verify health", {
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
        step!(runner, "gateway route", {
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
        step!(runner, "grafana dashboard", {
            let dest = crate::ops::dashboard::dashboard_file(dir, &m.stack_name);
            let body = crate::ops::dashboard::dashboard_json(&m.stack_name, &m.apps);
            match push_content(exec, ctx.safety.gateway_vmid, &dest, &body, "644").await {
                Ok(changed) => {
                    if changed {
                        log_info(format!("[t2] {} (provisioning watcher reloads)", dest));
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

    // ── D3: garbage-collect apps removed from intent — stop + remove their
    // compose project and /opt dir; /appdata config dirs are kept.
    step!(runner, "garbage collect", {
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
    step!(runner, "native units", {
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
            let active = pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
            if active.stdout.trim() == "active" {
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

    step!(runner, "record state", {
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
            },
        );
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

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
