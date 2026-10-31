//! TODO

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "install-native" => cmd_install_native(c).await,
        "update-native" => cmd_update_native(c).await,
        "release-update-native" => cmd_release_update_native(c).await,
        "rollback-native" => cmd_rollback_native(c).await,
        "tool" => cmd_tool(c).await,
        "update" => cmd_update(c).await,
        "release-update" => cmd_release_update(c).await,
        "self-install" => cmd_self_install(c).await,
        "self-update" => cmd_self_update(c).await,
        "update-policy" => cmd_update_policy(c).await,
        _ => return false,
    }
    true
}

/// T11: install a native service into a container the deploy has
/// already created. The other half of C7 — until now the orchestrator
/// could take over a hand-built container and could not build one.
pub(crate) async fn cmd_install_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let dir = stack_dir(args.get(2).unwrap_or_else(|| {
        die("usage: homelab install-native stacks/<name>[/<unit>] [<tag> | --file <path>]")
    }));
    let path = dir.join("service.yml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
    let m: homelab_proto::NativeServiceManifest =
        serde_yaml::from_str(&raw).unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
    if let Err(problems) = homelab_core::native::validate_native(&m) {
        die(&format!("service.yml invalid: {}", problems.join("; ")));
    }
    // B7: `--file <path>` hands the bytes over from disk — the
    // rollback drill's way of installing a fake service, good and
    // then deliberately broken. A release stays the normal source.
    let source = homelab_client::release::install_source(&args[3..]).unwrap_or_else(|e| die(&e));
    let from_file = matches!(source, homelab_client::release::InstallSource::File(_));
    let repo = match m.release_repo.clone() {
        Some(r) => r,
        None if from_file => String::new(),
        None => die(&format!(
            "{} declares no release_repo — this service is adopt-only, and where its \
             binary comes from is not written down anywhere. Add release_repo to its \
             service.yml rather than installing by hand again",
            m.unit
        )),
    };
    // The unit file lives beside the service file, or in the unit's
    // own directory when several services share one stack.
    let unit_name = format!("{}.service", m.unit);
    let candidates = [dir.join(&unit_name), dir.join(&m.unit).join(&unit_name)];
    let unit_file = candidates
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| {
            die(&format!(
                "no {} found beside {} — the file that makes the service exist is not \
                 in the repository, so a rebuilt container would have the binary and \
                 nothing to run it",
                unit_name,
                dir.display()
            ))
        });
    let asset = m.asset_name().to_string();
    let binary_b64 = match source {
        homelab_client::release::InstallSource::File(path) => {
            let (b64, sha) = homelab_client::release::stage_file(&path).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ install-native {} :: CT {} · from file {} (sha256 {}){}",
                C_CYAN, m.unit, m.vmid, path, sha, C_RESET
            );
            b64
        }
        homelab_client::release::InstallSource::Release(tag) => {
            let tag = match tag {
                Some(t) => t,
                None => homelab_client::release::latest_tag_of(&repo).unwrap_or_else(|| {
                    die(&format!("no release found in {} (gh authenticated?)", repo))
                }),
            };
            println!(
                "{}▶ install-native {} :: CT {} · {} {} from {}{}",
                C_CYAN, m.unit, m.vmid, asset, tag, repo, C_RESET
            );
            let b64 = homelab_client::release::stage_asset(&repo, &tag, &asset)
                .unwrap_or_else(|e| die(&e));
            println!(
                "{}✓ checksum verified — shipping over the line{}",
                C_GREEN, C_RESET
            );
            b64
        }
    };
    if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
        die(&why);
    }
    rpc(
        host,
        token,
        Command::InstallNative {
            manifest: Box::new(m),
            binary_b64,
            unit_file,
        },
    )
    .await;
}

pub(crate) async fn cmd_update_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::UpdateNative { stack } = invocation(&args) else {
        die("internal: update-native parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&stack);
    rpc(host, token, Command::UpdateNative { stack }).await;
}

/// B1: the orchestrator's own release update of a native stack, now.
pub(crate) async fn cmd_release_update_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::ReleaseUpdateNative { stack } = invocation(&args)
    else {
        die("internal: release-update-native parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&stack);
    rpc(host, token, Command::ReleaseUpdateNative { stack }).await;
}

/// fix-114: back to a native unit's kept previous binary.
pub(crate) async fn cmd_rollback_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::RollbackNative { stack, unit } = invocation(&args)
    else {
        die("internal: rollback-native parsed as another verb")
    };
    rpc(host, token, Command::RollbackNative { stack, unit }).await;
}

/// Managed tools on the host (latch): `homelab tool status|update|rollback`.
pub(crate) async fn cmd_tool(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match homelab_client::tools_cli::command(&args) {
        Ok(command) => rpc(host, token, command).await,
        Err(e) => die(&e),
    }
}

pub(crate) async fn cmd_update(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Update { stack, app } = invocation(&args) else {
        die("internal: update parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // F294 (see REGISTER.md): an update pulls an image and recreates
    // a container on the host, so it needs only the manifest, the
    // same fault F291 closed for `backup`, still open in
    // the two verbs beside it.
    let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
    println!(
        "{}▶ update {} :: {}{}",
        C_CYAN,
        manifest.stack_name,
        app.as_deref().unwrap_or("all apps"),
        C_RESET
    );
    rpc(
        host,
        token,
        Command::UpdateStack {
            manifest: Box::new(manifest),
            app,
        },
    )
    .await;
}

pub(crate) async fn cmd_release_update(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // H7: fetch the newest GitHub release, verify its checksum, and
    // ship it over the line — the host's selfcheck/rollback pipeline
    // takes it from there.
    let tag = match args.get(2).cloned() {
        Some(t) => t,
        None => homelab_client::release::latest_release_tag()
            .unwrap_or_else(|| die("no release found (gh authenticated? release exists?)")),
    };
    println!(
        "{}▶ release update :: staging {} from GitHub{}",
        C_CYAN, tag, C_RESET
    );
    match homelab_client::release::stage_release(&tag) {
        Ok((binary_b64, proof)) => {
            if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
                die(&why);
            }
            println!(
                "{}✓ signature and checksum verified — shipping over the line{}",
                C_GREEN, C_RESET
            );
            // fix-121: done means the shipped version answered, not
            // that a restart was scheduled.
            let expected = homelab_client::release::expected_host_version(&tag);
            let ok = rpc_with(host, token, Command::SelfUpdateHost { binary_b64, proof }).await
                && wait_for_updated_host(host, token, expected).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        Err(e) => die(&e),
    }
}

/// fix-105 (older-client-no-warning, 2026-09-27): the client updates
/// itself the way `release-update` updates the host — the release's
/// own binary, checksum-verified, then put where this one runs from.
/// It was a five-line gh/sha256sum/install routine per workstation.
pub(crate) async fn cmd_self_install(c: &Ctx) {
    let args = &c.args;
    let tag = match args.get(2).cloned() {
        Some(t) => t,
        None => homelab_client::release::latest_release_tag()
            .unwrap_or_else(|| die("no release found (gh authenticated? release exists?)")),
    };
    let target = std::env::current_exe()
        .unwrap_or_else(|e| die(&format!("cannot tell where this client runs from: {}", e)));
    println!(
        "{}▶ self-install :: client {} from GitHub into {}{}",
        C_CYAN,
        tag,
        target.display(),
        C_RESET
    );
    let bytes = homelab_client::release::stage_client(&tag).unwrap_or_else(|e| die(&e));
    println!("{}✓ signature and checksum verified{}", C_GREEN, C_RESET);
    homelab_client::release::install_binary(&bytes, &target).unwrap_or_else(|e| die(&e));
    println!(
        "{}✓ client {} installed{} — the next command runs it",
        C_GREEN, tag, C_RESET
    );
}

pub(crate) async fn cmd_self_update(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    // H5: ship a new HOST binary over the line; the host selfchecks,
    // installs with an armed rollback, and restarts itself.
    let path = args
        .get(2)
        .unwrap_or_else(|| die("usage: homelab self-update <path-to-homelab-host-binary>"));
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(&format!("{}: {}", path, e)));
    use base64::Engine as _;
    let binary_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
        die(&why);
    }
    println!(
        "{}▶ self-update :: shipping {} ({} KiB){}",
        C_YELLOW,
        path,
        bytes.len() / 1024,
        C_RESET
    );
    // fix-121: the version inside a local file is not known here, so
    // whatever answers after the restart is the one checked.
    // redesign-host-4: a file carries no release signature, so the
    // host will report itself as not a signed release.
    let ok = rpc_with(
        host,
        token,
        Command::SelfUpdateHost {
            binary_b64,
            proof: None,
        },
    )
    .await
        && wait_for_updated_host(host, token, None).await;
    std::process::exit(if ok { 0 } else { 1 });
}

/// fix-144 (expert panel 2026-09-27, update-policy-doc-drift): the
/// policy table in UPDATE_POLICY.md is written from the stack files,
/// like the runbook, and a test fails when the committed one is stale.
pub(crate) async fn cmd_update_policy(c: &Ctx) {
    let args = &c.args;
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "docs/deployment/UPDATE_POLICY.md".into());
    match homelab_client::updatepolicy::write_update_policy(Path::new("stacks"), Path::new(&out)) {
        Ok(n) => println!(
            "{}✓ update policy written{} — {} ({} row(s))",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&format!("update-policy: {}", e)),
    }
}
