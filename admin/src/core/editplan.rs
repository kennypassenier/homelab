//! feat-stacks-2: the plan shown before a commit: the file diff, and what
//! homelab would change on the machines when the commit is deployed, read
//! from the same homelab-core functions the deploy runs (the firewall's
//! rendered file, the boot reconcile, the resize rule). Pure.

use homelab_core::firewall;
use homelab_proto::StackManifest;
use serde::Serialize;

use super::actions::SELF_STACK;
use super::stackedit::FileChange;
use super::textdiff::{self, Hunk};

/// One consequence on the machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Effect {
    /// `info` or `warning`.
    pub tone: &'static str,
    pub what: String,
    /// Lines under it (the rendered firewall lines that change, …).
    pub detail: Vec<String>,
    /// The action that carries it out: `deploy` or `resize`; None when
    /// nothing is needed or nothing can.
    pub by: Option<&'static str>,
}

fn effect(tone: &'static str, what: impl Into<String>, by: Option<&'static str>) -> Effect {
    Effect {
        tone,
        what: what.into(),
        detail: Vec::new(),
        by,
    }
}

/// A file's part of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub path: String,
    /// `added`, `changed` or `removed`.
    pub status: &'static str,
    pub added: usize,
    pub removed: usize,
    pub hunks: Vec<Hunk>,
}

pub fn file_diffs(changes: &[FileChange]) -> Vec<FileDiff> {
    changes
        .iter()
        .map(|c| {
            let (old, new) = (
                c.old.as_deref().unwrap_or(""),
                c.new.as_deref().unwrap_or(""),
            );
            let (added, removed) = textdiff::counts(old, new);
            FileDiff {
                path: c.path.clone(),
                status: match (&c.old, &c.new) {
                    (None, _) => "added",
                    (_, None) => "removed",
                    _ => "changed",
                },
                added,
                removed,
                hunks: textdiff::hunks(old, new, 3),
            }
        })
        .collect()
}

fn mb(v: u32) -> String {
    if v >= 1024 && v.is_multiple_of(1024) {
        format!("{} GB", v / 1024)
    } else if v >= 1024 {
        format!("{:.1} GB", f64::from(v) / 1024.0)
    } else {
        format!("{v} MB")
    }
}

/// What the machines would see of the change from `old` to `new` (None:
/// a new stack). `other_files`: stack files other than the stack file that
/// change (compose files, routes). `natives`: the stack runs native units.
pub fn effects(
    old: Option<&StackManifest>,
    new: &StackManifest,
    other_files: &[String],
    // tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    // fleet-wide `tile_watch_source` (host.toml), so the same derived rule
    // the deploy writes shows in the plan the owner reviews before a
    // commit. None here (the caller has not yet fetched it from the host)
    // means the plan shows only the hand-declared rules, same as before
    // this decision — never wrong, only silent about the derived one.
    tile_watch_source: Option<&str>,
) -> Vec<Effect> {
    let mut out = Vec::new();
    let Some(old) = old else {
        out.push(effect(
            "info",
            format!(
                "the first deploy creates CT {} ({}) at {}",
                new.vmid, new.hostname, new.network.ip
            ),
            Some("deploy"),
        ));
        return out;
    };
    let (o, n) = (&old.resources, &new.resources);
    let mut raised = Vec::new();
    let mut lowered = Vec::new();
    for (what, a, b, show) in [
        (
            "cores",
            o.cores as u32,
            n.cores as u32,
            None::<fn(u32) -> String>,
        ),
        (
            "memory",
            o.memory_mb,
            n.memory_mb,
            Some(mb as fn(u32) -> String),
        ),
        ("swap", o.swap_mb, n.swap_mb, Some(mb as fn(u32) -> String)),
        (
            "disk",
            o.disk_gb,
            n.disk_gb,
            Some((|v| format!("{v} GB")) as fn(u32) -> String),
        ),
    ] {
        let fmt = |v| show.map(|f| f(v)).unwrap_or_else(|| v.to_string());
        if b > a {
            raised.push(format!("{what} {} → {}", fmt(a), fmt(b)));
        } else if b < a {
            lowered.push((what, format!("{what} {} → {}", fmt(a), fmt(b))));
        }
    }
    if !raised.is_empty() {
        let mut e = effect(
            "info",
            "Resize applies the raised resources to the running container; a deploy leaves resources alone",
            Some("resize"),
        );
        e.detail = raised;
        out.push(e);
    }
    for (what, line) in lowered {
        out.push(effect(
            "warning",
            if what == "disk" {
                format!("{line}: Proxmox cannot shrink a disk; only a rebuild of the container gives it the smaller one, and the fleet check reports the difference until then")
            } else {
                format!("{line}: lowering takes a rebuild of the container; the fleet check reports the difference until then")
            },
            None,
        ));
    }
    if old.boot.onboot != new.boot.onboot || old.boot.order != new.boot.order {
        out.push(effect(
            "info",
            format!(
                "the deploy puts the boot policy back to start on boot {} with order {} (W3)",
                if new.boot.onboot { "on" } else { "off" },
                new.boot
                    .order
                    .map(|o| o.to_string())
                    .unwrap_or_else(|| "unset".into())
            ),
            Some("deploy"),
        ));
    }
    if old.lxc.protection != new.lxc.protection {
        out.push(effect(
            if new.lxc.protection {
                "info"
            } else {
                "warning"
            },
            format!(
                "Proxmox protection {} for CT {}: only a rebuild applies it",
                if new.lxc.protection { "on" } else { "off" },
                new.vmid
            ),
            None,
        ));
    }
    // feat-stacks-9: O5/H4 — `unprivileged`, `gpu` and `vpn` are only ever
    // set at `pct create`/`pct clone`; a deploy of a running container
    // leaves them exactly as they are, same shape as protection above.
    for (label, was, is) in [
        (
            "privilege level",
            old.lxc.unprivileged,
            new.lxc.unprivileged,
        ),
        ("gpu passthrough", old.lxc.gpu, new.lxc.gpu),
        ("vpn device", old.lxc.vpn, new.lxc.vpn),
    ] {
        if was != is {
            out.push(effect(
                "warning",
                format!(
                    "{label} {}: only a rebuild of CT {} applies it; a deploy leaves the \
                     running container as it is",
                    if is { "on" } else { "off" },
                    new.vmid
                ),
                None,
            ));
        }
    }
    if old.resources.storage != new.resources.storage {
        out.push(effect(
            "warning",
            format!(
                "storage {} → {}: only a rebuild of CT {} moves the disk; a deploy leaves it where it is",
                old.resources.storage, new.resources.storage, new.vmid
            ),
            None,
        ));
    }
    if old.network.ip != new.network.ip
        || old.network.gateway != new.network.gateway
        || old.network.bridge != new.network.bridge
        || old.network.vlan != new.network.vlan
    {
        out.push(effect(
            "info",
            format!(
                "the deploy writes the new network config to CT {}'s config; the container picks it up at its next start",
                new.vmid
            ),
            Some("deploy"),
        ));
    }
    // tile-watch: a tile arriving or leaving can change the derived rule
    // even when the hand-declared `firewall:` block did not move.
    if old.firewall != new.firewall || old.tiles != new.tiles {
        out.extend(firewall_effects(old, new, tile_watch_source));
    }
    if old.apps != new.apps {
        let added: Vec<&String> = new.apps.iter().filter(|a| !old.apps.contains(a)).collect();
        if !added.is_empty() {
            out.push(effect(
                "info",
                format!(
                    "the deploy starts the new app{} {}",
                    if added.len() == 1 { "" } else { "s" },
                    added
                        .iter()
                        .map(|a| a.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Some("deploy"),
            ));
        }
    }
    let paths = |m: &StackManifest| {
        m.storage
            .iter()
            .map(|s| s.host_path.clone())
            .collect::<Vec<_>>()
    };
    if paths(old) != paths(new) {
        out.push(effect(
            "info",
            "the deploy creates the new /appdata directories and mounts them; a running container sees a new mount after its next start",
            Some("deploy"),
        ));
    }
    if !other_files.is_empty() {
        let mut e = effect(
            "info",
            "the deploy sends the changed files into the container; docker compose recreates the services whose files changed",
            Some("deploy"),
        );
        e.detail = other_files.to_vec();
        out.push(e);
    }
    if !new.natives.is_empty() {
        out.push(effect(
            "info",
            "native programs stay as they are: a deploy never replaces an installed binary (fix-28); a newer release is its own action",
            None,
        ));
    }
    if new.stack_name == SELF_STACK && out.iter().any(|e| e.by == Some("deploy")) {
        out.push(effect(
            "warning",
            "deploying admin restarts this dashboard; the page reconnects after it (arch-self)",
            None,
        ));
    }
    out
}

fn firewall_effects(
    old: &StackManifest,
    new: &StackManifest,
    tile_watch_source: Option<&str>,
) -> Vec<Effect> {
    let path = firewall::fw_path(new.vmid);
    let render = |m: &StackManifest| {
        m.firewall.as_ref().filter(|f| f.enabled).map(|f| {
            // tile-watch: the same derived rule the deploy writes, so the
            // owner sees it here rather than being surprised by it on pve.
            let effective = firewall::with_tile_watch(
                f,
                &m.network.ip,
                &m.tiles,
                tile_watch_source.unwrap_or(""),
            );
            firewall::render(&m.stack_name, &effective)
        })
    };
    match (render(old), render(new)) {
        (_, None) if new.firewall.is_some() => vec![effect(
            "warning",
            format!("declared, not enabled: the deploy leaves {path} as it is"),
            None,
        )],
        (Some(_), None) => vec![effect(
            "warning",
            format!("the declaration is gone: the deploy no longer writes {path}, and the fleet check reports the file it finds there"),
            None,
        )],
        (a, Some(b)) => {
            let (added, removed) = firewall::line_changes(a.as_deref().unwrap_or(""), &b);
            let mut e = effect(
                "info",
                if a.is_none() {
                    format!("the deploy writes {path} and switches firewall=1 on the container's network card: the rules apply from then on")
                } else {
                    format!("the deploy rewrites {path}: +{} −{} line(s)", added.len(), removed.len())
                },
                Some("deploy"),
            );
            e.detail = added
                .iter()
                .map(|l| format!("+ {l}"))
                .chain(removed.iter().map(|l| format!("- {l}")))
                .collect();
            let mut out = vec![e];
            if let Some(fw) = new.firewall.as_ref() {
                if fw.policy_in == homelab_core::manifest::FwAction::Drop
                    && !fw.rules.iter().any(|r| r.dir == homelab_core::manifest::FwDir::In)
                {
                    out.push(effect(
                        "warning",
                        "inbound policy DROP and no inbound rule: nothing can reach this container, Uptime Kuma and Traefik included",
                        None,
                    ));
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// The follow-up actions the plan offers after the commit, in order.
pub fn follow_ups(effects: &[Effect]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for by in ["deploy", "resize"] {
        if effects.iter().any(|e| e.by == Some(by)) {
            out.push(by);
        }
    }
    out
}

/// The commit's first line: `stacks/<stack>: <what> [<feature>]`.
pub fn commit_subject(stack: &str, summary: &str, feature: &str) -> String {
    let summary = summary.trim();
    let summary: String = summary.chars().take(100).collect();
    format!("stacks/{stack}: {summary} [{feature}]")
}

/// A summary of the change for the commit subject.
pub fn summary(kind: &str, diffs: &[FileDiff], effects: &[Effect]) -> String {
    let files = diffs.len();
    match kind {
        "firewall" => {
            let lines: usize = diffs.iter().map(|d| d.added + d.removed).sum();
            format!("firewall edited in the dashboard ({lines} line(s))")
        }
        "settings" => {
            let parts: Vec<String> = effects
                .iter()
                .flat_map(|e| e.detail.iter().cloned())
                .take(3)
                .collect();
            if parts.is_empty() {
                format!("settings edited in the dashboard ({files} file(s))")
            } else {
                format!("settings: {}", parts.join(", "))
            }
        }
        "add_app" => "app added in the dashboard".into(),
        _ => match diffs.first() {
            Some(d) if files == 1 => format!(
                "{} edited in the dashboard",
                d.path.rsplit('/').next().unwrap_or(&d.path)
            ),
            _ => format!("{files} file(s) edited in the dashboard"),
        },
    }
}

/// The commit message: the subject, then what the plan said, then where it
/// was made.
pub fn commit_message(subject: &str, note: &str, effects: &[Effect], diffs: &[FileDiff]) -> String {
    let mut s = subject.trim().to_string();
    s.push_str("\n\n");
    if !note.trim().is_empty() {
        s.push_str(note.trim());
        s.push_str("\n\n");
    }
    for d in diffs {
        s.push_str(&format!(
            "{} {} (+{} -{})\n",
            d.status, d.path, d.added, d.removed
        ));
    }
    if !effects.is_empty() {
        s.push('\n');
        for e in effects {
            s.push_str(&format!("- {}\n", e.what));
        }
    }
    s.push_str("\nMade in the homelab dashboard (homelab-admin).\n");
    s
}
