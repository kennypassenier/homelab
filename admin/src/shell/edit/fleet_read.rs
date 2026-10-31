//! feat-firewall-2: one pass over the working copy's stack manifests, shared
//! by every reader that needs the fleet's firewall declarations (the matrix,
//! the topology, the dependencies), and the host's live answer beside it.

use std::collections::BTreeMap;
use std::time::Duration;

use homelab_proto::Command;
use serde::Deserialize;

use super::EditCtx;
use super::check::bare_ip;
use crate::core::actions::Refusal;
use crate::core::fwmatrix::FleetFirewall;
use crate::core::stackedit::{self, MANIFEST};
use crate::shell::workcopy::{CommitRef, WorkingCopy};

/// The fleet's firewall declarations, the editable summary rows the
/// firewall page shows, the working copy's head commit (if any) they were
/// read at, and — fix-203 — every stack's own `natives`, every vmid some
/// stack's own `gateway_route` names, and every `10.10.x.y:port` a stack's
/// own files name (the fleet view's richer topology needs all three; the
/// firewall page itself only ever reads the first three fields).
pub(super) type FleetFirewallRead = (
    Vec<FleetFirewall>,
    Vec<serde_json::Value>,
    Option<CommitRef>,
    Vec<String>,
    Vec<u16>,
    Vec<crate::core::topology::NamedFlow>,
);

#[derive(Default)]
struct FleetRead {
    fleet: Vec<FleetFirewall>,
    summaries: Vec<serde_json::Value>,
    natives: Vec<String>,
    gateway_vmids: Vec<u16>,
    named: Vec<crate::core::topology::NamedFlow>,
}

/// Shared by every reader that needs the fleet's firewall declarations
/// (feat-firewall-2's matrix, feat-overview-7's topology, feat-stacks-9's
/// dependencies): one pass over the working copy's stack manifests.
pub(super) fn fleet_firewall(wc: &WorkingCopy) -> Result<FleetFirewallRead, Refusal> {
    if !wc.present() {
        return Err(Refusal::new(
            "the fleet's firewall declarations",
            "the dashboard has no working copy of the homelab repository yet",
            "the working copy panel says why; it clones at start once the deploy key is there",
        ));
    }
    let mut read = FleetRead::default();
    for name in wc.stack_names() {
        read_stack(wc, name, &mut read);
    }
    let head = wc.status().head;
    Ok((
        read.fleet,
        read.summaries,
        head,
        read.natives,
        read.gateway_vmids,
        read.named,
    ))
}

/// One stack's contribution; a stack whose files do not read adds nothing.
fn read_stack(wc: &WorkingCopy, name: String, read: &mut FleetRead) {
    let Ok(texts) = wc.stack_texts(&name) else {
        return;
    };
    read.named
        .extend(crate::core::topology::addresses_named_in(&name, &texts));
    let Some(raw) = texts.get(MANIFEST) else {
        return;
    };
    let Some(Ok(m)) = texts.get(MANIFEST).map(|t| stackedit::parse_manifest(t)) else {
        return;
    };
    let Ok(ip) = bare_ip(&m.network.ip).parse() else {
        return;
    };
    if !m.natives.is_empty() {
        read.natives.push(name.clone());
    }
    read.gateway_vmids.extend(gateway_vmid(raw));
    read.summaries.push(firewall_summary(&name, &m));
    read.fleet.push(FleetFirewall {
        stack: name,
        vmid: m.vmid,
        ip,
        firewall: m.firewall,
    });
}

/// `gateway_route` lives on `DeploySpec`, not `StackManifest` — the
/// stack file names a gateway's vmid to route THROUGH, which is not
/// itself part of what the container needs to run. Read generically
/// off the raw YAML, same as
/// `core/tests/firewall_declarations_tests.rs`'s own
/// `gateway_vmid` lookup.
fn gateway_vmid(raw: &str) -> Option<u16> {
    serde_yaml::from_str::<serde_yaml::Value>(raw)
        .ok()
        .and_then(|v| v.get("gateway_route")?.get("gateway_vmid")?.as_u64())
        .map(|v| v as u16)
}

fn firewall_summary(name: &str, m: &homelab_proto::StackManifest) -> serde_json::Value {
    serde_json::json!({
        "stack": name, "vmid": m.vmid, "ip": bare_ip(&m.network.ip),
        "declared": m.firewall.is_some(),
        "enabled": m.firewall.as_ref().is_some_and(|f| f.enabled),
        "policy_in": m.firewall.as_ref().map(|f| stackedit::action_word(f.policy_in)),
        "policy_out": m.firewall.as_ref().map(|f| stackedit::action_word(f.policy_out)),
        "rules": m.firewall.as_ref().map(|f| f.rules.len()).unwrap_or(0),
        "management_open": m.firewall.as_ref().and_then(|f| f.management_open.clone()),
    })
}

/// fix-207: ask the host what it actually enforces for the named stacks,
/// keyed by stack name. `None` when an older host has no arm for the
/// command, the link is down, or its answer does not parse — every one of
/// those means "the dashboard could not learn this", never "off", so every
/// caller falls back to showing the repository alone rather than guessing.
pub(super) async fn fetch_firewall_live(
    c: &EditCtx,
    fleet: &[FleetFirewall],
) -> Option<BTreeMap<String, homelab_core::ops::fleetcheck::FirewallLiveStatus>> {
    if fleet.is_empty() {
        return None;
    }
    let stack_files: Vec<(String, u16)> = fleet.iter().map(|f| (f.stack.clone(), f.vmid)).collect();
    let r = c
        .host
        .ask_traced(
            Command::GetFirewallLive { stack_files },
            Duration::from_secs(15),
            None,
        )
        .await
        .ok()?;
    if !r.ok {
        return None;
    }
    #[derive(Deserialize)]
    struct Body {
        statuses: BTreeMap<String, homelab_core::ops::fleetcheck::FirewallLiveStatus>,
    }
    serde_json::from_str::<Body>(&r.message)
        .ok()
        .map(|b| b.statuses)
}
