//! rule-public-docs: the public-repo address gate (`.githooks/
//! check-internal-ips.sh`) refuses a commit that adds a literal 10.x.x.x,
//! 192.168.x.x or 172.16-31.x.x address to `docs/**`, `README.md`,
//! `CLAUDE.md` or `captured/**/*.md`. `homelab testplan` and `homelab
//! runbook` both write into `docs/`, and both read data (stack files, test
//! doc comments) that names real fleet addresses, so left alone they would
//! trip the gate the moment someone regenerated them.
//!
//! This module is the one place that decides what an internal address
//! becomes in generated prose: a machine's name when the address is one the
//! stack files declare (a container's own IP, its gateway, or the host
//! daemon's address), and an RFC 5737 documentation address — the same
//! range `check-internal-ips.sh` allowlists — for anything else. Both
//! generators build their map the same way and run their finished text
//! through [`redact`] before writing it, so the mapping cannot drift between
//! the two documents.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use homelab_core::manifest::StackManifest;

/// address (bare, no mask or port) -> the name it is written as.
pub type AddressMap = BTreeMap<String, String>;

/// Build the map from the stacks directory's own `lxc-compose.yml` files
/// (each container's address and its gateway) plus, when it can be found,
/// `config/client.toml`'s `host` (the Proxmox host the daemon listens on —
/// named `pve`, the one name the fleet's own docs already use for it).
/// Reads the stacks directory itself, so a stack renamed or renumbered
/// tomorrow needs no change here.
pub fn build_address_map(stacks_dir: &Path, client_host: Option<&str>) -> AddressMap {
    let mut map = AddressMap::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(stacks_dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let Ok(raw) = std::fs::read_to_string(dir.join("lxc-compose.yml")) else {
            continue;
        };
        let Ok(m) = serde_yaml::from_str::<StackManifest>(&raw) else {
            continue;
        };
        add(&mut map, &m);
    }
    if let Some(host) = client_host {
        let ip = host.split(':').next().unwrap_or("").trim();
        if !ip.is_empty() {
            map.entry(ip.to_string())
                .or_insert_with(|| "pve".to_string());
        }
    }
    map
}

/// One manifest's own address and gateway.
fn add(map: &mut AddressMap, m: &StackManifest) {
    let ip = m.network.ip.split('/').next().unwrap_or("").trim();
    if !ip.is_empty() {
        map.entry(ip.to_string())
            .or_insert_with(|| format!("CT {} ({})", m.vmid, m.stack_name));
    }
    let gw = m.network.gateway.trim();
    if !gw.is_empty() {
        map.entry(gw.to_string())
            .or_insert_with(|| "the router".to_string());
    }
}

fn is_internal_ipv4(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let mut n = [0u16; 4];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        match p.parse::<u16>() {
            Ok(v) if v <= 255 => n[i] = v,
            _ => return false,
        }
    }
    n[0] == 10 || (n[0] == 192 && n[1] == 168) || (n[0] == 172 && (16..=31).contains(&n[1]))
}

/// One matched address in `s`: its byte span (including a trailing `/NN`
/// mask or `:NNNNN` port, when either immediately follows), the bare
/// address, and the suffix text found (if any).
struct Span {
    start: usize,
    end: usize,
    ip: String,
    suffix: String,
}

/// Every internal IPv4 address in `s`, left to right, each extended over an
/// immediately following CIDR mask or port so the replacement can decide
/// what to keep.
fn find_spans(s: &str) -> Vec<Span> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        let mut groups = 0;
        while groups < 4 {
            let g_start = j;
            while j < b.len() && b[j].is_ascii_digit() && j - g_start < 3 {
                j += 1;
            }
            if j == g_start {
                break;
            }
            groups += 1;
            if groups < 4 {
                if j < b.len() && b[j] == b'.' {
                    j += 1;
                } else {
                    break;
                }
            }
        }
        if groups != 4 {
            i = start + 1;
            continue;
        }
        let ip = &s[start..j];
        let pre_ok = start == 0 || !(b[start - 1].is_ascii_digit() || b[start - 1] == b'.');
        let post_ok = j >= b.len() || !b[j].is_ascii_digit();
        if !pre_ok || !post_ok || !is_internal_ipv4(ip) {
            i = start + 1;
            continue;
        }
        // An immediately following `/<1-2 digits>` (CIDR mask) or
        // `:<1-5 digits>` (port) belongs to this address.
        let mut end = j;
        let mut suffix = String::new();
        if j < b.len() && b[j] == b'/' {
            let mut k = j + 1;
            while k < b.len() && b[k].is_ascii_digit() && k - (j + 1) < 2 {
                k += 1;
            }
            if k > j + 1 {
                suffix = s[j..k].to_string();
                end = k;
            }
        } else if j < b.len() && b[j] == b':' {
            let mut k = j + 1;
            while k < b.len() && b[k].is_ascii_digit() && k - (j + 1) < 5 {
                k += 1;
            }
            if k > j + 1 {
                suffix = s[j..k].to_string();
                end = k;
            }
        }
        out.push(Span {
            start,
            end,
            ip: ip.to_string(),
            suffix,
        });
        i = end;
    }
    out
}

/// RFC 5737 documentation addresses, in the order `check-internal-ips.sh`
/// allowlists them: 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24 — 762
/// addresses, far more than one fleet's worth.
const DOC_RANGES: [[u8; 3]; 3] = [[192, 0, 2], [198, 51, 100], [203, 0, 113]];

fn placeholder(n: usize) -> String {
    let range = n / 254;
    let host = (n % 254) as u8 + 1;
    let [a, b, c] = DOC_RANGES[range.min(2)];
    format!("{}.{}.{}.{}", a, b, c, host)
}

/// Replace every internal address in `text`: a name from `map` when its bare
/// address is in it, else a stable RFC 5737 placeholder — the same
/// placeholder every time the same address recurs in this document, assigned
/// in the order each address is first seen so a regenerated document is
/// reproducible.
pub fn redact(text: &str, map: &AddressMap) -> String {
    let spans = find_spans(text);
    if spans.is_empty() {
        return text.to_string();
    }
    let mut assigned: BTreeMap<String, String> = BTreeMap::new();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for span in spans {
        out.push_str(&text[last..span.start]);
        if let Some(name) = map.get(&span.ip) {
            // A mask says nothing once the address is a name; a port is
            // still a fact about the service, so it stays.
            out.push_str(name);
            if span.suffix.starts_with(':') {
                out.push_str(&span.suffix);
            }
        } else {
            let n = assigned.len();
            let ph = assigned
                .entry(span.ip.clone())
                .or_insert_with(|| placeholder(n));
            out.push_str(ph);
            out.push_str(&span.suffix);
        }
        last = span.end;
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> AddressMap {
        let mut m = AddressMap::new();
        m.insert("10.10.10.20".to_string(), "CT 120 (admin)".to_string());
        m.insert("10.10.10.1".to_string(), "the router".to_string());
        m.insert("10.10.10.250".to_string(), "pve".to_string());
        m
    }

    /// guards: rule-public-docs
    #[test]
    fn a_mapped_address_becomes_its_name_and_drops_the_mask() {
        let out = redact("ip `10.10.10.20/24` on vmbr0", &map());
        assert_eq!(out, "ip `CT 120 (admin)` on vmbr0");
    }

    /// guards: rule-public-docs
    #[test]
    fn a_mapped_address_keeps_its_port() {
        let out = redact("reach it at 10.10.10.250:8443 from the lan", &map());
        assert_eq!(out, "reach it at pve:8443 from the lan");
    }

    /// guards: rule-public-docs
    #[test]
    fn an_unmapped_address_gets_an_rfc5737_placeholder() {
        let out = redact("node-exporter on 10.10.10.13:9100 timed out", &map());
        assert!(out.contains(":9100"));
        assert!(!out.contains("10.10.10.13"));
        assert!(
            out.contains("192.0.2.") || out.contains("198.51.100.") || out.contains("203.0.113.")
        );
    }

    /// guards: rule-public-docs
    #[test]
    fn the_same_unmapped_address_gets_the_same_placeholder_every_time() {
        let out = redact("a at 10.10.10.6:1 and again 10.10.10.6:2", &map());
        // extract both IPs before the colon
        let ips: Vec<&str> = out
            .split_whitespace()
            .filter(|w| w.contains(':'))
            .map(|w| w.split(':').next().unwrap())
            .collect();
        assert_eq!(ips.len(), 2);
        assert_eq!(
            ips[0], ips[1],
            "the same address must map to the same placeholder"
        );
    }

    /// guards: rule-public-docs
    #[test]
    fn a_public_or_already_placeholder_address_is_untouched() {
        let out = redact("8.8.8.8 and 198.51.100.5 stay put", &map());
        assert_eq!(out, "8.8.8.8 and 198.51.100.5 stay put");
    }

    /// guards: rule-public-docs
    #[test]
    fn a_bare_network_address_is_redacted_even_with_no_host_match() {
        let out = redact("whitelists 172.16.0.0/12 on the bridge", &map());
        assert!(!out.contains("172.16.0.0"));
        assert!(out.ends_with("/12 on the bridge") || out.contains("/12"));
    }
}
