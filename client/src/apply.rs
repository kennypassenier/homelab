//! `homelab apply` (ask-8, Kenny 2026-09-27): hold the whole stacks
//! directory against the host in one command.
//!
//! What the files declare is deployed; what left them is destroyed — but only
//! after the operator types each stack's name, never by the nightly round,
//! and always through the same destroy with the same safety gates. The plan
//! itself is pure so it can be tested without a host or a repository.

/// What `apply` will do, by stack name. Each list is sorted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyPlan {
    /// Local stacks whose intent hash differs from what the host last
    /// applied, or that the host has never applied.
    pub deploy: Vec<String>,
    /// Local stacks the host already runs exactly as the files say.
    pub unchanged: Vec<String>,
    /// Stacks in host state whose directory is gone from the repository.
    /// Offered for destruction; each needs its name typed.
    pub destroy: Vec<String>,
}

/// Build the plan.
///
/// * `local`: every deployable stack directory (one with `lxc-compose.yml`)
///   with the intent hash of its files;
/// * `local_dirs`: the name of EVERY directory under the stacks directory,
///   deployable or not — an adopted native stack with only a `service.yml`
///   still has its directory, and is not "gone";
/// * `host`: every stack in host state with the hash it last applied (empty
///   when it never completed a deploy, which can never count as equal).
pub fn plan(
    local: &[(String, String)],
    local_dirs: &[String],
    host: &[(String, String)],
) -> ApplyPlan {
    let mut out = ApplyPlan::default();
    for (name, hash) in local {
        let applied = host.iter().find(|(n, _)| n == name).map(|(_, h)| h);
        match applied {
            Some(h) if !h.is_empty() && h == hash => out.unchanged.push(name.clone()),
            _ => out.deploy.push(name.clone()),
        }
    }
    for (name, _) in host {
        if !local_dirs.iter().any(|d| d == name) {
            out.destroy.push(name.clone());
        }
    }
    out.deploy.sort();
    out.unchanged.sort();
    out.destroy.sort();
    out.destroy.dedup();
    out
}

/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): the exit
/// code of `homelab apply --plan`, which prints the plan and changes
/// nothing: 0 when the host runs exactly what the files say, 2 when a
/// deploy or a destruction is pending. `apply` itself could only act.
pub fn plan_exit_code(plan: &ApplyPlan) -> i32 {
    if plan.deploy.is_empty() && plan.destroy.is_empty() {
        0
    } else {
        2
    }
}
