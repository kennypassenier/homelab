//! Helpers shared by the deploy and apply verbs.

use super::*;

/// Read one line the operator typed after `prompt` (typed-name gates).
pub(crate) fn read_typed(prompt: &str) -> String {
    use std::io::Write as _;
    eprint!("{}{}{}", C_RED, prompt, C_RESET);
    std::io::stderr().flush().ok();
    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed).ok();
    typed.trim().to_string()
}

/// Ship one stack: its native binaries one per message (T85), then the
/// deploy itself. `true` when the host reported the deploy done and ok.
/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): what each
/// stack directory says, for the host to compare with what it applied. A
/// directory that does not read is said out loud and left out; the rest of
/// the check still runs.
pub(crate) fn stack_digests(
    stack_files: &[(String, u16)],
) -> Vec<homelab_core::ops::fleetcheck::StackDigest> {
    let mut out = Vec::new();
    for (dir, _) in stack_files {
        match spec::stack_digest(Path::new(dir)) {
            Ok(d) => out.push(d),
            Err(e) => eprintln!(
                "{}  {} not compared with the host: {}{}",
                C_YELLOW, dir, e, C_RESET
            ),
        }
    }
    out
}

/// fix-110: `config/host.toml` as this working copy reads it, for the fleet
/// check to compare with the host's own running settings. `None` when the
/// repository has no such file (an older checkout, or one that has not
/// taken the coordinator's reconciliation yet) — `homelab check` then skips
/// the comparison, same as an older client talking to a newer host.
pub(crate) fn declared_host_config() -> Option<std::collections::BTreeMap<String, serde_json::Value>>
{
    let path = in_repo("config/host.toml");
    let raw = std::fs::read_to_string(&path).ok()?;
    let table: toml::Table = toml::from_str(&raw)
        .inspect_err(|e| {
            eprintln!(
                "{}  {} does not read as TOML: {} — the host-settings comparison is skipped{}",
                C_YELLOW,
                path.display(),
                e,
                C_RESET
            )
        })
        .ok()?;
    let mut out = std::collections::BTreeMap::new();
    for (key, value) in &table {
        // A secret, or a host-held key (`tokens`, fix-170), must never have
        // been in this file; dropped defensively rather than sent, in case
        // one was pasted in by hand.
        if homelab_core::hostconfig::is_secret(key) || homelab_core::hostconfig::is_host_held(key) {
            continue;
        }
        if let Ok(v) = serde_json::to_value(value) {
            out.insert(key.clone(), v);
        }
    }
    Some(out)
}

/// arch-deploy-guard: may this tree deploy `stack` over what the host runs?
/// The decision is `homelab_core::ops::deployguard::decide`; this only asks
/// git whether the host's commit is behind HEAD.
pub(crate) fn deploy_guard(
    dir: &Path,
    stack: &str,
    fleet: &homelab_proto::FleetState,
    force: bool,
) -> Result<(), String> {
    use homelab_core::ops::deployguard::{Ancestry, applied_commit, decide};
    let applied = fleet
        .stacks
        .iter()
        .find(|s| s.name == stack)
        .and_then(|s| s.applied_source.clone());
    let ancestry = match applied.as_deref().and_then(applied_commit) {
        None => Ancestry::Contained,
        Some(commit) => {
            let git = |args: &[&str]| {
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|st| st.success())
                    .unwrap_or(false)
            };
            let object = format!("{}^{{commit}}", commit);
            if !git(&["cat-file", "-e", &object]) {
                Ancestry::Unknown
            } else if git(&["merge-base", "--is-ancestor", commit, "HEAD"]) {
                Ancestry::Contained
            } else {
                Ancestry::Diverged
            }
        }
    };
    decide(stack, applied.as_deref(), ancestry, force)
}

/// T85: each native binary goes over the link on its own, then the deploy
/// follows with the map emptied — the host fills it back in from what was
/// staged. Three binaries in one message measured 94.7 MiB against a
/// 64 MiB frame (F303); one at a time never meets that ceiling, however
/// many services a stack grows. False when a staging failed.
async fn stage_native_binaries(
    host: &str,
    token: &str,
    spec: &mut homelab_proto::DeploySpec,
) -> bool {
    let staged: Vec<(String, String)> = spec
        .native_binaries
        .iter()
        .map(|(u, b)| (u.clone(), b.clone()))
        .collect();
    for (unit, b64) in staged {
        println!(
            "{}▶ staging {} ({} KiB of base64){}",
            C_DIM,
            unit,
            b64.len() / 1024,
            C_RESET
        );
        let stage = Command::StageNativeBinary {
            stack: spec.manifest.stack_name.clone(),
            unit: unit.clone(),
            binary_b64: b64,
        };
        if !rpc_with(host, token, stage).await {
            eprintln!(
                "{}✗ staging the binary of {} failed — deploy not started, nothing changed{}",
                C_RED, unit, C_RESET
            );
            return false;
        }
        spec.native_binaries.insert(unit, String::new());
    }
    true
}

pub(crate) async fn deploy_spec(
    host: &str,
    token: &str,
    mut spec: homelab_proto::DeploySpec,
) -> bool {
    println!(
        "{}▶ deploy {} :: vmid {} :: {} file(s), {} env(s){}",
        C_CYAN,
        spec.manifest.stack_name,
        spec.manifest.vmid,
        spec.files.len(),
        spec.env.len(),
        C_RESET
    );
    // fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci):
    // `deploy` and `apply` send the working tree; say so when it is not a
    // commit. A warning, not a refusal (Kenny's batch, 2026-09-27).
    if let Some(w) = spec
        .source
        .as_ref()
        .and_then(|s| spec::uncommitted_warning(&spec.manifest.stack_name, s))
    {
        eprintln!("{}{}{}", C_YELLOW, w, C_RESET);
    }
    if !stage_native_binaries(host, token, &mut spec).await {
        return false;
    }
    rpc_with(host, token, Command::DeployStack(Box::new(spec))).await
}
