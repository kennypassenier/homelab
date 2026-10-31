//! Installing and updating native services.

use super::*;

pub(crate) fn read_native_manifest(dir: &Path) -> homelab_proto::NativeServiceManifest {
    let path = dir.join("service.yml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
    let m: homelab_proto::NativeServiceManifest =
        serde_yaml::from_str(&raw).unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
    if let Err(problems) = homelab_core::native::validate_native(&m) {
        die(&format!("service.yml invalid: {}", problems.join("; ")));
    }
    m
}

/// The unit file lives beside the service file, or in the unit's
/// own directory when several services share one stack.
pub(crate) fn read_unit_file(dir: &Path, unit: &str) -> String {
    let unit_name = format!("{}.service", unit);
    let candidates = [dir.join(&unit_name), dir.join(unit).join(&unit_name)];
    candidates
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
        })
}

/// The repository a release comes from; empty when installing from a file.
pub(crate) fn release_repo_of(m: &homelab_proto::NativeServiceManifest, from_file: bool) -> String {
    match m.release_repo.clone() {
        Some(r) => r,
        None if from_file => String::new(),
        None => die(&format!(
            "{} declares no release_repo — this service is adopt-only, and where its \
             binary comes from is not written down anywhere. Add release_repo to its \
             service.yml rather than installing by hand again",
            m.unit
        )),
    }
}

/// The binary, base64, from the file or the release named.
pub(crate) fn stage_binary(
    m: &homelab_proto::NativeServiceManifest,
    source: homelab_client::release::InstallSource,
    repo: &str,
) -> String {
    match source {
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
                None => homelab_client::release::latest_tag_of(repo).unwrap_or_else(|| {
                    die(&format!("no release found in {} (gh authenticated?)", repo))
                }),
            };
            let asset = m.asset_name().to_string();
            println!(
                "{}▶ install-native {} :: CT {} · {} {} from {}{}",
                C_CYAN, m.unit, m.vmid, asset, tag, repo, C_RESET
            );
            let b64 = homelab_client::release::stage_asset(repo, &tag, &asset)
                .unwrap_or_else(|e| die(&e));
            println!(
                "{}✓ checksum verified — shipping over the line{}",
                C_GREEN, C_RESET
            );
            b64
        }
    }
}

pub(crate) async fn cmd_install_native(c: &Ctx) {
    let (host, token, args) = (&c.host, &c.token, &c.args);
    let dir = stack_dir(args.get(2).unwrap_or_else(|| {
        die("usage: homelab install-native stacks/<name>[/<unit>] [<tag> | --file <path>]")
    }));
    let m = read_native_manifest(&dir);
    // B7: `--file <path>` hands the bytes over from disk — the
    // rollback drill's way of installing a fake service, good and
    // then deliberately broken. A release stays the normal source.
    let source = homelab_client::release::install_source(&args[3..]).unwrap_or_else(|e| die(&e));
    let from_file = matches!(source, homelab_client::release::InstallSource::File(_));
    let repo = release_repo_of(&m, from_file);
    let unit_file = read_unit_file(&dir, &m.unit);
    let binary_b64 = stage_binary(&m, source, &repo);
    if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
        die(&why);
    }
    let install = Command::InstallNative {
        manifest: Box::new(m),
        binary_b64,
        unit_file,
    };
    rpc(host, token, install).await;
}

pub(crate) async fn cmd_update_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::UpdateNative { stack } = invocation(args) else {
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
    let homelab_client::cli_args::Invocation::ReleaseUpdateNative { stack } = invocation(args)
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
    let homelab_client::cli_args::Invocation::RollbackNative { stack, unit } = invocation(args)
    else {
        die("internal: rollback-native parsed as another verb")
    };
    rpc(host, token, Command::RollbackNative { stack, unit }).await;
}
