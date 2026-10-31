//! What the apply plan shows: local stacks, the host's side and the printed plan.

use super::*;
use homelab_proto::{DeploySpec, FleetState};
use std::collections::BTreeMap;

/// Every stack in the files, built and validated, with the host-side facts
/// the plan is held against.
pub(crate) struct Local {
    pub(crate) local: Vec<(String, String)>,
    pub(crate) specs: BTreeMap<String, DeploySpec>,
    pub(crate) ephemeral: Vec<String>,
}

/// Every stack is built and validated before anything is sent: a
/// stack file that does not build stops the whole apply rather
/// than leaving the fleet half-applied.
pub(crate) fn build_local(base_path: &Path) -> Local {
    let mut local: Vec<(String, String)> = Vec::new();
    let mut specs: BTreeMap<String, DeploySpec> = BTreeMap::new();
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
    Local {
        local,
        specs,
        ephemeral,
    }
}

pub(crate) fn local_dirs(base_path: &Path) -> Vec<String> {
    std::fs::read_dir(base_path)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) async fn read_fleet(host: &str, token: &str) -> FleetState {
    let (ok, fleet) = rpc_collect(host, token, Command::GetState).await;
    match fleet {
        Some(f) if ok => f,
        _ => die("could not read the host's state — nothing applied"),
    }
}

/// The host's side of the comparison.
pub(crate) struct HostSide {
    pub(crate) pairs: Vec<(String, String)>,
    /// fix-192: the host's per-component digests, keyed by name, for the
    /// reason text — an empty manifest digest is the same "nothing recorded"
    /// case as no record at all.
    pub(crate) digests: BTreeMap<String, homelab_core::manifest::ComponentDigests>,
}

pub(crate) fn host_side(fleet: &FleetState) -> HostSide {
    HostSide {
        pairs: fleet
            .stacks
            .iter()
            .map(|s| (s.name.clone(), s.applied_hash.clone()))
            .collect(),
        digests: fleet
            .stacks
            .iter()
            .map(|s| (s.name.clone(), s.component_digests.clone()))
            .collect(),
    }
}

pub(crate) fn print_plan_summary(plan: &homelab_client::apply::ApplyPlan, ephemeral: &[String]) {
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
    for n in ephemeral {
        println!(
            "{}  · {} — ephemeral: deployed by name only, never by apply{}",
            C_DIM, n, C_RESET
        );
    }
}

async fn read_applied(host: &str, token: &str, n: &str) -> Vec<homelab_proto::FileBlob> {
    rpc_reply(
        host,
        token,
        Command::GetApplied {
            stack: n.to_string(),
        },
    )
    .await
    .filter(|r| r.ok)
    .and_then(|r| serde_json::from_str(&r.message).ok())
    .unwrap_or_else(|| {
        die(&format!(
            "could not read what the host applied for {} — nothing applied",
            n
        ))
    })
}

/// fix-100: what the deploy of `n` changes, file by file, removals
/// included — the host's applied files against the local ones.
pub(crate) async fn print_deploy_detail(
    host: &str,
    token: &str,
    n: &str,
    sp: &DeploySpec,
    side: &HostSide,
) {
    if !side.pairs.iter().any(|(h, _)| h == n) {
        println!(
            "{}      new: creates CT {}{}",
            C_DIM, sp.manifest.vmid, C_RESET
        );
        return;
    }
    let applied = read_applied(host, token, n).await;
    let changes = homelab_client::apply::file_changes(&sp.files, &applied);
    if changes.is_empty() {
        print_redeploy_reason(n, sp, side);
    }
    for c in changes {
        println!("      {}", c);
    }
    // fix-159: which running native units the deploy restarts.
    for r in homelab_client::apply::native_restarts(&sp.files, &applied) {
        println!("{}      ↻ {}{}", C_YELLOW, r, C_RESET);
    }
}

/// fix-192 (media-redeploys-without-changing, Kenny 2026-10-02): no file
/// differs, but the intent hash still moved — say which component did, from
/// the digests alone, instead of the old generic "settings differ" guess.
pub(crate) fn print_redeploy_reason(n: &str, sp: &DeploySpec, side: &HostSide) {
    let local_digest = homelab_core::manifest::component_digests(sp);
    let applied_digest = side.digests.get(n).filter(|d| !d.manifest.is_empty());
    println!(
        "{}      {}{}",
        C_DIM,
        homelab_client::apply::redeploy_reason(&local_digest, applied_digest),
        C_RESET
    );
}

pub(crate) async fn print_plan(
    host: &str,
    token: &str,
    base: &str,
    plan: &homelab_client::apply::ApplyPlan,
    local: &Local,
    side: &HostSide,
) {
    print_plan_summary(plan, &local.ephemeral);
    for n in &plan.deploy {
        println!("  ↑ {}", n);
        let Some(sp) = local.specs.get(n) else {
            continue;
        };
        print_deploy_detail(host, token, n, sp, side).await;
    }
    for n in &plan.destroy {
        println!(
            "{}  ✗ {} — in host state, no {}/{}/{}",
            C_YELLOW, n, base, n, C_RESET
        );
    }
}
