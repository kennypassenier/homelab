//! feat-overview-2: the guests on the host, from the host's `status` reply.
//!
//! `Status` answers text: `pct list` and then the host's managed state. Only
//! the `pct list` part is read; the managed state never reaches the browser
//! (it is the host's own bookkeeping and has no place on a page).

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

/// The `pct list` table inside a `status` reply. A line that does not read
/// as a guest (the header, the managed state after it) is skipped.
pub fn parse_status(message: &str) -> Vec<Guest> {
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
