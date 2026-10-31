//! Stack lifecycle verbs: adopt, forget, guards, enable/disable, export/import, resize, exec, deploy dispatch.

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
    let homelab_client::cli_args::Invocation::Adopt { stack } = invocation(args) else {
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
    let homelab_client::cli_args::Invocation::Guards { vmid } = invocation(args) else {
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
    let homelab_client::cli_args::Invocation::Forget { stack } = invocation(args) else {
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
    let homelab_client::cli_args::Invocation::Enable { stack, enabled } = invocation(args) else {
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
    let homelab_client::cli_args::Invocation::Resize { stack } = invocation(args) else {
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
