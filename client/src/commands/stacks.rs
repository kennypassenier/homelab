//! TODO

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "adopt" => cmd_adopt(c).await,
        "guards" => cmd_guards(c).await,
        "forget" => cmd_forget(c).await,
        "enable" | "disable" => cmd_enable_disable(c).await,
        "export" => cmd_export(c).await,
        "import" => cmd_import(c).await,
        "resize" => cmd_resize(c).await,
        "templates" => cmd_templates(c).await,
        "template-build" => cmd_template_build(c).await,
        "exec" => cmd_exec(c).await,
        "new" => cmd_new(c).await,
        "presets" => cmd_presets().await,
        "plan" => cmd_plan(c).await,
        "deploy" => cmd_deploy(c).await,
        "apply" => cmd_apply(c).await,
        "wipe" => cmd_wipe(c).await,
        "prune-orphans" => cmd_prune_orphans(c).await,
        "destroy" => cmd_destroy(c).await,
        _ => return false,
    }
    true
}

/// C7: adopt a hand-built native-service container, and drive an
/// adopted one. The stack file is stacks/<name>/service.yml.
pub(crate) async fn cmd_adopt(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Adopt { stack } = invocation(&args) else {
        die("internal: adopt parsed as another verb")
    };
    let dir = stack_dir(&stack);
    let path = dir.join("service.yml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
    let m: homelab_proto::NativeServiceManifest =
        serde_yaml::from_str(&raw).unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
    if let Err(problems) = homelab_core::native::validate_native(&m) {
        die(&format!("service.yml invalid: {}", problems.join("; ")));
    }
    println!(
        "{}▶ adopt {} :: CT {} · unit {} · never restarts anything{}",
        C_CYAN, m.stack_name, m.vmid, m.unit, C_RESET
    );
    rpc(host, token, Command::AdoptService(Box::new(m))).await;
}

/// Y4: the client contributes what only it can see — the vmid each
/// stack directory claims — and the host contributes the rest.
/// G1: the runaway guards, for a container the orchestrator did not
/// build. They are what keeps a container able to run for years.
pub(crate) async fn cmd_guards(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Guards { vmid } = invocation(&args) else {
        die("internal: guards parsed as another verb")
    };
    println!(
        "{}▶ guards :: CT {} — log caps, journald limits, logrotate, weekly prune{}",
        C_CYAN, vmid, C_RESET
    );
    rpc(host, token, Command::ApplyGuards { vmid }).await;
}

pub(crate) async fn cmd_forget(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Forget { stack } = invocation(&args) else {
        die("internal: forget parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&stack);
    rpc(host, token, Command::ForgetStack { stack }).await;
}

/// H8 (light): park / unpark a stack for the nightly scheduler.
pub(crate) async fn cmd_enable_disable(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Enable { stack, enabled } = invocation(&args) else {
        die("internal: enable parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&stack);
    rpc(host, token, Command::SetStackEnabled { stack, enabled }).await;
}

pub(crate) async fn cmd_export(c: &Ctx) {
    let args = &c.args;
    // D11: single-file bundle, never secrets.
    let dir = stack_dir(
        args.get(2)
            .unwrap_or_else(|| die("usage: homelab export stacks/<name> [out.yml]")),
    );
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "stack".into());
    let out = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| format!("{}-bundle.yml", name));
    match spec::export_bundle(&dir, &out) {
        Ok(n) => println!(
            "{}✓ exported{} — {} ({} file(s), no secrets)",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&format!("export: {}", e)),
    }
}

pub(crate) async fn cmd_import(c: &Ctx) {
    let args = &c.args;
    let usage = "usage: homelab import <bundle.yml> <new-name> <vmid>";
    let bundle = args.get(2).unwrap_or_else(|| die(usage));
    let name = args.get(3).unwrap_or_else(|| die(usage));
    let vmid: u16 = args
        .get(4)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| die(usage));
    match spec::import_bundle(Path::new(bundle), &stacks_base(), name, vmid) {
        Ok(dest) => {
            // Validate what we just wrote with the same validator as deploy.
            match spec::build_spec(&dest)
                .and_then(|s| homelab_core::manifest::validate(&s).map_err(|e| e.to_string()))
            {
                Ok(()) => println!(
                    "{}✓ imported{} — {} (vmid {}) :: add .env files if the apps need secrets, then deploy",
                    C_GREEN,
                    C_RESET,
                    dest.display(),
                    vmid
                ),
                Err(e) => die(&format!("imported but invalid: {}", e)),
            }
        }
        Err(e) => die(&format!("import: {}", e)),
    }
}

pub(crate) async fn cmd_resize(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // C4: apply the manifest's resources to the live container.
    let homelab_client::cli_args::Invocation::Resize { stack } = invocation(&args) else {
        die("internal: resize parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // gap-34: resize sends only the manifest (`ApplyResources`
    // takes `Box<StackManifest>`) — building the full spec ran
    // latch for any stack with `latch_secrets`/`latch_files` and
    // then threw every secret away unread.
    let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
    println!(
        "{}▶ resize {} :: {} MiB / {} cores / {}G{}",
        C_CYAN,
        manifest.stack_name,
        manifest.resources.memory_mb,
        manifest.resources.cores,
        manifest.resources.disk_gb,
        C_RESET
    );
    rpc(host, token, Command::ApplyResources(Box::new(manifest))).await;
}

pub(crate) async fn cmd_templates(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::ListTemplates).await;
}

pub(crate) async fn cmd_template_build(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // B8: bake the golden template. Temp vmid defaults to 999.
    // O2: `--privileged` builds the second template. CT 105 and 106
    // are privileged and a clone cannot change that, so they need one.
    // `--base <vztmpl>` picks the OS. Absent keeps the host's default,
    // so this stays the command it has always been for a caller that
    // does not care which Debian it is baking.
    // fix-110 (see REGISTER.md): an argument that does not parse is
    // refused.
    let homelab_client::version::TemplateArgs {
        temp_vmid,
        version,
        unprivileged,
        base_template,
    } = homelab_client::version::template_build_args(&args[2..]).unwrap_or_else(|e| die(&e));
    let shown = base_template
        .as_deref()
        .map(homelab_core::ops::template::os_slug)
        .unwrap_or_else(|| "the host's default OS".into());
    println!(
        "{}▶ template build :: {}-homelab-v{}{} on temp vmid {}{}",
        C_CYAN,
        shown,
        version,
        if unprivileged { "" } else { "-priv" },
        temp_vmid,
        C_RESET
    );
    rpc(
        host,
        token,
        Command::BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
            base_template,
        },
    )
    .await;
}

pub(crate) async fn cmd_exec(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // A6: requires exec_enabled = true in the host config.
    let vmid: u16 = args
        .get(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| die("usage: homelab exec <vmid> <command...>"));
    let command = args[3..].join(" ");
    if command.is_empty() {
        die("usage: homelab exec <vmid> <command...>");
    }
    rpc(host, token, Command::ExecIn { vmid, command }).await;
}

pub(crate) async fn cmd_new(c: &Ctx) {
    let args = &c.args;
    // T65 / F136: a stack could only ever be created through the TUI
    // wizard. Twenty-one commands and not one of them scaffolded, so
    // every stack made without the TUI was hand-written — which is
    // how three stack files ended up claiming live vmids.
    //
    // Same scaffolder, same preset catalogue, same defaults as the
    // wizard: `homelab new <name> --preset <p> --vmid <n> [...]`.
    let name = args
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            die("usage: homelab new <name> --preset <preset> --vmid <n> \
                 [--ram MiB] [--cores N] [--disk GiB] [--swap MiB] \
                 [--no-data /appdata/<stack>/<app>-config]")
        });
    let flag = |k: &str| -> Option<String> {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let num = |k: &str, what: &str| -> Option<u64> {
        flag(k).map(|v| {
            v.parse()
                .unwrap_or_else(|_| die(&format!("{} takes a number, got '{}'", what, v)))
        })
    };
    let presets_path = in_repo("presets");
    let presets_dir = presets_path.as_path();
    let presets = homelab_client::scaffold::scan_presets(presets_dir);
    let preset_name = flag("--preset")
        .unwrap_or_else(|| die("--preset is required; `homelab presets` lists them"));
    let preset = presets
        .iter()
        .find(|p| p.name == preset_name)
        .unwrap_or_else(|| {
            die(&format!(
                "no preset '{}' — `homelab presets` lists what there is",
                preset_name
            ))
        });
    let vmid = num("--vmid", "--vmid")
        .unwrap_or_else(|| die("--vmid is required: the address and hostname are derived from it"))
        as u16;
    let d = homelab_client::scaffold::StackDefaults::default();
    let ram = num("--ram", "--ram").unwrap_or(preset.meta.ram_mb as u64) as u32;
    // Every `--no-data` flag names one path that keeps nothing of its
    // own (D79). The wizard asks this per directory; here it is
    // repeatable so a script can say the same thing.
    let no_data: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "--no-data")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();
    let params = homelab_client::scaffold::StackParams {
        name: &name,
        vmid,
        ram_mb: ram,
        cores: num("--cores", "--cores").unwrap_or(preset.meta.cores.unwrap_or(2) as u64) as u16,
        disk_gb: num("--disk", "--disk").unwrap_or(preset.meta.disk_gb.unwrap_or(8) as u64) as u16,
        swap_mb: Some(num("--swap", "--swap").unwrap_or(d.swap_for(ram) as u64) as u32),
        preset: Some(preset),
        no_data_paths: &no_data,
    };
    match homelab_client::scaffold::scaffold_stack(&stacks_base(), presets_dir, &params) {
        Ok(s) => {
            println!(
                "{}✓ scaffolded {}{} — {} file(s)",
                C_GREEN,
                s.dir.display(),
                C_RESET,
                s.files.len()
            );
            for f in &s.files {
                println!("    {}", f);
            }
            println!(
                "\n  next: read the compose files, then `homelab plan stacks/{}`",
                name
            );
        }
        Err(e) => die(&e),
    }
}

pub(crate) async fn cmd_presets() {
    // G2: list the data-driven preset catalog (local, no network).
    for pr in homelab_client::scaffold::scan_presets(&in_repo("presets")) {
        let src = if pr.dir.is_some() {
            ""
        } else {
            " (built-in fallback)"
        };
        println!(
            "{:<14} {:>5} MiB  {}  [{}]{}",
            pr.name,
            pr.meta.ram_mb,
            pr.meta.description,
            if pr.apps.is_empty() {
                "no apps".to_string()
            } else {
                pr.apps.join(", ")
            },
            src
        );
    }
}

pub(crate) async fn cmd_plan(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // D6/D10: validate locally first — this much never needs the
    // network, and a bad stack file fails before anything is asked
    // of the host.
    let dir = &stack_dir(
        args.get(2)
            .unwrap_or_else(|| die("usage: homelab plan stacks/<name>")),
    );
    let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
    check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
    match homelab_core::manifest::validate(&spec) {
        Ok(()) => println!(
            "{}✓ valid{} — {} would deploy vmid {}: {} file(s), {} env(s)",
            C_GREEN,
            C_RESET,
            spec.manifest.stack_name,
            spec.manifest.vmid,
            spec.files.len(),
            spec.env.len()
        ),
        Err(e) => die(&format!("validation failed: {}", e)),
    }
    // fix-100 (apply-no-confirm-creates-drill, 2026-09-27): the
    // per-file diff this verb never had, now that `apply` has one
    // for every stack at once — this reads one stack's, with
    // HOMELAB_TOKEN set (D10's "no network" promise is kept when
    // it is not: validation above already ran and stands).
    if token.is_empty() {
        println!(
            "{}  (per-file diff needs HOMELAB_TOKEN — validation above stands on its \
             own){}",
            C_DIM, C_RESET
        );
    } else {
        match rpc_reply(
            host,
            token,
            Command::GetApplied {
                stack: spec.manifest.stack_name.clone(),
            },
        )
        .await
        {
            Some(r) if r.ok => {
                match serde_json::from_str::<Vec<homelab_proto::FileBlob>>(&r.message) {
                    Ok(applied) => {
                        let changes = homelab_client::apply::file_changes(&spec.files, &applied);
                        if changes.is_empty() {
                            println!(
                                "{}  files unchanged (secrets or settings differ){}",
                                C_DIM, C_RESET
                            );
                        }
                        for c in changes {
                            println!("  {}", c);
                        }
                        for r in homelab_client::apply::native_restarts(&spec.files, &applied) {
                            println!("{}  ↻ {}{}", C_YELLOW, r, C_RESET);
                        }
                    }
                    Err(_) => println!(
                        "{}  (could not read what the host applied — the host answered: \
                         {}){}",
                        C_DIM, r.message, C_RESET
                    ),
                }
            }
            Some(_not_ok) => println!(
                "{}  new: the host has never applied {} — the deploy would create CT \
                 {}{}",
                C_DIM, spec.manifest.stack_name, spec.manifest.vmid, C_RESET
            ),
            None => println!(
                "{}  (the host did not answer — per-file diff skipped, validation above \
                 stands){}",
                C_DIM, C_RESET
            ),
        }
    }
}

pub(crate) async fn cmd_deploy(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Deploy { stack, force } = invocation(&args) else {
        die("internal: deploy parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
    // D10: fail fast client-side before opening a connection.
    if let Err(e) = homelab_core::manifest::validate(&spec) {
        die(&format!("validation failed: {}", e));
    }
    check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
    // arch-deploy-guard: not over a deploy this tree has not seen.
    if !force {
        let (ok, fleet) = rpc_collect(host, token, Command::GetState).await;
        if let (true, Some(fleet)) = (ok, fleet)
            && let Err(e) = deploy_guard(Path::new(dir), &spec.manifest.stack_name, &fleet, false)
        {
            die(&e);
        }
    }
    let ok = deploy_spec(host, token, spec).await;
    std::process::exit(if ok { 0 } else { 1 });
}

/// ask-8 (Kenny, 2026-09-27): the whole stacks directory against the
/// host. Changed stacks are deployed; a stack in host state whose
/// directory is gone is destroyed only after its name is typed. The
/// nightly round never does this — it never destroys anything.
pub(crate) async fn cmd_apply(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let base = args
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| stacks_base().display().to_string());
    let skip_backup = args.iter().any(|a| a == "--no-backup");
    // fix-100 (see REGISTER.md)
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let assume_yes = args.iter().any(|a| a == "--yes");
    let plan_only = args.iter().any(|a| a == "--plan");
    let base_path = Path::new(&base);
    if !base_path.is_dir() {
        die(&format!(
            "no stacks directory at '{}' — run this from the repository root, or pass \
             the path: homelab apply ~/Projects/homelab/stacks",
            base
        ));
    }
    check_fleet_routes(base_path);
    let (ok, fleet) = rpc_collect(host, token, Command::GetState).await;
    let fleet = match fleet {
        Some(f) if ok => f,
        _ => die("could not read the host's state — nothing applied"),
    };
    let host_pairs: Vec<(String, String)> = fleet
        .stacks
        .iter()
        .map(|s| (s.name.clone(), s.applied_hash.clone()))
        .collect();
    // fix-192: the host's per-component digests, keyed by name, for
    // the reason text below — an empty manifest digest is the same
    // "nothing recorded" case as no record at all.
    let host_digests: std::collections::BTreeMap<String, homelab_core::manifest::ComponentDigests> =
        fleet
            .stacks
            .iter()
            .map(|s| (s.name.clone(), s.component_digests.clone()))
            .collect();
    // Every stack is built and validated before anything is sent: a
    // stack file that does not build stops the whole apply rather
    // than leaving the fleet half-applied.
    let mut local: Vec<(String, String)> = Vec::new();
    let mut specs: std::collections::BTreeMap<String, homelab_proto::DeploySpec> =
        std::collections::BTreeMap::new();
    let declared = spec::declared_stacks(base_path);
    let ephemeral: Vec<String> = spec::scan_local_stacks(base_path)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| !declared.iter().any(|(d, _)| d == n))
        .collect();
    for (name, dir) in declared {
        let sp = spec::build_spec(&dir)
            .unwrap_or_else(|e| die(&format!("{}: {} — nothing applied", name, e)));
        if let Err(e) = homelab_core::manifest::validate(&sp) {
            die(&format!(
                "{}: validation failed: {} — nothing applied",
                name, e
            ));
        }
        local.push((name.clone(), homelab_core::manifest::intent_hash(&sp)));
        specs.insert(name, sp);
    }
    let dirs: Vec<String> = std::fs::read_dir(base_path)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    let plan = homelab_client::apply::plan(&local, &dirs, &host_pairs);
    println!(
        "{}▶ apply :: {} to deploy · {} unchanged · {} gone from the files{}",
        C_CYAN,
        plan.deploy.len(),
        plan.unchanged.len(),
        plan.destroy.len(),
        C_RESET
    );
    for n in &plan.unchanged {
        println!("{}  = {}{}", C_DIM, n, C_RESET);
    }
    for n in &ephemeral {
        println!(
            "{}  · {} — ephemeral: deployed by name only, never by apply{}",
            C_DIM, n, C_RESET
        );
    }
    for n in &plan.deploy {
        println!("  ↑ {}", n);
        // fix-100: what the deploy changes, file by file, removals
        // included — the host's applied files against the local ones.
        let Some(sp) = specs.get(n) else { continue };
        if !host_pairs.iter().any(|(h, _)| h == n) {
            println!(
                "{}      new: creates CT {}{}",
                C_DIM, sp.manifest.vmid, C_RESET
            );
            continue;
        }
        let applied: Vec<homelab_proto::FileBlob> =
            rpc_reply(host, token, Command::GetApplied { stack: n.clone() })
                .await
                .filter(|r| r.ok)
                .and_then(|r| serde_json::from_str(&r.message).ok())
                .unwrap_or_else(|| {
                    die(&format!(
                        "could not read what the host applied for {} — nothing applied",
                        n
                    ))
                });
        let changes = homelab_client::apply::file_changes(&sp.files, &applied);
        if changes.is_empty() {
            // fix-192 (media-redeploys-without-changing, Kenny
            // 2026-10-02): no file differs, but the intent hash
            // still moved — say which component did, from the
            // digests alone, instead of the old generic "settings
            // differ" guess.
            let local_digest = homelab_core::manifest::component_digests(sp);
            let applied_digest = host_digests.get(n).filter(|d| !d.manifest.is_empty());
            println!(
                "{}      {}{}",
                C_DIM,
                homelab_client::apply::redeploy_reason(&local_digest, applied_digest),
                C_RESET
            );
        }
        for c in changes {
            println!("      {}", c);
        }
        // fix-159: which running native units the deploy restarts.
        for r in homelab_client::apply::native_restarts(&sp.files, &applied) {
            println!("{}      ↻ {}{}", C_YELLOW, r, C_RESET);
        }
    }
    for n in &plan.destroy {
        println!(
            "{}  ✗ {} — in host state, no {}/{}/{}",
            C_YELLOW, n, base, n, C_RESET
        );
    }
    // fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift):
    // the plan alone, with an exit code a script can test.
    if plan_only {
        std::process::exit(homelab_client::apply::plan_exit_code(&plan));
    }
    let answer = if dry_run || assume_yes || plan.deploy.is_empty() {
        None
    } else {
        use std::io::Write as _;
        eprint!("Deploy these {} stack(s)? [y/N] ", plan.deploy.len());
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .ok()
            .filter(|n| *n > 0)
            .map(|_| line)
    };
    match homelab_client::apply::decide(
        dry_run,
        assume_yes || plan.deploy.is_empty(),
        answer.as_deref(),
    ) {
        homelab_client::apply::Decision::Preview => {
            println!("dry run — nothing deployed, nothing destroyed");
            std::process::exit(0);
        }
        homelab_client::apply::Decision::Decline => {
            die("not confirmed — nothing deployed, nothing destroyed")
        }
        homelab_client::apply::Decision::Deploy => {}
    }
    // arch-deploy-guard: every planned stack is checked before the
    // first one is sent, so a refusal leaves nothing half-applied.
    let force = args.iter().any(|a| a == "--force");
    for name in &plan.deploy {
        if let Err(e) = deploy_guard(&base_path.join(name), name, &fleet, force) {
            die(&e);
        }
    }
    for name in &plan.deploy {
        let Some(sp) = specs.remove(name) else {
            continue;
        };
        if !deploy_spec(host, token, sp).await {
            die(&format!(
                "deploy of {} failed — apply stopped here; nothing after it was \
                 deployed and nothing was destroyed",
                name
            ));
        }
    }
    let mut all_ok = true;
    if !plan.destroy.is_empty() {
        println!(
            "{}! {} stack(s) run on the host but are gone from the files. Each is \
             destroyed from the manifest the host recorded — backed up first{}, its \
             /appdata, backups and vault kept (see `homelab check`) — only after you \
             type its name. Enter keeps it.{}",
            C_YELLOW,
            plan.destroy.len(),
            if skip_backup {
                " (NOT: --no-backup)"
            } else {
                ""
            },
            C_RESET
        );
        for name in &plan.destroy {
            let typed = read_typed(&format!("Type the stack name '{}' to destroy it: ", name));
            if &typed != name {
                println!("  kept {} — nothing destroyed", name);
                continue;
            }
            all_ok &= rpc_with(
                host,
                token,
                Command::DestroyRecorded {
                    stack: name.clone(),
                    confirm: typed,
                    skip_backup,
                },
            )
            .await;
        }
    }
    std::process::exit(if all_ok { 0 } else { 1 });
}

/// ask-9: delete what a retired stack, app or unit kept — never
/// automatic, always after the list and the typed name.
pub(crate) async fn cmd_wipe(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Wipe { name, yes } = invocation(&args) else {
        die("internal: wipe parsed as another verb")
    };
    let name = homelab_client::repo_config::stack_name(&name);
    let listed = rpc_with(
        host,
        token,
        Command::WipeRetired {
            name: name.clone(),
            confirm: None,
        },
    )
    .await;
    if !listed {
        std::process::exit(1);
    }
    // cli-yes: a line copied from the dashboard's form, where the
    // name was already typed, carries --yes.
    let typed = if yes {
        name.clone()
    } else {
        read_typed(&format!(
            "Type '{}' to delete all of the above, permanently: ",
            name
        ))
    };
    if typed != name {
        die("name mismatch — nothing deleted");
    }
    rpc(
        host,
        token,
        Command::WipeRetired {
            name,
            confirm: Some(typed),
        },
    )
    .await;
}

pub(crate) async fn cmd_prune_orphans(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // Kenny's H2b made this the only remover of files the repository
    // no longer has; since ask-8 (2026-09-27) the deploy removes them
    // itself, so this is mostly a no-op kept for a container that has
    // not been deployed since. Same typed confirmation as before.
    let homelab_client::cli_args::Invocation::PruneOrphans { stack, yes } = invocation(&args)
    else {
        die("internal: prune-orphans parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // gap-34: `orphan_files_keeping` (core/src/ops/deploy.rs) reads
    // only `spec.files` — never `.env` or `.secret_files` — so
    // latch never needed to run here. `build_spec` ran it anyway
    // for any stack with `latch_secrets`/`latch_files`.
    let spec = spec::build_spec_files_only(Path::new(dir)).unwrap_or_else(|e| die(&e));
    let stack = spec.manifest.stack_name.clone();
    let confirm = if yes {
        stack.clone()
    } else {
        eprint!(
            "{}Type the stack name '{}' to remove the files the repository no longer \
             has (the deploy log lists them): {}",
            C_RED, stack, C_RESET
        );
        use std::io::Write as _;
        std::io::stderr().flush().ok();
        let mut typed = String::new();
        std::io::stdin().read_line(&mut typed).ok();
        typed.trim().to_string()
    };
    if confirm != stack {
        die("name mismatch — nothing removed");
    }
    rpc(
        host,
        token,
        Command::PruneOrphans {
            manifest: Box::new(spec.manifest.clone()),
            spec: Box::new(spec),
            confirm,
        },
    )
    .await;
}

pub(crate) async fn cmd_destroy(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Destroy {
        stack,
        skip_backup,
        yes,
    } = invocation(&args)
    else {
        die("internal: destroy parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // ask-8: a stack whose directory is gone is destroyed from the
    // manifest the host recorded when it last applied it.
    if !Path::new(dir).join("lxc-compose.yml").exists() {
        let stack = Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        eprintln!(
            "{}! no {}/lxc-compose.yml — destroying '{}' from the manifest the host \
             recorded{}",
            C_YELLOW,
            dir.display(),
            stack,
            C_RESET
        );
        let confirm = if yes {
            stack.clone()
        } else {
            read_typed(&format!(
                "Type the stack name '{}' to confirm destroy: ",
                stack
            ))
        };
        if confirm != stack {
            die("name mismatch — aborted");
        }
        rpc(
            host,
            token,
            Command::DestroyRecorded {
                stack,
                confirm,
                skip_backup,
            },
        )
        .await;
    }
    // gap-34: `DestroyStack` sends only the manifest — building
    // the full spec ran latch for any stack with
    // `latch_secrets`/`latch_files` and then threw every secret
    // away unread (F291 made the same point for `backup`).
    let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
    let stack = &manifest.stack_name;
    // Kenny's B2: the destroy backs up first and refuses if that
    // fails. Skipping is deliberate and says so out loud.
    if skip_backup {
        eprintln!(
            "{}! --no-backup: destroying without the backup that would otherwise be \
             taken first{}",
            C_YELLOW, C_RESET
        );
    }
    // C2: typed-name confirmation, exactly like the TUI; cli-yes:
    // a line copied from the dashboard's form, where the name was
    // typed already, carries --yes.
    let confirm = if yes {
        stack.clone()
    } else {
        eprint!(
            "{}Type the stack name '{}' to confirm destroy: {}",
            C_RED, stack, C_RESET
        );
        use std::io::Write as _;
        std::io::stderr().flush().ok();
        let mut typed = String::new();
        std::io::stdin().read_line(&mut typed).ok();
        typed.trim().to_string()
    };
    if &confirm != stack {
        die("name mismatch — aborted");
    }
    rpc(
        host,
        token,
        Command::DestroyStack {
            manifest: Box::new(manifest),
            confirm,
            skip_backup,
        },
    )
    .await;
}
