//! What Proxmox is enforcing for each stack's firewall right now, and the
//! tile-watch inputs both firewall judgements share.

use serde::{Deserialize, Serialize};

use crate::firewall::FleetTileTargets;
use crate::manifest::FirewallSpec;
use crate::manifest::StackManifest;
use crate::state::HostState;

/// fix-88: what pve holds for one recorded stack's firewall. `content` None =
/// no file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FirewallFact {
    pub stack: String,
    pub vmid: u16,
    pub content: Option<String>,
}

/// fix-207: one stack's firewall as Proxmox is actually enforcing it right
/// now, independent of what the repository declares — what the dashboard's
/// topology and firewall page need to answer Kenny's "why does the
/// topology not show that 5 of 11 are on".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FirewallLiveStatus {
    /// Whether the live `/etc/pve/firewall/<vmid>.fw` has `enable: 1` —
    /// Proxmox is applying this stack's rules right now, whatever the
    /// repository's working copy currently declares.
    pub enforced: bool,
    /// Whether the live file matches what the manifest declares (the same
    /// comparison [`evaluate_firewalls`] makes to report drift). `false`
    /// means the repository and pve disagree, in either direction.
    pub matches_repo: bool,
}

/// The inputs that turn a stack's declared firewall into the file the deploy
/// writes: the tile-watch rules derived across the fleet.
///
/// tile-watch: the fleet check must hold pve to the same declaration the
/// deploy writes, derived rule included, or every rollout with a tile watch
/// would report drift. Computed once for every stack, so the watcher's own
/// file is held against declared + derived IN *and* OUT.
pub(super) struct TileInputs<'a> {
    source: Option<&'a str>,
    fleet: FleetTileTargets,
    trusted: &'a [String],
}

impl<'a> TileInputs<'a> {
    pub(super) fn new(state: &HostState, source: Option<&'a str>, trusted: &'a [String]) -> Self {
        Self {
            source,
            fleet: crate::ops::tiles::fleet_tile_watch_targets(state),
            trusted,
        }
    }

    /// The rendered file this stack's declaration asks pve to hold.
    pub(super) fn wanted(
        &self,
        stack: &str,
        declared: &FirewallSpec,
        manifest: Option<&StackManifest>,
    ) -> String {
        let effective = manifest
            .map(|m| {
                crate::firewall::with_tile_watch(
                    declared,
                    &m.network.ip,
                    &m.tiles,
                    self.source.unwrap_or(""),
                    &self.fleet,
                    self.trusted,
                )
            })
            .unwrap_or_else(|| declared.clone());
        crate::firewall::render(stack, &effective)
    }
}

/// fix-207: the live status for every stack `files` was read for, keyed by
/// stack and vmid, pure given the facts and the state the deploy already
/// has. Reuses the exact comparison [`evaluate_firewalls`] reports as
/// Drift, so the two can never disagree about what "matches" means.
pub fn firewall_live_status(
    state: &HostState,
    files: &[FirewallFact],
    tile_watch_source: Option<&str>,
    trusted: &[String],
) -> std::collections::BTreeMap<String, FirewallLiveStatus> {
    let inputs = TileInputs::new(state, tile_watch_source, trusted);
    files
        .iter()
        .map(|f| {
            let manifest = state.stacks.get(&f.stack).and_then(|s| s.manifest.as_ref());
            let decl = manifest.and_then(|m| m.firewall.as_ref());
            let content = f.content.as_deref();
            let enforced = content.is_some_and(|c| c.lines().any(|l| l.trim() == "enable: 1"));
            let matches_repo = matches_repo(&inputs, &f.stack, manifest, decl, content);
            (
                f.stack.clone(),
                FirewallLiveStatus {
                    enforced,
                    matches_repo,
                },
            )
        })
        .collect()
}

/// Whether pve's file agrees with the declaration, in either direction.
fn matches_repo(
    inputs: &TileInputs,
    stack: &str,
    manifest: Option<&StackManifest>,
    decl: Option<&FirewallSpec>,
    content: Option<&str>,
) -> bool {
    match (decl, content) {
        (Some(d), content) if d.enabled => {
            let want = inputs.wanted(stack, d, manifest);
            content
                .map(|have| {
                    let (extra, missing) = crate::firewall::line_changes(&want, have);
                    extra.is_empty() && missing.is_empty()
                })
                .unwrap_or(false)
        }
        // Nothing declared enabled, and pve shows nothing either:
        // the two agree there is no firewall in force here.
        (_, None) => true,
        // Pve has a file but nothing here declares it enabled: an
        // undeclared or disabled-in-the-repo ruleset.
        (_, Some(_)) => false,
    }
}
