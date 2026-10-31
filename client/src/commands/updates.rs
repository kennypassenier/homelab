//! Update verbs: tool updates, host and client self-update, update policy.

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

/// Managed tools on the host (latch): `homelab tool status|update|rollback`.
pub(crate) async fn cmd_tool(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match homelab_client::tools_cli::command(args) {
        Ok(command) => rpc(host, token, command).await,
        Err(e) => die(&e),
    }
}

pub(crate) async fn cmd_update(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Update { stack, app } = invocation(args) else {
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
