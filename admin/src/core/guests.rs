//! feat-overview-2: the guests on the host, from the host's `status` reply.
//!
//! fix-371-1 (measured 2026-10-04 through the client): since fix-68 the
//! host's `status` answers its fleet snapshot as JSON — every guest Proxmox
//! reports (`host.guests_usage`, managed or not) and every stack — with no
//! `pct list` text. The guests are read from that structure: a guest runs
//! when it has been up for a second or more, and a stack's guest carries the
//! stack's hostname. The older text (`pct list:` and then the host's managed
//! state) is still read when a host answers it; its managed state never
//! reaches the browser.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Guest {
    pub vmid: u32,
    /// `running`, `stopped`, … as `pct` says it.
    pub status: String,
    /// `backup`, `migrate`, …; empty when there is no lock.
    pub lock: String,
    pub name: String,
}

/// The guests in a `status` reply: the fleet snapshot's, or the `pct list`
/// table of an older host.
pub fn parse_status(message: &str) -> Vec<Guest> {
    match serde_json::from_str::<serde_json::Value>(message) {
        Ok(v) if v.get("host").is_some() => from_fleet(&v),
        _ => from_pct_list(message),
    }
}

/// The guests of a fleet snapshot (`homelab_proto::FleetState` as JSON):
/// every guest the host measured, else (a host that sends no per-guest
/// reading) every stack's.
fn from_fleet(v: &serde_json::Value) -> Vec<Guest> {
    let num = |x: &serde_json::Value, k: &str| x.get(k).and_then(|n| n.as_u64());
    let stacks: Vec<&serde_json::Value> = v
        .get("stacks")
        .and_then(|s| s.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let hostname = |vmid: u64| {
        stacks
            .iter()
            .find(|s| num(s, "vmid") == Some(vmid))
            .and_then(|s| s.get("hostname").and_then(|h| h.as_str()))
            .unwrap_or("")
            .to_string()
    };
    let guest = |vmid: u64, running: bool| Guest {
        vmid: vmid as u32,
        status: if running { "running" } else { "stopped" }.to_string(),
        lock: String::new(),
        name: hostname(vmid),
    };
    let mut out: Vec<Guest> = match v.pointer("/host/guests_usage").and_then(|g| g.as_array()) {
        Some(all) => all
            .iter()
            .filter_map(|g| Some(guest(num(g, "vmid")?, num(g, "uptime_s").unwrap_or(0) > 0)))
            .collect(),
        None => stacks
            .iter()
            .filter_map(|s| {
                let online = s.get("online").and_then(|o| o.as_bool()).unwrap_or(false);
                Some(guest(num(s, "vmid")?, online))
            })
            .collect(),
    };
    out.sort_by_key(|g| g.vmid);
    out
}

/// The `pct list` table inside an older host's `status` reply. A line that
/// does not read as a guest (the header, the managed state after it) is
/// skipped.
fn from_pct_list(message: &str) -> Vec<Guest> {
    let listing = message
        .split_once("pct list:")
        .map(|(_, rest)| rest)
        .unwrap_or(message);
    let listing = listing
        .split_once("managed state:")
        .map(|(l, _)| l)
        .unwrap_or(listing);
    let mut out = Vec::new();
    for line in listing.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(vmid) = fields.first().and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let (status, lock, name) = match fields.len() {
            3 => (fields[1], "", fields[2]),
            n if n >= 4 => (fields[1], fields[2], fields[3]),
            _ => continue,
        };
        out.push(Guest {
            vmid,
            status: status.to_string(),
            lock: lock.to_string(),
            name: name.to_string(),
        });
    }
    out.sort_by_key(|g| g.vmid);
    out
}
