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
    }
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
