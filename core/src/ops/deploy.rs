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

    // ── E3: fresh/empty config dirs are refilled from the latest snapshot
    // BEFORE apps start. Backup-target trouble degrades to a loud warning,
    // never a blocked deploy (spec: upgraded-by-Kenny Must).
    step!(runner, "auto-restore check", {
        if m.storage.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut all_empty = true;
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
                all_empty = false;
                break;
            }
        }
        if !all_empty {
            return Ok(StepOutcome::Unchanged);
        }
        let bcfg = crate::ops::backup::BackupCfg::default();
        let has_snapshot = exec
            .run(&crate::ops::backup::restic_cmd(
                &bcfg,
                &m.stack_name,
                &["snapshots", "--last", "--json"],
                120,
            ))
            .await;
        match has_snapshot {
            Ok(out)
                if out.success() && out.stdout.trim() != "[]" && !out.stdout.trim().is_empty() =>
            {
                log_info("[e3] config dirs are empty and a snapshot exists — restoring".into());
                let restored = exec
                    .run(&crate::ops::backup::restic_cmd(
                        &bcfg,
                        &m.stack_name,
                        &["restore", "latest", "--target", "/"],
                        bcfg.snapshot_timeout_s,
                    ))
                    .await;
                match restored {
                    Ok(o) if o.success() => {
                        log_info("[e3] auto-restore complete".into());
                        Ok(StepOutcome::Changed)
                    }
                    _ => {
                        ctx.sink.emit(PipelineEvent::Line {
                            level: Level::Warn,
                            source: "HOST".into(),
                            msg: "[e3] AUTO-RESTORE FAILED — deploy continues with EMPTY config dirs; restore manually if this stack had data".into(),
                        });
                        Ok(StepOutcome::Unchanged)
                    }
                }
            }
            _ => {
                log_info("[e3] empty config dirs, no snapshot available — fresh stack".into());
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
                if m.lxc.protection {
                    set_args.push("--protection".into());
                    set_args.push("1".into());
                }
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
                if m.lxc.gpu {
                    run_ok(
                        exec,
                        &Cmd::new(
                            "pct",
                            &[
                                "set",
                                &vm,
                                "--dev0",
                                "/dev/dri/card0,gid=44",
                                "--dev1",
                                "/dev/dri/renderD128,gid=104",
                            ],
                            60,
                        ),
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
            if m.lxc.protection {
                // Hypervisor-level destroy refusal; gated destroy (C2) lifts
                // it deliberately before removal.
                args.push("--protection".into());
                args.push("1".into());
            }
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
            // H4: hardware passthrough flags. GPU via pct dev entries
            // (targeted gids — render/video — NOT the old ansible
            // chmod-0777-recurse). TUN needs raw lxc config lines that pct
            // has no flag for, appended to the container config.
            if m.lxc.gpu {
                run_ok(
                    exec,
                    &Cmd::new(
                        "pct",
                        &[
                            "set",
                            &vm,
                            "--dev0",
                            "/dev/dri/card0,gid=44",
                            "--dev1",
                            "/dev/dri/renderD128,gid=104",
                        ],
                        60,
                    ),
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
        guards::apply(exec, ctx.sink, m.vmid).await?;
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
    step!(runner, "push files", {
        for f in &spec.files {
            let dest = format!("/opt/{}/{}", m.stack_name, f.path);
            let perms = format!("{:o}", f.mode.unwrap_or(0o644));
            push_content(exec, m.vmid, &dest, &f.content, &perms).await?;
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

    // ── D3: garbage-collect apps removed from intent — stop + remove their
    // compose project and /opt dir; /appdata config dirs are kept.
    step!(runner, "garbage collect", {
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

    step!(runner, "record state", {
        let store = StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await?;
        // Preserve last_backup and the H8 enabled flag across redeploys —
        // parking is an explicit operator choice; refresh everything else.
        let (last_backup, enabled) = state
            .stacks
            .get(&m.stack_name)
            .map(|s| (s.last_backup, s.enabled))
            .unwrap_or((0, true));
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
