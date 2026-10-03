//! dash-apply (Kenny, 2026-09-28: "Yes, plan first"): `homelab apply` in the
//! dashboard. The plan per stack first, one confirmation, and a stack whose
//! directory is gone destroyed only after its name is typed. The plan is
//! the CLI's own (`homelab_client::apply::plan`); this adds what the page
//! needs around it. Pure.

use serde::Serialize;

/// The plan as the Apply page and the apply form show it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyView {
    /// Stacks whose files differ from what the host applied, in the order
    /// they are deployed.
    pub deploy: Vec<String>,
    /// Of those, the ones the host has never applied (a new container).
    pub new: Vec<String>,
    /// Stacks the host runs exactly as the files say.
    pub unchanged: Vec<String>,
    /// Stacks in host state whose directory is gone: destroyed only when
    /// their name is typed.
    pub destroy: Vec<String>,
    /// Deployed by name only, never by apply.
    pub ephemeral: Vec<String>,
    /// Stacks whose files do not build, with why: apply refuses while any
    /// is listed, as `homelab apply` does ("nothing applied").
    pub broken: Vec<(String, String)>,
    /// fix-192 (media-redeploys-without-changing, Kenny 2026-10-02): for
    /// each stack in `deploy`, WHY — which files/env/secrets changed, or
    /// that only the derived manifest did. Filled by `deploy_reasons`; empty
    /// until that runs (the pure `plan` above never touches the host's
    /// per-component digests).
    pub reasons: std::collections::BTreeMap<String, String>,
}

impl ApplyView {
    /// Whether apply would do anything.
    pub fn pending(&self) -> bool {
        !self.deploy.is_empty() || !self.destroy.is_empty()
    }
}

/// The plan: `hashes` is every declared stack's intent hash (or why it did
/// not build), `dirs` every directory under stacks/, `host` every stack the
/// host records with the hash it last applied.
pub fn plan(
    hashes: &[(String, Result<String, String>)],
    dirs: &[String],
    host: &[(String, String)],
    ephemeral: &[String],
) -> ApplyView {
    let local: Vec<(String, String)> = hashes
        .iter()
        .filter_map(|(n, h)| h.as_ref().ok().map(|h| (n.clone(), h.clone())))
        .collect();
    let p = homelab_client::apply::plan(&local, dirs, host);
    let new = p
        .deploy
        .iter()
        .filter(|n| !host.iter().any(|(h, _)| h == *n))
        .cloned()
        .collect();
    ApplyView {
        deploy: p.deploy,
        new,
        unchanged: p.unchanged,
        destroy: p.destroy,
        ephemeral: ephemeral.to_vec(),
        broken: hashes
            .iter()
            .filter_map(|(n, h)| h.as_ref().err().map(|e| (n.clone(), e.clone())))
            .collect(),
        // fix-192: filled by the caller (`deploy_reasons`), which needs the
        // component digests this pure function never takes.
        reasons: Default::default(),
    }
}

/// fix-192: the reason text for every stack in `deploy`, from each side's
/// `ComponentDigests` (`client::apply::redeploy_reason`). A stack with no
/// local digest recorded (should not happen — `deploy` only ever lists a
/// stack `hashes` built) is left out rather than guessed at.
pub fn deploy_reasons(
    deploy: &[String],
    local: &std::collections::BTreeMap<String, homelab_core::manifest::ComponentDigests>,
    host: &[(String, homelab_core::manifest::ComponentDigests)],
) -> std::collections::BTreeMap<String, String> {
    deploy
        .iter()
        .filter_map(|name| {
            let l = local.get(name)?;
            let applied = host.iter().find(|(n, _)| n == name).map(|(_, d)| d);
            // A host record whose manifest digest is empty is the same
            // "nothing recorded" case as no record at all — never applied,
            // or applied by a host from before this field existed.
            let applied = applied.filter(|d| !d.manifest.is_empty());
            Some((
                name.clone(),
                homelab_client::apply::redeploy_reason(l, applied),
            ))
        })
        .collect()
}

/// What one Apply press runs (redesign-flows-5): the deploys and the
/// destroys, chosen from the plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chosen {
    pub deploy: Vec<String>,
    pub destroy: Vec<String>,
}

/// redesign-flows-5: the subset one Apply press runs.
///
/// - `leave_out`: stacks of the plan's deploy list left out of this press,
///   and the stacks that cannot be planned, each named, so they are left
///   alone instead of refusing the whole run. A stack that cannot be planned
///   and is not named still refuses everything (`homelab apply`'s rule).
/// - `typed`, `ids`, `ack`: a destroy is its own step after the deploys
///   (coordinator, 2026-10-03): only in a press that deploys nothing, only
///   with its own confirmation ticked, and only for the CT number the host
///   records under that name, so a plan read before a stack was re-created
///   never destroys the new one. The backup and restore check the host takes
///   first is never skipped from here (`validate` refuses `skip_backup`).
pub fn choose(
    view: &ApplyView,
    leave_out: &[String],
    typed: &[String],
    ids: &[u16],
    ack: bool,
    vmid_of: impl Fn(&str) -> Option<u16>,
) -> Result<Chosen, String> {
    for n in leave_out {
        if !view.deploy.contains(n) && !view.broken.iter().any(|(b, _)| b == n) {
            return Err(format!(
                "{n} is neither a stack to deploy nor one that cannot be planned, so there is nothing to leave out; plan again"
            ));
        }
    }
    if let Some((name, why)) = view.broken.iter().find(|(b, _)| !leave_out.contains(b)) {
        return Err(format!("{name} does not build: {why} — nothing applied"));
    }
    let destroy = chosen_destroys(view, typed)?;
    let deploy: Vec<String> = view
        .deploy
        .iter()
        .filter(|s| !leave_out.contains(s))
        .cloned()
        .collect();
    if !destroy.is_empty() {
        if !deploy.is_empty() {
            return Err(
                "a destroy is its own step after the deploys: leave every deploy out of the press that destroys"
                    .into(),
            );
        }
        if !ack {
            return Err("a destroy runs only after its own confirmation is ticked".into());
        }
        if ids.len() != destroy.len() {
            return Err(format!(
                "each stack to destroy needs its CT number, in the same order ({} names, {} numbers)",
                destroy.len(),
                ids.len()
            ));
        }
        for (n, id) in destroy.iter().zip(ids) {
            match vmid_of(n) {
                Some(real) if real == *id => {}
                Some(real) => {
                    return Err(format!(
                        "{n} is CT {real} on the host, not CT {id}; plan again before destroying it"
                    ));
                }
                None => {
                    return Err(format!(
                        "the host records no CT number for {n}; plan again before destroying it"
                    ));
                }
            }
        }
    }
    if deploy.is_empty() && destroy.is_empty() {
        return Err("nothing to apply: every stack is left out and no destroy is armed".into());
    }
    Ok(Chosen { deploy, destroy })
}

/// The gone stacks the typed names choose. A typed name the plan does not
/// list as gone is refused (it would destroy a stack the files still
/// declare); a gone stack whose name was not typed is kept.
pub fn chosen_destroys(view: &ApplyView, typed: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for n in typed {
        if !view.destroy.contains(n) {
            return Err(format!(
                "{n} is not gone from the files, so apply does not destroy it"
            ));
        }
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    Ok(out)
}
