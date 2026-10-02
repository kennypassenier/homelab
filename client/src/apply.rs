//! `homelab apply` (ask-8, Kenny 2026-09-27): hold the whole stacks
//! directory against the host in one command.
//!
//! What the files declare is deployed; what left them is destroyed — but only
//! after the operator types each stack's name, never by the nightly round,
//! and always through the same destroy with the same safety gates. The plan
//! itself is pure so it can be tested without a host or a repository.

use std::collections::BTreeMap;

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

/// What `apply` does once the plan is on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// `--dry-run`: the plan is the whole answer.
    Preview,
    Deploy,
    /// Anything but a yes, including no answer at all.
    Decline,
}

/// fix-100 (apply-no-confirm-creates-drill, 2026-09-27): `apply` deployed
/// every changed stack the moment it had printed the plan, and since ask-8 a
/// deploy also removes what the files no longer carry. It now asks once.
/// `answer` is the line typed at the prompt, `None` when nothing could be
/// read; only a typed yes deploys.
pub fn decide(dry_run: bool, yes: bool, answer: Option<&str>) -> Decision {
    if dry_run {
        return Decision::Preview;
    }
    if yes {
        return Decision::Deploy;
    }
    match answer.map(|a| a.trim().to_ascii_lowercase()) {
        Some(a) if a == "y" || a == "yes" => Decision::Deploy,
        _ => Decision::Decline,
    }
}

/// fix-100: the per-file change a deploy makes, from the files the host last
/// applied: `+ path` new, `~ path` changed, `- path` removed by the deploy.
/// Sorted by path; empty when the files are the same.
pub fn file_changes(
    local: &[homelab_proto::FileBlob],
    applied: &[homelab_proto::FileBlob],
) -> Vec<String> {
    let mut out: Vec<(String, char)> = Vec::new();
    for f in local {
        match applied.iter().find(|a| a.path == f.path) {
            None => out.push((f.path.clone(), '+')),
            Some(a) if a.content != f.content || a.mode != f.mode => {
                out.push((f.path.clone(), '~'))
            }
            Some(_) => {}
        }
    }
    for a in applied {
        if !local.iter().any(|f| f.path == a.path) {
            out.push((a.path.clone(), '-'));
        }
    }
    out.sort();
    out.into_iter()
        .map(|(p, sign)| format!("{} {}", sign, p))
        .collect()
}

/// fix-159 (2026-09-29): the running native units a deploy of these files
/// restarts, said in the plan before it happens: "restarts <unit>: unit
/// changed" (its unit file, or a drop-in of it under `rootfs/`). The deploy
/// restarts a unit only when it runs and what it reads really changed on
/// the container; a unit the host never had starts instead, and is not
/// listed. An env file is never among the stack files (latch fills it, the
/// deploy restores it from the vault), so "env changed" is said by the
/// deploy's transcript alone.
pub fn native_restarts(
    local: &[homelab_proto::FileBlob],
    applied: &[homelab_proto::FileBlob],
) -> Vec<String> {
    let differs = |f: &homelab_proto::FileBlob| match applied.iter().find(|a| a.path == f.path) {
        None => true,
        Some(a) => a.content != f.content || a.mode != f.mode,
    };
    let unit_of = |path: &str| {
        let (dir, file) = path.split_once('/')?;
        (file.strip_suffix(".service") == Some(dir)).then(|| dir.to_string())
    };
    let mut written: Vec<String> = Vec::new();
    for f in local.iter().filter(|f| differs(f)) {
        if let Some(rest) = f.path.strip_prefix(homelab_core::manifest::ROOTFS_PREFIX) {
            written.push(format!("/{}", rest));
        } else if let Some(unit) = unit_of(&f.path) {
            // A unit file new to the stack starts; only a changed one restarts.
            if applied.iter().any(|a| a.path == f.path) {
                written.push(format!("/etc/systemd/system/{}.service", unit));
            }
        }
    }
    let mut out: Vec<String> = local
        .iter()
        .filter_map(|f| {
            let unit = unit_of(&f.path)?;
            let why = homelab_core::native::restart_reason(&unit, &f.content, &written)?;
            Some(format!("restarts {}: {}", unit, why))
        })
        .collect();
    out.sort();
    out
}

/// fix-192 (media-redeploys-without-changing, Kenny 2026-10-02): the names
/// whose digest differs between `local` and `applied`, each written
/// `"<prefix>: <key>"`. Shared by the file/env/secret-file comparisons below
/// — one pure rule, no per-kind special case.
fn digest_diff_names(
    local: &BTreeMap<String, String>,
    applied: &BTreeMap<String, String>,
    prefix: &str,
) -> Vec<String> {
    let keys: std::collections::BTreeSet<&String> = local.keys().chain(applied.keys()).collect();
    let mut out: Vec<String> = keys
        .iter()
        .filter(|k| local.get(k.as_str()) != applied.get(k.as_str()))
        .map(|k| format!("{prefix}: {k}"))
        .collect();
    out.sort();
    out
}

/// fix-192: why a stack in the apply plan's `deploy` list differs from what
/// the host applied, from each side's `ComponentDigests` alone — never from
/// file content, so this costs nothing the plan did not already fetch
/// (`GetState`). `applied` is `None` when the host has recorded no component
/// digests for this stack yet (a deploy from before this field existed, or
/// a stack the host has never applied): said plainly rather than guessing
/// which component moved. Pure, and the only place this sentence is built —
/// the dashboard's Apply page and `homelab apply`'s plan both call it so
/// they say the same thing.
pub fn redeploy_reason(
    local: &homelab_core::manifest::ComponentDigests,
    applied: Option<&homelab_core::manifest::ComponentDigests>,
) -> String {
    let Some(applied) = applied else {
        return "new, or applied by a host that recorded no component digests yet — comparing \
                 by the combined fingerprint only"
            .to_string();
    };
    let mut changed: Vec<String> = Vec::new();
    for path in local.files.keys() {
        if !applied.files.contains_key(path) {
            changed.push(format!("+ {path}"));
        } else if local.files.get(path) != applied.files.get(path) {
            changed.push(format!("~ {path}"));
        }
    }
    for path in applied.files.keys() {
        if !local.files.contains_key(path) {
            changed.push(format!("- {path}"));
        }
    }
    changed.sort();
    changed.extend(digest_diff_names(&local.env, &applied.env, "env"));
    changed.extend(digest_diff_names(
        &local.secret_files,
        &applied.secret_files,
        "secret",
    ));
    if !changed.is_empty() {
        return changed.join(", ");
    }
    if local.manifest != applied.manifest {
        let old = if applied.built_by.is_empty() {
            "an earlier version".to_string()
        } else {
            applied.built_by.clone()
        };
        let new = if local.built_by.is_empty() {
            "this version".to_string()
        } else {
            local.built_by.clone()
        };
        return format!(
            "no file, env or secret changed — only the manifest homelab derives from them \
             (homelab {old} → {new})"
        );
    }
    "no difference found between the recorded digests".to_string()
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
