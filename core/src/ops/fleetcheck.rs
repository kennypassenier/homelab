//! Y4: hold the repository against reality and report every difference.
//!
//! Everything found by hand on 2026-08-30 had one thing in common: none of it
//! was a failure. kyu's stack record still described a container that had been
//! renamed weeks earlier, so its nightly run failed the hostname guard and the
//! stack quietly auto-disabled. A settings key in `host.toml` replaced the
//! compiled no-touch list rather than adding to it, so a code change had no
//! effect. Three stack files claimed vmids that live containers were using. A
//! Traefik route pointed at an empty container and another at a workstation.
//! Uptime Kuma watched exactly one target. Every one of those looked healthy,
//! and the only reason any of them surfaced is that somebody spent a day
//! looking.
//!
//! This module makes the looking a function. The comparison is pure so it can
//! be exercised without a fleet; gathering the facts is the shell's job.

use serde::{Deserialize, Serialize};

use crate::state::HostState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Something is not doing its job right now.
    Broken,
    /// Working, but drifting — it will bite on the next deploy or outage.
    Drift,
    /// Not a problem: something deliberately arranged, printed so it stays
    /// visible (Kenny, form Z3, 2026-09-02).
    ///
    /// The case it exists for: a stack whose data is declared reproducible
    /// has no backup, and a check that only knows Broken and Drift would
    /// either shout "never been backed up" at a decision, or say nothing at
    /// all — and silence makes a deliberate gap indistinguishable from a
    /// forgotten one. Neither is the truth; this is.
    Noted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    /// Which stack, container or file this is about.
    pub subject: String,
    /// What is wrong, in one sentence.
    pub what: String,
    /// What to do about it. Every finding carries one (standing rule 11).
    pub remedy: String,
}

/// What the shell reads off the machine so the comparison can stay pure.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    /// vmid → hostname, for every container that exists on the hypervisor.
    pub containers: Vec<(u16, String)>,
    /// checks-automate: each registered probe, read in its container.
    pub probe_readings: Vec<crate::ops::probes::ProbeReading>,
    /// Route file name → the address it forwards to, and whether anything
    /// answered there.
    pub routes: Vec<RouteFact>,
    /// Stack directories found in the repository, with the vmid they claim.
    pub stack_files: Vec<(String, u16)>,
    /// G3: what every managed container's resources look like right now.
    pub growth: Vec<GrowthFact>,
    /// Whether each managed stack's safety nets are actually attached.
    pub coverage: Vec<CoverageFact>,
    /// W3: what `pct config` says about boot policy and resources, per
    /// managed container.
    pub boot: Vec<BootFact>,
    /// F184: what the HOST itself has, so a per-container remedy cannot
    /// advise something the machine cannot give. `(total_mb, committed_mb,
    /// swap_used_mb, swap_total_mb)`, or None when it could not be read —
    /// which is not the same as a healthy host and is treated as unknown.
    pub host_memory: Option<(u32, u32, u32, u32)>,
    /// O1: backups made OUTSIDE this suite that it nevertheless watches —
    /// today the router's own nightly upload to Google Drive. Empty = none
    /// declared, which is what every fleet had before this existed.
    pub watched_backups: Vec<WatchedBackupFact>,
    /// R13: how full the pools are that the stacks keep their data on. Empty
    /// when no stack declares a data mount, which is what every fleet looked
    /// like before this existed.
    pub pools: Vec<PoolFact>,
    /// fix-24: log files on the declared data mounts above the size a
    /// rotated log never reaches. Empty when none, and when none was read.
    pub big_logs: Vec<BigLogFact>,
    /// fix-26: storage directories whose owner on disk is not the declared
    /// `host_owner_uid`. Only mismatches are kept.
    pub owners: Vec<OwnerFact>,
    /// fix-92: every file name in the gateway's routes directory, whatever
    /// its extension. Empty when none, and when it could not be read.
    pub route_files: Vec<String>,
    /// fix-88: each recorded stack's `/etc/pve/firewall/<vmid>.fw`.
    pub firewalls: Vec<FirewallFact>,
    /// fix-150: the patch state of every managed container that answered.
    pub patch: Vec<PatchFact>,
    /// fix-96: the configured `second_copy_dataset`, or None when no second
    /// copy is configured (and then nothing is said about one).
    pub second_copy_dataset: Option<String>,
    /// fix-142: what the client's stack files say, one digest per stack.
    /// Empty from a client older than this field, and in the nightly round,
    /// which has no repository; the repository comparison is then skipped.
    pub digests: Vec<StackDigest>,
    /// fix-142: the host's intent-history copy of each stack the client sent,
    /// stack → container-bound path → sha256. A stack with no copy is absent.
    pub intent_files:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// rule-20 (disk-audit, 2026-10-01): host-level capacity readings that
    /// are neither a managed container's own rootfs (`growth`) nor a
    /// stack's declared data pool (`pools`) — pve's own root filesystem,
    /// the local-lvm thin pool, pve's journald against its own cap, and
    /// every ZFS pool on the host. Empty when none were read.
    pub host_capacity: Vec<HostCapacityFact>,
    /// fix-110: `config/host.toml` as the client's (or the dashboard's)
    /// working copy reads it, non-secret keys only. `None` from an older
    /// client, the nightly round, or a repository that does not carry the
    /// file yet — the comparison is then skipped, same as `digests` being
    /// empty skips the stack-file comparison.
    pub declared_host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// fix-110: the same non-secret keys as the host's OWN host.toml sets
    /// them right now — gathered host-side, the way `GetHostConfig` reads
    /// them, so the comparison is file-to-file rather than against a
    /// resolved-with-defaults view nobody else sees. Empty when the host
    /// sets none of them (every key then uses its compiled default).
    pub live_host_config: std::collections::BTreeMap<String, serde_json::Value>,
    /// fix-142 (nightly hash comparison, Kenny's go 2026-10-01): every
    /// recorded stack's `/opt/<stack>/` hashed inside its container right
    /// now, stack → manifest path → sha256 — gathered host-side, nightly
    /// only (like `digests` from the repository side, this is empty on a
    /// `homelab check` the host did not run itself). Compared against
    /// `StackState::pushed_file_hashes` by `evaluate_container_drift`.
    pub container_file_hashes:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

/// fix-92 / fix-130: every name in the gateway's routes directory that no
/// stack's deploy recorded. Pure so `homelab doctor` (which wants the bare
/// names, not a `Finding`) and the fleet check (which wants one) can share a
/// single judgement.
pub fn unowned_route_files(state: &HostState, on_disk: &[String]) -> Vec<String> {
    let owned: std::collections::BTreeSet<&str> = state
        .stacks
        .values()
        .flat_map(|s| s.route_file.iter().chain(s.extra_route_files.iter()))
        .map(String::as_str)
        .collect();
    on_disk
        .iter()
        .filter(|f| !owned.contains(f.as_str()))
        .cloned()
        .collect()
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): a file in the
/// gateway's routes directory that no stack's deploy recorded.
///
/// Which hostnames the house publishes is decided by that directory, so a
/// file there that no stack declares is a hostname the repository cannot
/// account for — the four hand-written files fix-91 brought in were exactly
/// that. Judged against what deploys recorded (`route_file`,
/// `extra_route_files`) rather than against the stack files, because the
/// nightly round runs on the host, which has no working copy of them. Every
/// name counts, not only `*.yml`: a `.bak` Traefik ignores today is still a
/// file somebody put there and nobody owns. Drift, not Broken: nothing fails
/// because of it. Homelab never removes such a file itself (fix-41).
pub fn evaluate_route_owners(state: &HostState, on_disk: &[String]) -> Vec<Finding> {
    unowned_route_files(state, on_disk)
        .into_iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: format!("gateway route file {}", f),
            what: "is in the gateway's routes directory but no stack declares it — the \
                   repository cannot say what it publishes"
                .into(),
            remedy: "declare it in the stack it belongs to (gateway_route, or extra_routes to \
                     keep its name) and deploy that stack, or delete it by hand on the gateway; \
                     homelab never removes a route file no deploy wrote"
                .into(),
        })
        .collect()
}

/// fix-150 (expert panel 2026-09-27, the patch-state half of
/// check-blind-to-repo-drift; Kenny 2026-09-28: "7 dagen"): how long a
/// container's updates may stand before `homelab check` says so.
pub const PATCH_THRESHOLD_S: u64 = 7 * 86_400;

/// fix-150: one managed container's patch state, as its probe reported it.
/// `upgradable` None = the probe did not answer (a stopped container, a
/// broken exec); then nothing is judged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatchFact {
    pub vmid: u16,
    pub hostname: String,
    /// Packages `apt` would upgrade right now.
    pub upgradable: Option<u32>,
    /// How long `/var/run/reboot-required` has existed; None = no reboot
    /// pending.
    pub reboot_required_age_s: Option<u64>,
    /// How long ago unattended-upgrades last ran to completion
    /// (`/var/lib/apt/periodic/upgrade-stamp`); None = never on this
    /// container.
    pub unattended_stamp_age_s: Option<u64>,
    /// How old the container is (`/etc/hostname`, written by Proxmox when it
    /// creates the container); None = unknown, judged as old.
    pub age_s: Option<u64>,
}

/// fix-150: updates that stand still. Two things count, both against
/// [`PATCH_THRESHOLD_S`]: a reboot that has been required for longer, and a
/// container whose daily unattended-upgrades run has not completed for
/// longer (or ever). Upgradable packages on their own are not a finding:
/// the nightly `homelab patch` takes them, and on 2026-09-27 21:15 CT 116
/// held 40 and CT 113 64 with everything working — the count is named in
/// the finding, never the reason for it. Drift, not Broken: nothing is down.
pub fn evaluate_patch_state(facts: &[PatchFact], threshold_s: u64) -> Vec<Finding> {
    let days = |s: u64| s / 86_400;
    let mut out = Vec::new();
    for f in facts {
        let Some(upgradable) = f.upgradable else {
            continue;
        };
        let subject = format!("{} ({})", f.vmid, f.hostname);
        if let Some(age) = f.reboot_required_age_s
            && age > threshold_s
        {
            out.push(Finding {
                    severity: Severity::Drift,
                    subject: subject.clone(),
                    what: format!(
                        "has needed a reboot for {} days (a kernel or libc update is installed                          but not running); {} package(s) upgradable",
                        days(age),
                        upgradable
                    ),
                    remedy: "reboot the container outside the backup hour: `pct reboot <vmid>` on                              pve, or restart it from the TUI"
                        .into(),
                });
        }
        match f.unattended_stamp_age_s {
            Some(age) if age > threshold_s => out.push(Finding {
                severity: Severity::Drift,
                subject: subject.clone(),
                what: format!(
                    "unattended-upgrades last completed {} days ago; {} package(s) upgradable",
                    days(age),
                    upgradable
                ),
                remedy: "inside the container: `systemctl status apt-daily-upgrade.timer` and                          `unattended-upgrade -d`; the golden template carries the working                          configuration"
                    .into(),
            }),
            // A container younger than the threshold has not had its week
            // yet (the first check after the 2026-09-28 rebuilds flagged
            // seven containers built that morning).
            None if f.age_s.is_some_and(|a| a <= threshold_s) => {}
            None => out.push(Finding {
                severity: Severity::Drift,
                subject,
                what: format!(
                    "unattended-upgrades has never completed a run here; {} package(s) upgradable",
                    upgradable
                ),
                remedy: "inside the container: `apt install unattended-upgrades` and                          `systemctl enable --now apt-daily-upgrade.timer`, as the golden                          template has"
                    .into(),
            }),
            Some(_) => {}
        }
    }
    out
}

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

/// fix-207: the live status for every stack `files` was read for, keyed by
/// stack and vmid, pure given the facts and the state the deploy already
/// has. Reuses the exact comparison [`evaluate_firewalls`] reports as
/// Drift, so the two can never disagree about what "matches" means.
pub fn firewall_live_status(
    state: &HostState,
    files: &[FirewallFact],
    tile_watch_source: Option<&str>,
) -> std::collections::BTreeMap<String, FirewallLiveStatus> {
    let fleet_targets = crate::ops::tiles::fleet_tile_watch_targets(state);
    files
        .iter()
        .map(|f| {
            let manifest = state.stacks.get(&f.stack).and_then(|s| s.manifest.as_ref());
            let decl = manifest.and_then(|m| m.firewall.as_ref());
            let content = f.content.as_deref();
            let enforced = content.is_some_and(|c| c.lines().any(|l| l.trim() == "enable: 1"));
            let matches_repo = match (decl, content) {
                (Some(d), content) if d.enabled => {
                    let effective = manifest
                        .map(|m| {
                            crate::firewall::with_tile_watch(
                                d,
                                &m.network.ip,
                                &m.tiles,
                                tile_watch_source.unwrap_or(""),
                                &fleet_targets,
                            )
                        })
                        .unwrap_or_else(|| d.clone());
                    let want = crate::firewall::render(&f.stack, &effective);
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
            };
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

/// fix-88: every recorded stack's firewall file on pve, held against the
/// declaration the stack's last deploy recorded.
///
/// Drift, not Broken: a container without its rules still serves, and the
/// point is to see a hand edit or a missing file before it matters. The one
/// state that is neither a fault nor silent is a declaration kept for the
/// rollout — noted in a single line, so the list of stacks still to switch
/// on stays visible without a finding per stack every night.
pub fn evaluate_firewalls(
    state: &HostState,
    files: &[FirewallFact],
    boot: &[BootFact],
    tile_watch_source: Option<&str>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut not_enabled: Vec<String> = Vec::new();
    // tile-watch-watcher-out: computed once for every stack below, so the
    // watcher's own file is held against declared + derived IN *and* OUT —
    // the same `with_tile_watch` call that renders every other stack's file
    // also renders the watcher's, and this is the one fleet-wide input it
    // needs to do that.
    let fleet_targets = crate::ops::tiles::fleet_tile_watch_targets(state);
    for f in files {
        let path = crate::firewall::fw_path(f.vmid);
        let subject = format!("{} ({})", path, f.stack);
        let manifest = state.stacks.get(&f.stack).and_then(|s| s.manifest.as_ref());
        let decl = manifest.and_then(|m| m.firewall.as_ref());
        match (decl, f.content.as_deref()) {
            (Some(d), content) if d.enabled => {
                // tile-watch: the fleet check must hold pve to the same
                // declaration the deploy writes, derived rule included, or
                // every rollout with a tile watch would report drift.
                let effective = manifest
                    .map(|m| {
                        crate::firewall::with_tile_watch(
                            d,
                            &m.network.ip,
                            &m.tiles,
                            tile_watch_source.unwrap_or(""),
                            &fleet_targets,
                        )
                    })
                    .unwrap_or_else(|| d.clone());
                let want = crate::firewall::render(&f.stack, &effective);
                match content {
                    None => out.push(Finding {
                        severity: Severity::Drift,
                        subject: subject.clone(),
                        what: "the stack declares a firewall, but the file is absent — the \
                               container runs without its rules"
                            .into(),
                        remedy: format!("deploy stacks/{}; it writes the file", f.stack),
                    }),
                    Some(have) if have != want => {
                        let (extra, missing) = crate::firewall::line_changes(&want, have);
                        let show = |v: &[String]| {
                            let mut s = v.iter().take(3).cloned().collect::<Vec<_>>().join(" | ");
                            if v.len() > 3 {
                                s.push_str(&format!(" | … {} more", v.len() - 3));
                            }
                            if s.is_empty() { "nothing".into() } else { s }
                        };
                        out.push(Finding {
                            severity: Severity::Drift,
                            subject: subject.clone(),
                            what: format!(
                                "differs from its declaration — on pve only: {}; declared only: {}",
                                show(&extra),
                                show(&missing)
                            ),
                            remedy: format!(
                                "a hand edit on pve: put the change in the `firewall:` block of \
                                 stacks/{}/lxc-compose.yml and deploy, or deploy as it is to put \
                                 the declaration back",
                                f.stack
                            ),
                        });
                    }
                    Some(_) => {}
                }
                if boot
                    .iter()
                    .any(|b| b.vmid == f.vmid && b.live.nic_firewall == Some(false))
                {
                    out.push(Finding {
                        severity: Severity::Drift,
                        subject: format!("{} ({})", f.vmid, f.stack),
                        what: "net0 has firewall=0, so Proxmox applies none of the declared rules"
                            .into(),
                        remedy: format!("deploy stacks/{}; it switches the NIC flag on", f.stack),
                    });
                }
            }
            (_, Some(_)) => out.push(Finding {
                severity: Severity::Drift,
                subject,
                what: "exists on pve, but the stack file does not enable a firewall — a \
                       ruleset the repository does not know"
                    .into(),
                remedy: format!(
                    "declare it in the `firewall:` block of stacks/{}/lxc-compose.yml with \
                     `enabled: true` and deploy, so the deploy owns the file; or remove the file \
                     on pve if it should not exist",
                    f.stack
                ),
            }),
            (Some(_), None) => not_enabled.push(f.stack.clone()),
            (None, None) => {}
        }
    }
    if !not_enabled.is_empty() {
        not_enabled.sort();
        out.push(Finding {
            severity: Severity::Noted,
            subject: "firewall".into(),
            what: format!("declared, not enabled yet: {}", not_enabled.join(", ")),
            remedy: "switch one stack on at a time: `enabled: true` in its `firewall:` block, \
                     deploy it, and watch its checks and monitors"
                .into(),
        });
    }
    out
}

/// fix-142: one stack directory as the client reads it, for comparison with
/// what the host last applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackDigest {
    /// The stack's name (its directory name).
    pub stack: String,
    /// The parsed `lxc-compose.yml`; None for a stack with only a
    /// `service.yml`. Sent whole rather than hashed so the host compares it
    /// through its own type, and a client one release older does not make
    /// every stack look changed. Used only when `component_digests` is empty
    /// on either side (fix-201).
    #[serde(default)]
    pub manifest: Option<crate::manifest::StackManifest>,
    /// Every file a deploy would send into the container → sha256 hex. Used
    /// only when `component_digests` is empty on either side (fix-201).
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
    /// fix-201 (today-reports-just-deployed-stacks-as-drifted): this stack
    /// directory's manifest and files digested the same way
    /// `StackState::component_digests` is — the same inputs `intent_hash`
    /// uses, from the fully derived spec (tile `probe` fields included, no
    /// latch: F291). Preferred over `manifest`/`files` above whenever both
    /// sides have it, because it compares the raw spec the host actually
    /// applied against the raw spec this directory would build, rather than
    /// a parsed-manifest snapshot built a different way on one side and a
    /// separate host-side git mirror of the repository on the other — a
    /// mirror that can simply not have caught up yet, and that was never
    /// asked for `lxc-compose.yml` in the first place. Empty (`has_digests()`
    /// false) for an older client.
    #[serde(default)]
    pub component_digests: crate::manifest::ComponentDigests,
}

/// fix-142: the repository against what the host applied. Story:
/// `docs/deployment/REGISTER.md`.
///
/// Each difference is a Drift finding naming what differs: the manifest
/// (compared through the host's own type), and every container-bound file
/// changed, new or gone against the host's intent copy.
///
/// Only what was asked: no digests (an older client, or the nightly round,
/// which has no repository) means no comparison; a stack whose intent copy
/// could not be read is compared on its manifest alone.
///
/// fix-201: when both sides carry `component_digests` (recorded at the last
/// deploy from the exact spec it applied — see `manifest::component_digests`
/// and `StackState::component_digests`), the manifest and files comparison
/// below uses those instead of `manifest`/`intent_files` — raw spec vs raw
/// spec, the same inputs `intent_hash` used to decide `apply` already saw
/// this stack as unchanged. That is what fixed the finding: `apply --plan`
/// and `today` used to read "what changed" from two different places and
/// could disagree. The old comparison remains the fallback for a host or
/// client that recorded no component digests yet.
pub fn evaluate_repo_drift(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    for d in &live.digests {
        let Some(st) = state.stacks.get(&d.stack) else {
            // Never deployed. A vmid that is a live container nobody manages
            // is already reported by the stack-file check above.
            let vmid = d.manifest.as_ref().map(|m| m.vmid);
            let taken = vmid.is_some_and(|v| live.containers.iter().any(|(c, _)| *c == v));
            let on_demand = d.manifest.as_ref().is_some_and(|m| m.on_demand);
            if d.manifest.is_some() && !taken && !on_demand {
                out.push(Finding {
                    severity: Severity::Drift,
                    subject: d.stack.clone(),
                    what: format!(
                        "is declared in stacks/{} (vmid {}) but was never deployed",
                        d.stack,
                        vmid.unwrap_or_default()
                    ),
                    remedy: format!(
                        "`homelab apply` would create it; deploy it with `homelab deploy \
                         stacks/{0}`, or remove stacks/{0}/ if it is not meant to exist",
                        d.stack
                    ),
                });
            }
            continue;
        };
        let list = |names: Vec<&String>| -> String {
            names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts: Vec<String> = Vec::new();
        if d.component_digests.has_digests() && st.component_digests.has_digests() {
            // fix-201: raw spec vs raw spec — the same inputs `intent_hash`
            // compared when this deploy was judged unchanged, never the
            // separate host-side intent-history mirror (`live.intent_files`),
            // which is not asked for in this branch at all.
            if d.component_digests.manifest != st.component_digests.manifest {
                parts.push("lxc-compose.yml differs".into());
            }
            let changed: Vec<&String> = d
                .component_digests
                .files
                .iter()
                .filter(|(p, h)| st.component_digests.files.get(*p).is_some_and(|c| c != *h))
                .map(|(p, _)| p)
                .collect();
            let added: Vec<&String> = d
                .component_digests
                .files
                .keys()
                .filter(|p| !st.component_digests.files.contains_key(*p))
                .collect();
            let gone: Vec<&String> = st
                .component_digests
                .files
                .keys()
                .filter(|p| !d.component_digests.files.contains_key(*p))
                .collect();
            if !changed.is_empty() {
                parts.push(format!("changed: {}", list(changed)));
            }
            if !added.is_empty() {
                parts.push(format!("new: {}", list(added)));
            }
            if !gone.is_empty() {
                parts.push(format!("gone from the files: {}", list(gone)));
            }
        } else {
            // Fallback: no component digests on one side (an older client or
            // host, or a stack never (re)applied since fix-192) — compare the
            // parsed manifest and the files against the host's git-mirror
            // intent copy, same as before fix-201.
            if let (Some(local), Some(applied)) = (d.manifest.as_ref(), st.manifest.as_ref())
                && serde_json::to_value(local).ok() != serde_json::to_value(applied).ok()
            {
                parts.push("lxc-compose.yml differs".into());
            }
            if let Some(copy) = live.intent_files.get(&d.stack) {
                let changed: Vec<&String> = d
                    .files
                    .iter()
                    .filter(|(p, h)| copy.get(*p).is_some_and(|c| c != *h))
                    .map(|(p, _)| p)
                    .collect();
                let added: Vec<&String> =
                    d.files.keys().filter(|p| !copy.contains_key(*p)).collect();
                let gone: Vec<&String> =
                    copy.keys().filter(|p| !d.files.contains_key(*p)).collect();
                if !changed.is_empty() {
                    parts.push(format!("changed: {}", list(changed)));
                }
                if !added.is_empty() {
                    parts.push(format!("new: {}", list(added)));
                }
                if !gone.is_empty() {
                    parts.push(format!("gone from the files: {}", list(gone)));
                }
            }
        }
        if parts.is_empty() {
            continue;
        }
        out.push(Finding {
            severity: Severity::Drift,
            subject: d.stack.clone(),
            what: format!(
                "{} {} — {}",
                REPO_DRIFT_WHAT_PREFIX,
                crate::state::ymd(st.applied_at),
                parts.join("; ")
            ),
            remedy: format!(
                "`homelab deploy stacks/{}` (or `homelab apply`) to apply them, or put the \
                 files back as they were",
                d.stack
            ),
        });
    }
    out
}

/// fix-219 (drift-finding-names-only-filenames, 2026-10-02): the exact words
/// `evaluate_repo_drift` opens a finding with. One place names them, so the
/// client and the dashboard — which only see this finding's rendered text or
/// its `what` field, never a structured "this is a repo-drift finding" flag
/// (digests are hashes, not content, by design: see this module's doc
/// comment) — can find the same findings `evaluate_repo_drift` produced
/// without a second copy of the sentence to keep in step.
pub const REPO_DRIFT_WHAT_PREFIX: &str = "the files differ from what the host applied on";

/// fix-219: is `what` (a `Finding::what`, or `"<subject>: <what>"` as
/// `ops::today::assemble` folds it) a repo-drift finding from
/// `evaluate_repo_drift` — the one kind of finding that names a bare
/// filename where Kenny wants the actual change.
pub fn is_repo_file_drift_text(what: &str) -> bool {
    what.contains(REPO_DRIFT_WHAT_PREFIX)
}

/// fix-219: every stack named by a repo-drift finding in `rendered` — the
/// text `render()` (for `homelab check`) or `ops::today::render` (for
/// `homelab today`) produces. Both shapes put the stack directly beside the
/// phrase (`"[drift] <stack> — the files differ …"` and `"[attention]
/// <stack>: the files differ … (check)"`), so one scan handles either: find
/// the phrase, trim the separator right before it (" —" or ":"), then take
/// the text after the last `"] "` on what remains. Sorted and deduplicated so
/// a caller fetches each drifted stack's applied files once.
pub fn repo_drift_stacks_in_rendered_text(rendered: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in rendered.lines() {
        let Some(pos) = line.find(REPO_DRIFT_WHAT_PREFIX) else {
            continue;
        };
        let before = line[..pos].trim_end();
        let before = before
            .strip_suffix(" —")
            .or_else(|| before.strip_suffix(':'))
            .unwrap_or(before);
        let stack = before.rsplit("] ").next().unwrap_or(before).trim();
        if !stack.is_empty() {
            out.push(stack.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// fix-219: is `path` one this project never puts in a repository/applied
/// comparison in the first place (`.env`, `.env.*`) — belt and braces on top
/// of `evaluate_repo_drift`'s own inputs, which never carry a secret path at
/// all (`fetch_secrets: false`, see this module's doc comment above). A
/// caller that builds a per-file diff from full file content (the client, or
/// the dashboard, fetching `GetApplied`) checks this before ever putting
/// that content in a diff — never content for a secret, only "changed".
pub fn is_secret_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env" || name.starts_with(".env.")
}

/// fix-219: how much of one file's diff a drift finding shows inline, before
/// "… N more lines" — short enough to read beside the finding, not a
/// replacement for `git diff` on the stack directory itself.
pub const FILE_DIFF_MAX_LINES: usize = 20;

/// fix-142 (nightly hash comparison, Kenny's go 2026-10-01): the files a
/// deploy pushed under `/opt/<stack>/` against what sits in the container
/// right now, hashed inside it.
///
/// Unlike `evaluate_repo_drift`, which compares the repository with the
/// host's intent-history copy, this compares the host's own record of what
/// it pushed — post any registry-cache compose rewrite,
/// `StackState::pushed_file_hashes` — with the live container. That was the
/// missing half the original fleetcheck-blind-to-repo-drift finding could
/// not build: a deploy rewrites every cached app's compose file, so hashing
/// the repository's own copy and comparing it straight against the
/// container would call every cached app drifted. Now the host hashes what
/// it actually wrote, once, at deploy time, and only ever compares against
/// that.
///
/// Nightly only (`live.container_file_hashes` is gathered host-side, like
/// `digests` is client-side for `evaluate_repo_drift` — the two run in
/// different places and never both at once). A stack with no recorded
/// pushed hashes (never deployed since this was built, or adopted rather
/// than deployed) is skipped, and a container that could not be asked is
/// left out of the comparison rather than reported as entirely gone — an
/// unasked question is not a finding. A file the container has that was
/// never pushed is not reported either: `/opt/<stack>/` legitimately holds
/// files no deploy wrote (logs, caches, `generated_dirs`), and listing those
/// every night would bury the files that actually matter.
pub fn evaluate_container_drift(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        if st.pushed_file_hashes.is_empty() {
            continue;
        }
        let Some(live_files) = live.container_file_hashes.get(name) else {
            continue;
        };
        let list = |names: Vec<&String>| -> String {
            names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let changed: Vec<&String> = st
            .pushed_file_hashes
            .iter()
            .filter(|(p, h)| live_files.get(*p).is_some_and(|c| c != *h))
            .map(|(p, _)| p)
            .collect();
        let gone: Vec<&String> = st
            .pushed_file_hashes
            .keys()
            .filter(|p| !live_files.contains_key(*p))
            .collect();
        if changed.is_empty() && gone.is_empty() {
            continue;
        }
        let mut parts: Vec<String> = Vec::new();
        if !changed.is_empty() {
            parts.push(format!("changed inside the container: {}", list(changed)));
        }
        if !gone.is_empty() {
            parts.push(format!("gone from the container: {}", list(gone)));
        }
        out.push(Finding {
            severity: Severity::Drift,
            subject: name.clone(),
            what: format!(
                "/opt/{} no longer matches what the last deploy pushed — {}",
                name,
                parts.join("; ")
            ),
            remedy: format!(
                "redeploy {} to put back what was pushed, or adopt the change if it was \
                 intentional — nothing but a deploy is meant to write these files",
                name
            ),
        });
    }
    out
}

/// fix-110 (homelab-admin, 2026-10-01): `config/host.toml` against the
/// host's own running host.toml, key by key — the same shape
/// `evaluate_repo_drift` compares a stack's files in. `declared_host_config`
/// absent (an older client, the nightly round, or a repository without the
/// file yet) skips the comparison entirely, same as empty `digests` skips
/// the stack-file one. A key either side sets that the other does not, or
/// sets to a different value, is one Drift finding naming both.
pub fn evaluate_host_config_drift(live: &LiveFacts) -> Vec<Finding> {
    let Some(declared) = &live.declared_host_config else {
        return Vec::new();
    };
    // fix-170: a secret or a host-held key (`tokens`, generated and kept by
    // the host itself) is never the repository's to declare, so it is
    // never compared — belt and braces on top of the callers that already
    // leave these out of both maps.
    let mut keys: std::collections::BTreeSet<&String> = declared
        .keys()
        .chain(live.live_host_config.keys())
        .filter(|k| !crate::hostconfig::is_secret(k) && !crate::hostconfig::is_host_held(k))
        .collect();
    // The order drives only the output; sorted for a stable report.
    let mut out = Vec::new();
    while let Some(key) = keys.pop_first() {
        let want = declared.get(key);
        let have = live.live_host_config.get(key);
        if want == have {
            continue;
        }
        // fix-181: a key the host no longer reads at all (retired — it has
        // left `hostconfig::KEYS`, e.g. the Kuma/Grafana keys superseded by
        // later work) but still sits in the host's own host.toml is not
        // drift against the repository, which rightly declares nothing for
        // it any more — it is a leftover `homelab host apply` would clear.
        if crate::hostconfig::key_info(key).is_none() {
            if let Some(v) = have {
                out.push(Finding {
                    severity: Severity::Drift,
                    subject: "host.toml".into(),
                    what: format!("{key}: retired key still set on the host ({v})"),
                    remedy: "`homelab host apply` removes it — the host no longer reads this key"
                        .into(),
                });
            }
            continue;
        }
        // fix-181: compare EFFECTIVE values. A key either side leaves unset
        // runs as its compiled default (`hostconfig::default_effective`),
        // so declaring that same default explicitly is not a difference —
        // only a value that actually diverges from what the other side
        // ends up running is.
        let effective = |v: Option<&serde_json::Value>| {
            v.cloned()
                .or_else(|| crate::hostconfig::default_effective(key))
        };
        if effective(want) == effective(have) {
            continue;
        }
        let describe = |v: Option<&serde_json::Value>| match v {
            Some(v) => v.to_string(),
            None => "its compiled default (unset)".to_string(),
        };
        out.push(Finding {
            severity: Severity::Drift,
            subject: "host.toml".into(),
            what: format!(
                "{key}: config/host.toml declares {}, the host's host.toml has {}",
                describe(want),
                describe(have)
            ),
            remedy: "`homelab host apply` to make the host match the repository, or edit \
                      config/host.toml to match the host and commit that"
                .into(),
        });
    }
    out
}

/// fix-26: one storage directory owned by someone other than its stack file
/// says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnerFact {
    pub path: String,
    pub stack: String,
    pub declared: u32,
    pub actual: u32,
}

/// fix-26: parse `stat -c '%u %n'` lines against the declarations. A path
/// stat could not read produces no line and no fact: a missing directory is
/// the deploy's business, not this check's.
pub fn owner_facts(transcript: &str, declared: &[(String, u32, String)]) -> Vec<OwnerFact> {
    let mut out = Vec::new();
    for line in transcript.lines() {
        let Some((uid, path)) = line.trim().split_once(' ') else {
            continue;
        };
        let Ok(actual) = uid.parse::<u32>() else {
            continue;
        };
        for (p, want, stack) in declared {
            if p == path && *want != actual {
                out.push(OwnerFact {
                    path: path.to_string(),
                    stack: stack.clone(),
                    declared: *want,
                    actual,
                });
            }
        }
    }
    out
}

/// fix-26: Drift per mismatch. Not Broken: the service may run fine today,
/// and the finding is there to be read before a restart surfaces it. Story:
/// `docs/deployment/REGISTER.md`.
pub fn evaluate_owners(facts: &[OwnerFact]) -> Vec<Finding> {
    facts
        .iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: format!("{} ({})", f.path, f.stack),
            what: format!(
                "owned by uid {} on disk, while the stack file declares host_owner_uid {}",
                f.actual, f.declared
            ),
            remedy: "the next deploy chowns it to the declared uid — if the service runs as \
                     its own user, correct host_owner_uid in the stack file first (100000 + \
                     the uid inside an unprivileged container), or it stops at its next restart"
                .into(),
        })
        .collect()
}

/// fix-24: one oversized log file on a data mount.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BigLogFact {
    pub path: String,
    pub bytes: u64,
    /// Whether the mount declares `rotate:` — a big file under a declared
    /// rule means the rule is not running, which is a different remedy.
    pub rotated: bool,
}

/// fix-24: 500 MB. Traefik's access log reached 254 MB in 25 unrotated days,
/// and the largest rotated day on record is 84 MB, so a file past this is a
/// rotation that is missing or not running, not a busy day.
pub const BIG_LOG_BYTES: u64 = 500 * 1024 * 1024;

/// fix-24: parse `find -printf '%s %p\n'` output into facts, marking the ones
/// under a mount that declares rotation.
pub fn big_log_facts(transcript: &str, rotated_prefixes: &[String]) -> Vec<BigLogFact> {
    transcript
        .lines()
        .filter_map(|l| {
            let (size, path) = l.trim().split_once(' ')?;
            let bytes = size.parse::<u64>().ok()?;
            (bytes >= BIG_LOG_BYTES).then(|| BigLogFact {
                path: path.to_string(),
                bytes,
                rotated: rotated_prefixes
                    .iter()
                    .any(|p| path.starts_with(&format!("{}/", p.trim_end_matches('/')))),
            })
        })
        .collect()
}

/// fix-24: a Drift finding per oversized log.
pub fn evaluate_big_logs(facts: &[BigLogFact]) -> Vec<Finding> {
    facts
        .iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: f.path.clone(),
            what: format!("log file is {} MB", f.bytes / 1024 / 1024),
            remedy: if f.rotated {
                "the stack declares rotation for it, so the rule is not running — check \
                 `/etc/logrotate.d/homelab-<stack>` inside the container and `systemctl status logrotate.timer`"
                    .into()
            } else {
                "nothing rotates it — declare `rotate:` on this data mount in the stack file \
                 (fix-24) and deploy"
                    .into()
            },
        })
        .collect()
}

/// W3: the configured shape of a container that exists, next to the stack
/// file that is supposed to describe it.
///
/// Read from `pct config` rather than from inside the container, so a
/// stopped guest answers as well as a running one — which matters, because
/// "does not start on boot" is precisely the state you find a container in
/// after the reboot that should have started it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BootFact {
    pub vmid: u16,
    pub hostname: String,
    pub live: crate::ops::reconcile::LiveConfig,
}

/// Is this stack actually being measured and actually shipping logs?
///
/// The most expensive class of failure in this fleet is not a service that
/// falls over — it is a mechanism that runs, reports success and is wired to
/// nothing. On 2026-08-31 alone: log caps that ran on five of nine
/// containers, a growth check that watched five of nine, a discovery file the
/// orchestrator wrote for weeks that Prometheus was never told to read, a
/// promtail pipeline reading a field docker does not write, a database
/// answering its healthcheck while every query failed, and an alert chain
/// finished on every side but the middle. Not one was caught by a test. Every
/// one was found by somebody looking.
///
/// So the check looks. Both fields are `Option`: `None` means the question
/// was not asked — no Prometheus or Loki address is configured, or the stack
/// ships no logs by design — and an unasked question must never become a
/// finding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoverageFact {
    pub stack: String,
    /// A Prometheus target for this stack answered `up == 1`.
    pub scraped: Option<bool>,
    /// Loki holds at least one line from this stack, carrying a container
    /// name, inside the configured window.
    ///
    /// Labelled, because F79 was not silence. Promtail read `attrs.name` —
    /// a field docker does not write — so lines arrived for months with an
    /// empty `container_name` and the three dashboards querying it stayed
    /// blank. A plain line count would have been green throughout.
    ///
    /// The window is a host setting (`logs_window`, default 24h) rather than
    /// the hour it started as. On 2026-09-01 the hour version reported
    /// `home` and `kp-soft` as "going nowhere" while both were healthy and
    /// merely quiet — a check that alarms on healthy silence is a check that
    /// gets switched off, and then the real silence goes unnoticed too.
    pub logs_recent: Option<bool>,
    /// Its service.yml says `metrics: false`: deliberately not measured, so
    /// Prometheus is not asked and the check notes it (Phase 9, inbox).
    pub unmeasured_by_choice: bool,
}

/// One container's resource picture, as read off the machine.
///
/// G3 exists because of what G1 found: the guards that cap logs had been
/// written months earlier and ran on almost nothing, and Loki had quietly
/// written 923 MB of its own output. Nothing was watching. Kenny's bar is
/// that a container should be able to run for a hundred years — which is
/// only meaningful if something notices when it starts trending otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrowthFact {
    pub vmid: u16,
    pub hostname: String,
    /// Percentage of the container's rootfs in use.
    pub disk_used_pct: u8,
    /// Percentage of the container's memory allocation in use.
    pub mem_used_pct: u8,
    /// Swap actually in use, in MB.
    pub swap_used_mb: u32,
    /// Size of the systemd journal, in MB.
    pub journal_mb: u32,
    /// Size of docker's container log directory, in MB.
    pub docker_logs_mb: u32,
    /// Whether the runaway guards are installed *now* — a journald cap and
    /// a docker log cap present on disk. Not whether they were ever applied:
    /// that is exactly the distinction that let five containers run without
    /// them while the code that writes them had existed all along.
    pub guards: bool,
}

/// Where "growing" turns into "worth telling Kenny about".
///
/// Standing rule 27: a number expressing tolerance belongs in configuration,
/// and these are its defaults. They are deliberately far below the point of
/// failure — the whole idea is to see the trend, not the wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrowthLimits {
    /// Rootfs full enough that the next update can fail.
    pub disk_broken_pct: u8,
    /// Rootfs trending toward that.
    pub disk_drift_pct: u8,
    /// Memory this close to the allocation will start swapping.
    pub mem_drift_pct: u8,
    /// Any swap beyond this means pressure is being hidden rather than felt.
    pub swap_drift_mb: u32,
    /// A journal past this is not being capped effectively.
    pub journal_mb: u32,
    /// Docker logs past this are not being rotated effectively.
    pub docker_logs_mb: u32,
    /// A data pool this full is one where the next import can fail.
    pub pool_broken_pct: u8,
    /// A data pool trending toward that.
    pub pool_drift_pct: u8,
}

impl Default for GrowthLimits {
    fn default() -> Self {
        Self {
            // 85% of a 32 GB rootfs still leaves 4.8 GB, which is one image
            // pull; below that an update can fail halfway.
            disk_broken_pct: 85,
            disk_drift_pct: 70,
            mem_drift_pct: 90,
            // Measured on this fleet: a healthy container sits at 0. CT 106
            // sat at 1028 MB, which is what prompted G2.
            swap_drift_mb: 64,
            // The guards cap journald at 100 MB; 150 means the cap is absent
            // or not taking effect.
            journal_mb: 150,
            // 10m x 3 files per container: 250 MB is roughly eight busy
            // containers' worth, or one that is not being rotated.
            docker_logs_mb: 250,
            // R13: nothing in this suite looked at the pools the libraries
            // actually live on — `disk_*_pct` above is the container's own
            // rootfs, which on CT 106 is 80 GB while the films sit on 16.4 TB
            // of ZFS beside it.
            //
            // HIGHER than the rootfs thresholds, and the first draft had that
            // backwards on the reasoning that freeing a media pool takes days
            // rather than seconds — true, and served by warning at drift long
            // before broken, not by a lower percentage. At the rootfs's 70%
            // this pool would report 4.9 TB free as a problem.
            //
            // A percentage rather than "warn under 2 TB", which is what Kenny
            // asked for in R13 and is the wrong shape once measured: HDD2TB
            // holds 1798 GB free while being 1% used, so an absolute rule
            // would open with a finding about a pool that is empty. The free
            // space he wants to see is in the message instead, beside the
            // percentage. Measured 2026-09-04: 30% / 79% / 1% / 1% across the
            // four declared pools, so this opens silent and HDD12TB is the
            // one to cross first.
            pool_broken_pct: 90,
            pool_drift_pct: 80,
        }
    }
}

/// R13 · how full is a pool a stack keeps its data on.
///
/// It exists because the answer used to be nobody's business. Every disk
/// number this suite reads is a container's own rootfs; the libraries live on
/// ZFS pools bind-mounted in beside it, and those were measured by no check at
/// all. That was tolerable while the profiles said "any 1080p file is done".
/// It stopped being tolerable when Recyclarr started replacing 27 MB/min
/// releases with ones near Kenny's 95 MB/min preference (R13): the same 943
/// films go from 4.1 TB to roughly 10 TB, and the only thing standing between
/// that and a full pool was somebody happening to look.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PoolFact {
    /// The filesystem, as `df` names it — the key, because two stacks that
    /// declare different paths on the same pool are one pool, not two.
    pub filesystem: String,
    /// A declared path that lives on it, for a message that names something
    /// Kenny recognises rather than a dataset name.
    pub path: String,
    /// Which stacks keep data there.
    pub stacks: Vec<String>,
    pub used_pct: u8,
    pub free_gb: u64,
}

/// R13: turn one `df` transcript into pool facts.
///
/// Pure, and in core rather than beside the command that produces it, because
/// two things about it are easy to get wrong and impossible to see once wrong:
///
/// * **The path has to come from the line, not from the position.** A path
///   that does not exist produces no `df` row at all, so reading rows
///   positionally shifts every later one up and reports one stack's pool under
///   another stack's name — with entirely plausible numbers. The caller
///   prints the path it asked about at the start of each line for exactly this
///   reason.
/// * **A path can be declared by several stacks.** CT 105 and CT 106 both
///   mount `/HDD18TB/subvol-103-disk-0`. Taking the first declaration that
///   matches names one of them and silently drops the other, which is how a
///   finding sends somebody to the wrong container.
///
/// Line shape, as the caller emits it: `<path> <fs> <1k-blocks> <used>
/// <avail> <pct> <mountpoint>`. Anything shorter is a path `df` could not
/// answer for — a missing mount, which is a different check's business.
pub fn pool_facts_from_df(transcript: &str, declared: &[(String, String)]) -> Vec<PoolFact> {
    let mut by_fs: std::collections::BTreeMap<String, PoolFact> = std::collections::BTreeMap::new();
    for line in transcript.lines() {
        let c: Vec<&str> = line.split_whitespace().collect();
        if c.len() < 7 {
            continue;
        }
        let path = c[0];
        let (Ok(used), Ok(avail)) = (c[3].parse::<u64>(), c[4].parse::<u64>()) else {
            continue;
        };
        let total = used + avail;
        if total == 0 {
            continue;
        }
        let owners: Vec<&String> = declared
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, s)| s)
            .collect();
        if owners.is_empty() {
            continue;
        }
        let e = by_fs.entry(c[1].to_string()).or_insert_with(|| PoolFact {
            filesystem: c[1].to_string(),
            path: path.to_string(),
            used_pct: (used * 100 / total) as u8,
            free_gb: avail / 1024 / 1024,
            ..Default::default()
        });
        for owner in owners {
            if !e.stacks.contains(owner) {
                e.stacks.push(owner.clone());
            }
        }
    }
    for f in by_fs.values_mut() {
        f.stacks.sort();
    }
    by_fs.into_values().collect()
}

/// R13: a finding per pool that is filling up. Deduplicated by filesystem by
/// the caller — this only judges.
pub fn evaluate_pools(facts: &[PoolFact], lim: GrowthLimits) -> Vec<Finding> {
    let mut out = Vec::new();
    for p in facts {
        let who = if p.stacks.is_empty() {
            p.path.clone()
        } else {
            format!("{} ({})", p.path, p.stacks.join(", "))
        };
        // Free space is in the message on purpose, next to the percentage.
        // A percentage scales across a 2 TB pool and an 18 TB one; "1.6 TB
        // left" is the number somebody actually reasons with.
        let size = format!("{}% full, {} GB free", p.used_pct, p.free_gb);
        if p.used_pct >= lim.pool_broken_pct {
            out.push(Finding {
                severity: Severity::Broken,
                subject: who,
                what: format!("data pool {}", size),
                remedy: "an import lands here and there is no room for it — free space, or \
                         stop the quality upgrades that are filling it"
                    .into(),
            });
        } else if p.used_pct >= lim.pool_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who,
                what: format!("data pool {}", size),
                remedy: "still fine, and worth knowing which way it is going before it is \
                         urgent — `zfs list -o name,used,avail` on the host"
                    .into(),
            });
        }
    }
    out
}

// ── rule-20: host-level capacity thresholds ────────────────────────────────
//
// Disk-growth audit (2026-10-01): measurable today but unalarmed — pve's own
// root filesystem, the local-lvm thin pool's data and metadata percentages,
// every ZFS pool, and pve's journald against its own 2G cap
// (`hostunits::JOURNALD_CAP`). One evaluator for all of them, generic over
// which metric a fact names, the same shape as `evaluate_pools` beside it:
// the shell measures, this only judges.

/// Which host-level capacity reading a `HostCapacityFact` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostCapacityMetric {
    /// `/` on pve itself.
    PveRoot,
    /// local-lvm thin pool, data%.
    ThinPoolData,
    /// local-lvm thin pool, metadata% — harder to recover from than data%,
    /// so it gets its own, lower, pair of thresholds.
    ThinPoolMeta,
    /// One ZFS pool, named in `subject`.
    ZfsPool,
    /// pve's own journald, against its own `SystemMaxUse` cap
    /// (`hostunits::JOURNALD_CAP`) — distinct from a managed container's own
    /// small cap, which `GrowthFact::journal_mb` already covers.
    Journald,
    /// Prometheus' TSDB against its configured `--storage.tsdb.retention.size`.
    PrometheusTsdb,
    /// The homelab daemon's own native-backup staging directory
    /// (`native_backup_staging_dir`, default `/appdata/.backup-staging`)
    /// against `native_backup_staging_cap_mib` — it is emptied after every
    /// run, so anything here at all past a run is itself the finding.
    NativeBackupStaging,
}

impl HostCapacityMetric {
    fn label(self) -> &'static str {
        match self {
            Self::PveRoot => "pve's root filesystem",
            Self::ThinPoolData => "the local-lvm thin pool (data)",
            Self::ThinPoolMeta => "the local-lvm thin pool (metadata)",
            Self::ZfsPool => "ZFS pool",
            Self::Journald => "pve's journald",
            Self::PrometheusTsdb => "Prometheus' TSDB",
            Self::NativeBackupStaging => "the native-backup staging directory",
        }
    }
    fn remedy(self) -> &'static str {
        match self {
            Self::PveRoot => {
                "free space on pve itself — apt cache, old kernels, /var/lib/vz ISOs and \
                 templates — or grow pve/root"
            }
            Self::ThinPoolData => {
                "an LXC or VM disk write can fail once this pool is full — free space (prune \
                 old container disks/snapshots) or extend the pool"
            }
            Self::ThinPoolMeta => {
                "metadata exhaustion is harder to recover from than data: `lvextend \
                 --poolmetadatasize` ahead of time, not after"
            }
            Self::ZfsPool => "`zfs list -o name,used,avail` on the host — free space or add a vdev",
            Self::Journald => {
                "journald rotates its own oldest entries past this, but it is close to the cap \
                 all the time — `journalctl --vacuum-size` or raise SystemMaxUse in \
                 hostunits.rs"
            }
            Self::PrometheusTsdb => {
                "retention.size is the ceiling, not a target — either it is doing its job and \
                 this is expected, or retention.time is now too generous for the ceiling"
            }
            Self::NativeBackupStaging => {
                "it is emptied after every run — this means the last run did not finish \
                 cleanly; check the nightly log, then clear it by hand if it is stale"
            }
        }
    }
}

/// One host-level capacity reading, judged against `HostCapacityThresholds`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCapacityFact {
    pub metric: HostCapacityMetric,
    /// What to call it in the report — "pve", a ZFS pool's name, "Prometheus
    /// (CT 113)". `metric` already says WHAT kind of reading this is; this
    /// says WHICH one.
    pub subject: String,
    pub used_pct: u8,
    /// Extra detail for the message, e.g. "27.66% of 794.3G pool" — numbers
    /// Kenny can act on beside the bare percentage.
    pub detail: String,
    /// fix-181: only meaningful for `HostCapacityMetric::Journald` — whether
    /// a `SystemMaxUse` cap is actually in force (the
    /// `hostunits::JOURNALD_CAP` drop-in is present on disk). A capped
    /// journal sits near its cap *by design* (journald rotates its oldest
    /// entries to stay under it), so `used_pct` close to or at 100% of the
    /// cap is the healthy steady state, not drift — the percentage-threshold
    /// judgment below only applies while no cap is proven to exist. Every
    /// other metric always has a "cap" (the filesystem, the pool, the
    /// retention setting) so this is simply `true` for them.
    pub cap_configured: bool,
}

/// Warn/critical pairs, one per `HostCapacityMetric`. A host.toml key
/// (`capacity_thresholds`), editable from the dashboard like every other
/// fleet default. Defaults are the disk-growth audit's own numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapacityThresholds {
    pub pve_root_warn_pct: u8,
    pub pve_root_critical_pct: u8,
    pub thin_data_warn_pct: u8,
    pub thin_data_critical_pct: u8,
    pub thin_meta_warn_pct: u8,
    pub thin_meta_critical_pct: u8,
    pub zfs_pool_warn_pct: u8,
    pub zfs_pool_critical_pct: u8,
    /// Of pve journald's own `SystemMaxUse` cap (2G, `hostunits::JOURNALD_CAP`).
    pub journald_warn_pct: u8,
    pub journald_critical_pct: u8,
    /// Of Prometheus' configured `--storage.tsdb.retention.size`.
    pub tsdb_warn_pct: u8,
    pub tsdb_critical_pct: u8,
    /// Of `native_backup_staging_cap_mib` — low on purpose: the directory is
    /// emptied after every run, so ANY leftover is already worth a look,
    /// well before it could ever fill the cap.
    pub native_backup_staging_warn_pct: u8,
    pub native_backup_staging_critical_pct: u8,
}

impl Default for HostCapacityThresholds {
    fn default() -> Self {
        Self {
            pve_root_warn_pct: 70,
            pve_root_critical_pct: 85,
            thin_data_warn_pct: 70,
            thin_data_critical_pct: 85,
            // Lower and narrower than data: metadata exhaustion takes the
            // pool down in a way data filling up does not.
            thin_meta_warn_pct: 50,
            thin_meta_critical_pct: 70,
            zfs_pool_warn_pct: 80,
            zfs_pool_critical_pct: 90,
            journald_warn_pct: 80,
            journald_critical_pct: 95,
            tsdb_warn_pct: 70,
            tsdb_critical_pct: 90,
            // 10% of a 10 GiB default cap is already 1 GiB sitting where
            // nothing should be between runs.
            native_backup_staging_warn_pct: 10,
            native_backup_staging_critical_pct: 50,
        }
    }
}

impl HostCapacityThresholds {
    fn pair(self, metric: HostCapacityMetric) -> (u8, u8) {
        match metric {
            HostCapacityMetric::PveRoot => (self.pve_root_warn_pct, self.pve_root_critical_pct),
            HostCapacityMetric::ThinPoolData => {
                (self.thin_data_warn_pct, self.thin_data_critical_pct)
            }
            HostCapacityMetric::ThinPoolMeta => {
                (self.thin_meta_warn_pct, self.thin_meta_critical_pct)
            }
            HostCapacityMetric::ZfsPool => (self.zfs_pool_warn_pct, self.zfs_pool_critical_pct),
            HostCapacityMetric::Journald => (self.journald_warn_pct, self.journald_critical_pct),
            HostCapacityMetric::PrometheusTsdb => (self.tsdb_warn_pct, self.tsdb_critical_pct),
            HostCapacityMetric::NativeBackupStaging => (
                self.native_backup_staging_warn_pct,
                self.native_backup_staging_critical_pct,
            ),
        }
    }
}

/// rule-20: pve's own root filesystem, from `df --output=pcent /` (or any
/// single-column `df` output ending in a percentage) — the last non-empty
/// line, so a header row (`Use%`) is skipped without needing to know
/// whether one was printed.
pub fn parse_df_pcent(out: &str) -> Option<u8> {
    out.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .trim_end_matches('%')
        .parse()
        .ok()
}

/// rule-20: `lvs --noheadings -o data_percent,metadata_percent <thin-pool>`
/// — one line, two space-separated percentages (lvs prints them as plain
/// decimals, e.g. `27.66  1.15`, no `%` sign). Returns `(data, meta)`
/// rounded to the nearest whole percent.
pub fn parse_thin_pool_percents(out: &str) -> Option<(u8, u8)> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut parts = line.split_whitespace();
    let data: f64 = parts.next()?.parse().ok()?;
    let meta: f64 = parts.next()?.parse().ok()?;
    Some((data.round() as u8, meta.round() as u8))
}

/// rule-20: `zpool list -H -o name,capacity` — `-H` makes it tab-separated,
/// one pool per line, capacity as e.g. `79%`. Every pool on the host, not
/// only the ones a stack happens to declare a mount on.
pub fn parse_zpool_capacities(out: &str) -> Vec<(String, u8)> {
    out.lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let name = c.next()?.trim();
            let pct: u8 = c.next()?.trim().trim_end_matches('%').parse().ok()?;
            (!name.is_empty()).then(|| (name.to_string(), pct))
        })
        .collect()
}

/// rule-20: `journalctl --disk-usage`, e.g. "Archived and active journals \
/// take up 1.9G in the file system." — the number and unit just before "in
/// the file system", converted to MiB. None when the line cannot be read,
/// which is not the same as "empty" and is simply not reported.
pub fn parse_journal_disk_usage_mib(out: &str) -> Option<u64> {
    let line = out.lines().find(|l| l.contains("in the file system"))?;
    let token = line.split_whitespace().find(|t| {
        t.chars().next().is_some_and(|c| c.is_ascii_digit())
            && matches!(t.chars().last(), Some('K' | 'M' | 'G' | 'T'))
    })?;
    let (num, unit) = token.split_at(token.len() - 1);
    let value: f64 = num.parse().ok()?;
    let mib = match unit {
        "K" => value / 1024.0,
        "M" => value,
        "G" => value * 1024.0,
        "T" => value * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(mib.round() as u64)
}

/// rule-20: the scalar value of a Prometheus instant-query response, e.g.
/// `{"status":"success","data":{"resultType":"vector","result":[{"metric":{},"value":[1,"12345"]}]}}`
/// — the number right after the first `"value":[<timestamp>,`. None for an
/// empty result (`"result":[]`, nothing scraped yet) or anything that does
/// not parse, which is not the same as zero and is simply not reported.
pub fn parse_prometheus_scalar(json: &str) -> Option<f64> {
    let after = json.split_once("\"value\":[")?.1;
    let after_ts = after.split_once(',')?.1;
    let quoted = after_ts.split_once('"')?.1;
    let (num, _) = quoted.split_once('"')?;
    num.parse().ok()
}

/// rule-20: `du -sm <dir>` — the first whitespace-separated token, in MiB.
pub fn parse_du_sm(out: &str) -> Option<u64> {
    out.lines().next()?.split_whitespace().next()?.parse().ok()
}

/// rule-20: a finding per host-level capacity reading past its warn or
/// critical threshold. Pure — the shell measures, this only judges.
pub fn evaluate_host_capacity(
    facts: &[HostCapacityFact],
    lim: HostCapacityThresholds,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in facts {
        if f.metric == HostCapacityMetric::Journald {
            // fix-181: a capped journal sits near 100% of its cap all the
            // time — that is journald rotating, not drift. Judge it on
            // whether the cap exists and holds, not on a percentage scale
            // built for things that are broken long before they are full.
            if !f.cap_configured {
                out.push(Finding {
                    severity: Severity::Broken,
                    subject: f.subject.clone(),
                    what: format!(
                        "{} has no SystemMaxUse cap configured — it is {}",
                        f.metric.label(),
                        f.detail
                    ),
                    remedy: "the host's own journald cap drop-in \
                             (hostunits::JOURNALD_CAP) is missing — \
                             `homelab self-update` or `homelab doctor` \
                             should have put it back"
                        .into(),
                });
            } else if f.used_pct > 100 {
                out.push(Finding {
                    severity: Severity::Broken,
                    subject: f.subject.clone(),
                    what: format!("{} is over its own cap — {}", f.metric.label(), f.detail),
                    remedy: "a cap is set but journald is not rotating within it — \
                             `journalctl --vacuum-size` now, and check that \
                             systemd-journald actually restarted after the cap \
                             was written"
                        .into(),
                });
            }
            continue;
        }
        let (warn, critical) = lim.pair(f.metric);
        if f.used_pct >= critical {
            out.push(Finding {
                severity: Severity::Broken,
                subject: f.subject.clone(),
                what: format!("{} is {}", f.metric.label(), f.detail),
                remedy: f.metric.remedy().into(),
            });
        } else if f.used_pct >= warn {
            out.push(Finding {
                severity: Severity::Drift,
                subject: f.subject.clone(),
                what: format!("{} is {}", f.metric.label(), f.detail),
                remedy: f.metric.remedy().into(),
            });
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFact {
    pub file: String,
    pub target: String,
    pub answered: bool,
}

/// How stale a backup may be before it counts as a finding. A nightly run
/// that missed one night is noise; two is a pattern (standing rule 27 — a
/// number about patience belongs in configuration, and this is its default).
pub const DEFAULT_BACKUP_MAX_AGE_S: u64 = 48 * 3600;

/// Which findings are worth waking somebody for.
///
/// Z3: a `Noted` finding is a decision, not a fault — the registry cache
/// that is deliberately not backed up, a mount declared unkept. Letting one
/// raise the alarm trains the reader to ignore the notification, and that
/// notification is the one that has to be believed when it IS real.
///
/// Extracted from the nightly loop on 2026-09-02 (G15 of the Phase-7 gate).
/// It lived inside a 340-line async function that no test could reach, which
/// meant the rule deciding what counts as an alarm was itself unguarded.
pub fn alarming(findings: &[Finding]) -> Vec<Finding> {
    findings
        .iter()
        .filter(|f| f.severity != Severity::Noted)
        .cloned()
        .collect()
}

/// fix-65 (nightly-report-always-red, 2026-09-27): which things are alarming,
/// as one comparable string. Severity and subject only: the wording carries
/// ages and counts that move every night, and a fingerprint that moves every
/// night would send the same report every night — the failure this exists
/// to end.
pub fn report_fingerprint(findings: &[Finding]) -> String {
    let mut keys: Vec<String> = alarming(findings)
        .iter()
        .map(|f| format!("{:?}|{}", f.severity, f.subject))
        .collect();
    keys.sort();
    keys.dedup();
    keys.join("\n")
}

/// fix-65: does tonight's alarming set go out as a notification? The same
/// red report every night taught its reader to ignore it, and a real new
/// problem then arrived in that envelope. It goes out when the set changed
/// since the last one sent, and once a week while it stands.
pub fn nightly_report_due(
    fingerprint: &str,
    last_fingerprint: &str,
    last_sent: u64,
    now: u64,
    repeat_s: u64,
) -> bool {
    fingerprint != last_fingerprint || now.saturating_sub(last_sent) >= repeat_s
}

/// fix-147 (restore-check-failure, Kenny 2026-09-27, form "Keuzes helpers"):
/// a deploy that found data empty and could not check its backup goes on
/// (backup-target trouble never blocks a deploy, E3), so the service may be
/// running on empty data while a history exists. fix-54 made that a warning
/// in the transcript; this keeps it standing, on its stack, until a later
/// check of the same directory or unit succeeds (a deploy, or a restore).
pub fn evaluate_restore_checks(state: &HostState) -> Vec<Finding> {
    state
        .restore_check_failures
        .values()
        .map(|r| Finding {
            severity: Severity::Broken,
            subject: r.stack.clone(),
            what: format!(
                "the deploy of {} found {} empty and could not check its backup ({}) — it \
                 started without that data, and a history may exist",
                crate::state::ymd(r.at),
                r.what,
                r.why
            ),
            remedy: format!(
                "check the backup target, then restore the data if it held any (`homelab \
                 restore stacks/{} --app <app>`, or op-11 for a native unit); a later deploy \
                 whose check of it succeeds, or a restore, clears this",
                r.stack
            ),
        })
        .collect()
}

/// fix-111: older than this, the host-meta backup has stopped: two nights.
pub const HOST_META_MAX_AGE_S: u64 = 48 * 3600;

/// fix-111 (host-meta-gaps, 2026-09-27): the daemon's own repository — the
/// vault with the restic password, state.json, TLS and host.toml — had no
/// doctor line and no finding, so a host-meta backup that stopped was
/// reported nowhere until the host was lost. A host that manages no stack
/// yet is left alone: there is nothing of its own worth keeping.
pub fn evaluate_host_meta(state: &HostState, now: u64, max_age_s: u64) -> Vec<Finding> {
    if state.stacks.is_empty() {
        return Vec::new();
    }
    let age = now.saturating_sub(state.last_host_meta);
    if state.last_host_meta != 0 && age <= max_age_s {
        return Vec::new();
    }
    vec![Finding {
        severity: Severity::Broken,
        subject: "host-meta".into(),
        what: if state.last_host_meta == 0 {
            "the daemon's own state (vault, state.json, TLS, host.toml) has never been backed up"
                .into()
        } else {
            format!(
                "the daemon's own state was last backed up {} hours ago",
                age / 3600
            )
        },
        remedy: "a lost host disk now loses the vault and the restic password with it; run \
                 `homelab backup-host-meta` and read why the nightly one failed"
            .into(),
    }]
}

/// fix-65: how often a standing alarming set is sent again.
pub const NIGHTLY_REPORT_REPEAT_S: u64 = 7 * 24 * 3600;

/// Whether `homelab check` passes: nothing alarming (gap-32). `noted`
/// findings need nothing done and are printed only so they stay visible, so
/// they no longer turn the answer into a failure (exit 1).
pub fn check_passes(findings: &[Finding]) -> bool {
    alarming(findings).is_empty()
}

/// The check as text, for `homelab check`, the TUI and the nightly log.
///
/// fix-103: a summary line comes first, then each severity as its own
/// group, the one that needs action on top. The `[broken]`/`[drift]`/
/// `[noted]` tags stay on every item, so a client can colour it and a grep
/// still finds it. Story: `docs/deployment/REGISTER.md`.
pub fn render(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "fleet check: repo and reality agree".into();
    }
    let groups = [
        (Severity::Broken, "broken", "not doing its job now"),
        (
            Severity::Drift,
            "drift",
            "works, bites on the next deploy or outage",
        ),
        (Severity::Noted, "noted", "nothing to do"),
    ];
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let summary: Vec<String> = groups
        .iter()
        .filter(|(s, _, _)| count(*s) > 0)
        .map(|(s, tag, _)| match s {
            Severity::Noted => format!("{} {} (nothing to do)", count(*s), tag),
            _ => format!("{} {}", count(*s), tag),
        })
        .collect();
    let mut out = format!("fleet check: {}\n", summary.join(" · "));
    for (severity, tag, meaning) in groups {
        let items: Vec<&Finding> = findings.iter().filter(|f| f.severity == severity).collect();
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("{} — {}:\n", tag, meaning));
        for f in items {
            out.push_str(&format!(
                "  [{}] {} — {}\n      remedy: {}\n",
                tag, f.subject, f.what, f.remedy
            ));
        }
    }
    out
}

/// The whole comparison, as one pure function.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
    growth_limits: GrowthLimits,
    tile_watch_source: Option<&str>,
    patch_threshold_s: u64,
    host_meta_max_age_s: u64,
    host_capacity_limits: HostCapacityThresholds,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // fix-150: updates standing still for longer than a week.
    out.extend(evaluate_patch_state(&live.patch, patch_threshold_s));

    // rule-20: pve's own capacity — root fs, thin pool, journald, every ZFS
    // pool, Prometheus' TSDB.
    out.extend(evaluate_host_capacity(
        &live.host_capacity,
        host_capacity_limits,
    ));

    // F184: is the HOST itself short? Read once, so every per-container
    // remedy below can say something the machine can actually do.
    let host_short: Option<String> = live.host_memory.and_then(|(total, committed, su, st)| {
        let oversubscribed = committed > total;
        let swap_pressed = st > 0 && su * 100 / st.max(1) >= 50;
        (oversubscribed || swap_pressed).then(|| {
            format!(
                "the host has {} MB of RAM with {} MB promised to guests, and {} of its {} MB \
                 of swap in use",
                total, committed, su, st
            )
        })
    });

    // A stack that clones a template which is not there rebuilds into
    // nothing — and it fails at the moment you need the rebuild most. The
    // shape this guards against was found on 2026-09-01: the scaffold default
    // still named `clone:999`, the v1 golden image, two generations after
    // every live stack had moved to 997/998. Nobody noticed because nobody
    // scaffolds a stack often, and the eleven that exist carry their template
    // by hand.
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(rest) = m.lxc.template.trim_matches('"').strip_prefix("clone:") else {
            continue;
        };
        let Ok(tmpl_vmid) = rest.trim().parse::<u16>() else {
            continue;
        };
        if !live.containers.iter().any(|(v, _)| *v == tmpl_vmid) {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "clones template {}, which does not exist on the hypervisor",
                    tmpl_vmid
                ),
                remedy: format!(
                    "point {}'s lxc.template at a golden template that is there, \
                     or rebuild {} with `homelab template-build`",
                    name, tmpl_vmid
                ),
            });
        }
    }

    for (name, st) in &state.stacks {
        match live.containers.iter().find(|(v, _)| *v == st.vmid) {
            None => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("recorded on vmid {}, which does not exist", st.vmid),
                remedy: "the container was removed outside the orchestrator — redeploy it, or remove the stack from host state".into(),
            }),
            Some((_, hostname)) if hostname != &st.hostname => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "recorded as '{}' but vmid {} is really '{}' — every operation on this stack fails the hostname guard",
                    st.hostname, st.vmid, hostname
                ),
                remedy: "re-adopt or redeploy the stack so its record matches the container; until then its backups and updates stop".into(),
            }),
            Some(_) => {}
        }

        if !st.enabled {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "disabled — the nightly run skips it entirely".into(),
                remedy: format!(
                    "a failed nightly run auto-disables a stack (H8); fix the cause, then `homelab enable {}`",
                    name
                ),
            });
        }

        // fix-59: a failed nightly update parks the updates only. The backup
        // still runs, so nothing else would ever say the updates stopped.
        if let Some(since) = state.updates_parked.get(name) {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "automatic updates parked since {} after a failed nightly update or a \
                     `homelab rollback-native` (fix-114); the nightly backup still runs",
                    crate::state::ymd(*since)
                ),
                remedy: format!(
                    "read that night's update transcript, fix or pin the image or release, \
                     then `homelab enable {}` resumes the updates",
                    name
                ),
            });
        }

        let age = now_unix.saturating_sub(st.last_backup);
        // Z3: a stack that keeps nothing worth keeping, by declaration, is
        // not a stack that was forgotten. Saying "never been backed up"
        // about a decision trains the reader to ignore the line — and that
        // line is the one that has to be believed when it IS real.
        let declared_unkept: Vec<&crate::manifest::MountSpec> = st
            .manifest
            .as_ref()
            .map(|m| m.storage.iter().filter(|s| s.no_backup.is_some()).collect())
            .unwrap_or_default();
        let keeps_nothing = st
            .manifest
            .as_ref()
            .map(|m| {
                !m.storage.is_empty()
                    && m.storage.iter().all(|s| s.no_data || s.no_backup.is_some())
            })
            .unwrap_or(false);
        for mount in &declared_unkept {
            out.push(Finding {
                severity: Severity::Noted,
                subject: name.clone(),
                what: format!(
                    "{} is deliberately not backed up — {}",
                    mount.host_path,
                    mount.no_backup.as_deref().unwrap_or("")
                ),
                remedy:
                    "nothing to do; listed so a deliberate gap never looks like a forgotten one"
                        .into(),
            });
        }
        if keeps_nothing {
            // Deliberate, already said above per mount.
        } else if st.last_backup == 0 {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "has never been backed up".into(),
                remedy: "run a backup now and check why the nightly one never did".into(),
            });
        } else if age > backup_max_age_s {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("last backup was {} hours ago", age / 3600),
                remedy: "check the nightly run and the backup target".into(),
            });
        }
    }

    // A stack file that claims a vmid belonging to something else is one
    // hostname guard away from a deploy landing on a live container.
    for (dir, vmid) in &live.stack_files {
        let owned_by_state = state.stacks.values().any(|s| s.vmid == *vmid);
        let exists = live.containers.iter().any(|(v, _)| v == vmid);
        if exists && !owned_by_state {
            out.push(Finding {
                severity: Severity::Drift,
                subject: dir.clone(),
                what: format!(
                    "claims vmid {}, which is a live container this orchestrator does not manage",
                    vmid
                ),
                remedy: "only the hostname guard stands between this file and a deploy onto that container — delete the file or point it at a free vmid".into(),
            });
        }
    }

    // step-22 / ask-8: the other direction. A stack in host state whose
    // directory is gone from the repository is still running, still backed
    // up and still registered everywhere — and nothing compared the two, so
    // deleting a stack directory changed nothing and said nothing. Kenny's
    // answer (form 2026-09-27, `Via homelab apply`): it is reported here
    // until `homelab apply` destroys it after its name is typed.
    //
    // Only when the client sent its stack files at all: an empty list means
    // the check ran from somewhere without the repository, and judging
    // against nothing would call every stack deleted.
    if !live.stack_files.is_empty() {
        let dirs: std::collections::BTreeSet<&str> = live
            .stack_files
            .iter()
            .map(|(d, _)| {
                let d = d.trim_end_matches('/');
                d.rsplit('/').next().unwrap_or(d)
            })
            .collect();
        for (name, st) in &state.stacks {
            if dirs.contains(name.as_str()) {
                continue;
            }
            out.push(Finding {
                severity: Severity::Drift,
                subject: name.clone(),
                what: format!(
                    "is in host state (vmid {}) but has no stack file in the repository — \
                     stacks/{}/ is gone",
                    st.vmid, name
                ),
                remedy: format!(
                    "`homelab apply` lists it and destroys it after you type its name; put \
                     stacks/{}/ back to keep it",
                    name
                ),
            });
        }
    }

    // fix-142: the files against what the host applied.
    out.extend(evaluate_repo_drift(state, live));
    out.extend(evaluate_container_drift(state, live));
    out.extend(evaluate_host_config_drift(live));

    out.extend(evaluate_growth(
        &live.growth,
        growth_limits,
        host_short.as_deref(),
    ));
    out.extend(evaluate_pools(&live.pools, growth_limits));
    out.extend(evaluate_big_logs(&live.big_logs));
    out.extend(evaluate_owners(&live.owners));
    out.extend(evaluate_route_owners(state, &live.route_files));
    out.extend(evaluate_firewalls(
        state,
        &live.firewalls,
        &live.boot,
        tile_watch_source,
    ));
    out.extend(evaluate_coverage(&live.coverage));
    out.extend(evaluate_boot(state, &live.boot));
    out.extend(evaluate_watched_backups(&live.watched_backups));
    out.extend(evaluate_incomplete(state));
    out.extend(crate::ops::retired::evaluate_retired(state));
    out.extend(evaluate_notify(state, now_unix));
    // fix-94: a CrowdSec whitelist running on a last known home address.
    out.extend(crate::ops::homeaddress::evaluate_home_address(state));
    // fix-83: a pinned manual app whose upstream released something newer.
    out.extend(crate::ops::pins::evaluate_pins(state));
    out.extend(crate::ops::restoredrill::evaluate_drill(
        state,
        now_unix,
        crate::ops::restoredrill::DEFAULT_DRILL_INTERVAL_S,
    ));
    out.extend(evaluate_host_meta(state, now_unix, host_meta_max_age_s));
    out.extend(evaluate_restore_checks(state));
    // fix-96: the second copy and the rotating restic check.
    out.extend(crate::ops::secondcopy::evaluate_copies(
        state,
        now_unix,
        live.second_copy_dataset.as_deref(),
        crate::ops::secondcopy::DEFAULT_CHECK_INTERVAL_S,
    ));
    out.extend(crate::ops::manualchecks::evaluate_manual(state, now_unix));
    out.extend(crate::ops::probes::evaluate(state, &live.probe_readings));

    for r in &live.routes {
        if !r.answered {
            out.push(Finding {
                severity: Severity::Broken,
                subject: r.file.clone(),
                what: format!("routes to {}, where nothing answers", r.target),
                remedy: "fix the address or delete the route; a dead route is only found when someone needs it".into(),
            });
        }
    }

    out
}

/// G3: the growth half of the check, split out so it can be tested on its
/// own and reused by anything that has the facts.
///
/// Every finding here is Drift rather than Broken except a nearly-full disk,
/// because that is the honest reading: nothing is failing yet. The point is
/// to see it while it is still cheap.
/// F184: `host_short` is why the host itself cannot help — Some when the
/// machine is oversubscribed or already swapping hard. Passed in rather than
/// read here, because a per-guest judgement that silently reads global state
/// is a judgement nobody can test.
pub fn evaluate_growth(
    facts: &[GrowthFact],
    lim: GrowthLimits,
    host_short: Option<&str>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for g in facts {
        let who = format!("{} (CT {})", g.hostname, g.vmid);
        if g.disk_used_pct >= lim.disk_broken_pct {
            out.push(Finding {
                severity: Severity::Broken,
                subject: who.clone(),
                what: format!("rootfs is {}% full", g.disk_used_pct),
                remedy: "an image pull or an apt upgrade can fail halfway from here — free space or grow the disk with `pct resize`".into(),
            });
        } else if g.disk_used_pct >= lim.disk_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("rootfs is {}% full", g.disk_used_pct),
                remedy: "still fine, but find out what is growing before it is urgent — `du -xh --max-depth=2 /` inside the container".into(),
            });
        }
        if g.mem_used_pct >= lim.mem_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("using {}% of its memory allocation", g.mem_used_pct),
                remedy: "raise memory_mb in the stack manifest and redeploy; the alternative is swapping, which hides the pressure instead of reporting it".into(),
            });
        }
        if g.swap_used_mb >= lim.swap_drift_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("{} MB of swap in use", g.swap_used_mb),
                // F184: the old remedy said "give the container more memory"
                // unconditionally. On 2026-09-02 eight containers reported
                // this at once on a host with 31 GB of RAM, 47 GB committed
                // to guests, and 7 of its 8 GB of swap already used. Following
                // that advice eight times would have made the machine worse,
                // and the check would have kept saying it. A remedy that
                // cannot be carried out is not a remedy.
                remedy: host_short
                    .map(|why| {
                        format!(
                            "the container is not the problem: {} — reduce what is promised \
                             to guests, or give the host more RAM. Raising this container's \
                             memory takes it from another one",
                            why
                        )
                    })
                    .unwrap_or_else(|| {
                        "swap turns memory pressure into slow degradation instead of a loud \
                         failure — give the container more memory rather than more swap"
                            .into()
                    }),
            });
        }
        if g.journal_mb >= lim.journal_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("journal is {} MB", g.journal_mb),
                remedy: format!("the guards cap it well below this — run `homelab guards {}` and check the cap took effect", g.vmid),
            });
        }
        if g.docker_logs_mb >= lim.docker_logs_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("docker container logs total {} MB", g.docker_logs_mb),
                remedy: format!("the cap only applies to containers created after it was set — run `homelab guards {}`, then recreate the noisiest container so it picks the cap up", g.vmid),
            });
        }
        if !g.guards {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who,
                what: "has no runaway guards: no journald cap, no docker log cap, or both".into(),
                remedy: format!("`homelab guards {}` — without them nothing bounds log growth, which is how one service reached 923 MB unnoticed", g.vmid),
            });
        }
    }
    out
}

/// Is each stack's safety net attached? See `CoverageFact` for why this
/// exists at all.
/// O1 (Kenny, 2026-09-02): a backup this orchestrator does not MAKE but does
/// WATCH.
///
/// OPNsense is on the no-touch list and backs itself up: a plugin uploads an
/// encrypted copy of the router's configuration to Google Drive every night.
/// Nothing in this suite makes that backup and nothing should. But a backup
/// nobody watches is a backup you discover is broken on the day you need it,
/// so the nightly round that already asks "when was this last backed up" of
/// every stack asks it of that folder too.
///
/// Deliberately not a Kuma push monitor: that needs a script on the host
/// calling in on a timer, and a separately maintained file on a machine is
/// the thing Kenny ruled out on 2026-09-02. This reuses the round that
/// already runs, the finding shape that already exists, and the notification
/// path that already reaches him.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchedBackupFact {
    /// What to call it in the report.
    pub name: String,
    /// Age of the NEWEST file found, in seconds. None = nothing was found,
    /// which is not the same as "old" and is reported differently.
    pub newest_age_s: Option<u64>,
    /// Older than this and it is a finding.
    pub max_age_s: u64,
    /// The listing itself failed — no answer is not a healthy answer.
    pub error: Option<String>,
}

/// Broken rather than Drift throughout: unlike a missing metric, a backup
/// that stopped is not a slower kind of wrong. It is the thing itself.
pub fn evaluate_watched_backups(facts: &[WatchedBackupFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in facts {
        if let Some(e) = &f.error {
            out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: format!("could not be listed ({})", e),
                remedy: "check the rclone remote and the path in host.toml — a listing that \
                         does not answer says nothing about the backup either way"
                    .into(),
            });
            continue;
        }
        match f.newest_age_s {
            None => out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: "holds no files at all".into(),
                remedy: "the device that writes here has never succeeded, or writes somewhere \
                         else than host.toml says"
                    .into(),
            }),
            Some(age) if age > f.max_age_s => out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: format!(
                    "newest file is {} hours old, expected one within {}",
                    age / 3600,
                    f.max_age_s / 3600
                ),
                remedy: "the device stopped uploading — check its backup settings and its \
                         credentials before the next config change is the one you need back"
                    .into(),
            }),
            Some(_) => {}
        }
    }
    out
}

/// G8: a stack whose last deploy stopped halfway, reported out loud.
///
/// `incomplete_step` has been written since S2 and read by nobody. That is
/// the exact shape this whole audit keeps finding: a mechanism that runs,
/// records the truth, and is wired to nothing. The record was added because
/// the media stack failed at "start apps" and therefore did not exist as far
/// as the orchestrator was concerned — 12 GB of configuration with no nightly
/// backup, and nothing anywhere saying so. Writing the field fixed the
/// backup; it did not make anyone aware. This does.
///
/// Severity is Broken rather than Drift on purpose: a half-applied stack is
/// not a container that will bite later, it is one whose running state nobody
/// has claimed. The remedy names the step, because "the deploy failed" a week
/// ago is not something Kenny can act on and "it stopped at start apps" is.
/// G16: is the path by which Kenny learns anything still working?
///
/// The circularity is the point and cannot be engineered away: if every
/// notification route is down, this finding cannot reach him by notification
/// either. What it can do is be there in `homelab check` and the TUI, so the
/// question "why has it been so quiet" has an answer other than a guess.
pub fn evaluate_notify(state: &HostState, now: u64) -> Vec<Finding> {
    if state.last_notify_failed <= state.last_notify_ok {
        return Vec::new();
    }
    let ago = now.saturating_sub(state.last_notify_failed) / 60;
    let last_ok = if state.last_notify_ok == 0 {
        "and none has ever arrived".to_string()
    } else {
        format!(
            "the last one that arrived was {} h earlier",
            state
                .last_notify_failed
                .saturating_sub(state.last_notify_ok)
                / 3600
        )
    };
    vec![Finding {
        severity: Severity::Broken,
        subject: "notifications".into(),
        what: format!(
            "no route accepted the last notification ({} min ago{}){}",
            ago,
            state
                .last_notify_error
                .as_ref()
                .map(|e| format!(": {}", e))
                .unwrap_or_default(),
            format_args!(" — {}", last_ok)
        ),
        remedy: "check the route in `notify_url` and `notify_fallback_webhook` in host.toml — \
                 while this stands, every warning this host produces is going nowhere, \
                 including this one"
            .into(),
    }]
}

pub fn evaluate_incomplete(state: &HostState) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(step) = st.incomplete_step.as_ref() else {
            continue;
        };
        out.push(Finding {
            severity: Severity::Broken,
            subject: name.clone(),
            what: format!("its last deploy stopped at \"{}\" and never finished", step),
            remedy: "run the deploy again and watch that step — until it completes, what runs \
                     on the container and what the orchestrator has on record are two different \
                     things, and only the record drives drift detection and retention"
                .into(),
        });
    }
    out.sort_by(|a, b| a.subject.cmp(&b.subject));
    out
}

///
/// Drift rather than Broken throughout: nothing is failing: the stack runs
/// fine. What is missing is the ability to find out when it stops, which is a
/// slower and more expensive kind of wrong.
pub fn evaluate_coverage(facts: &[CoverageFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in facts {
        if c.unmeasured_by_choice {
            out.push(Finding {
                severity: Severity::Noted,
                subject: c.stack.clone(),
                what:
                    "deliberately not measured by Prometheus (`metrics: false` in its service.yml)"
                        .into(),
                remedy: "nothing, unless that decision changes: remove the line and add a target"
                    .into(),
            });
        }
        if c.scraped == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no Prometheus target answers for this stack — it is not being measured".into(),
                remedy: "check /appdata/metrics/prometheus-config/targets/<stack>.json exists and that Prometheus reads that directory".into(),
            });
        }
        if c.logs_recent == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no LABELLED log line reached Loki from this stack recently — either nothing is shipping, or it ships without the container name (F79)".into(),
                remedy: "on that container: `systemctl status alloy`, then `runuser -u alloy -- ls /var/lib/docker/containers` (a compose stack) or `journalctl -u alloy` (a native one); a redeploy re-applies Alloy's config and its read access".into(),
            });
        }
    }
    out
}

/// The address a route forwards to, as `host/port` for a `/dev/tcp` probe.
///
/// Extracted from the shell so it can be tested: the first live run reported
/// every route in the house dead, and one of the two reasons was here —
/// `https://10.10.5.1` carries no port, so probing it as written asks for a
/// path rather than a socket. The other reason was the shell itself (dash has
/// no /dev/tcp), which no amount of testing this function would have caught.
/// Both were invisible until the check ran against the real fleet once.
pub fn probe_hostport(target: &str) -> String {
    let (scheme, rest) = match target.split_once("://") {
        Some((s, r)) => (s, r),
        None => ("", target),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.rsplit_once(':') {
        // An IPv6 literal has colons of its own; only a numeric tail is a port.
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            format!("{}/{}", host, port)
        }
        _ => format!("{}/{}", authority, if scheme == "https" { 443 } else { 80 }),
    }
}

/// W3: report a container whose boot policy or resources no longer match the
/// stack file. Boot policy is a drift a deploy repairs on its own; memory and
/// cores are named with the remedy that actually applies, because raising
/// them is a deliberate operation and lowering them is a rebuild.
///
/// A stack whose state record carries no manifest is skipped rather than
/// guessed at — the same rule as everywhere else here: a question that was
/// not asked never becomes a finding.
pub fn evaluate_boot(state: &HostState, facts: &[BootFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(fact) = facts.iter().find(|f| f.vmid == st.vmid) else {
            continue;
        };
        for d in crate::ops::reconcile::divergences(m, &fact.live) {
            let boot_related = d.starts_with("starts on boot") || d.starts_with("boot order");
            out.push(Finding {
                severity: Severity::Drift,
                subject: name.clone(),
                what: d,
                remedy: if boot_related {
                    "a deploy puts the boot policy back; until then a reboot starts the fleet in the wrong order".into()
                } else {
                    "raise it with `homelab resize`, or lower it by rebuilding the container — a deploy deliberately does not change resources under a running service".into()
                },
            });
        }
    }
    out
}
