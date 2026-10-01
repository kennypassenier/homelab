//! fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): the Cloudflare
//! edge against its capture in `captured/gateway/`.
//!
//! The capture (R5, 2026-09-20) was read once and never again, and
//! CLOUDFLARE.md had already drifted from it (`trmnl.kp-soft.dev`). A
//! dashboard click, or a compromised Cloudflare session, that flips the
//! wildcard Access app to bypass makes the house public, and nothing said so.
//!
//! Pure: the caller fetches the API's answers (read-only token); this
//! projects them onto the capture's own shape, e-mail addresses redacted the
//! same way, and compares. What moves without anyone changing the edge
//! (tunnel health, ids, versions, record order) is not projected.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::ops::fleetcheck::{Finding, Severity};

/// fix-143: the Cloudflare API base, shared by the client (its own token,
/// `~/.config/cloudflare/kp-soft.token`) and the host's nightly run (its own
/// `cloudflare_token` in host.toml, owner decision 2026-10-01).
pub const API: &str = "https://api.cloudflare.com/client/v4";

/// The edge as the capture holds it and as the API reads, projected alike.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeState {
    /// `[{name, id, config}]`.
    pub tunnels: Value,
    /// `[{name, domain, self_hosted_domains, type, session_duration, policies}]`.
    pub apps: Value,
    /// `[{type, name, content, proxied}]`, sorted.
    pub dns: Value,
}

/// Where the API is asked, read from the capture files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeIds {
    pub account_id: String,
    pub zone_id: String,
    pub tunnel_id: String,
}

/// Every string that looks like an e-mail address becomes `<email>`, as in
/// the committed capture. An added or removed address still changes the
/// count; a replaced one does not, which is the price of not publishing them.
pub fn redact_emails(v: &Value) -> Value {
    match v {
        Value::String(s) if s.contains('@') && !s.contains(' ') => json!("<email>"),
        Value::Array(a) => Value::Array(a.iter().map(redact_emails).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), redact_emails(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut m = Map::new();
    for k in keys {
        m.insert((*k).to_string(), v.get(*k).cloned().unwrap_or(Value::Null));
    }
    Value::Object(m)
}

/// `cfd_tunnel/<id>` and `cfd_tunnel/<id>/configurations`, or a capture
/// entry (which holds `config` itself): name, id and configuration. Health
/// is left out; the external monitors watch that.
pub fn project_tunnel(tunnel: &Value, config: &Value) -> Value {
    let cfg = config
        .get("config")
        .cloned()
        .unwrap_or_else(|| config.clone());
    json!({
        "name": tunnel.get("name").cloned().unwrap_or(Value::Null),
        "id": tunnel.get("id").cloned().unwrap_or(Value::Null),
        "config": cfg,
    })
}

/// `access/apps`: the fields the capture keeps, each policy's decision and
/// rules, addresses redacted.
pub fn project_apps(apps: &Value) -> Value {
    let list = apps.as_array().cloned().unwrap_or_default();
    Value::Array(
        list.iter()
            .map(|a| {
                let mut p = pick(
                    a,
                    &[
                        "name",
                        "domain",
                        "self_hosted_domains",
                        "type",
                        "session_duration",
                    ],
                );
                let policies: Vec<Value> = a
                    .get("policies")
                    .and_then(Value::as_array)
                    .map(|ps| {
                        ps.iter()
                            .map(|x| {
                                pick(x, &["name", "decision", "include", "require", "exclude"])
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                p["policies"] = Value::Array(policies);
                redact_emails(&p)
            })
            .collect(),
    )
}

/// `dns_records`: type, name, target and whether Cloudflare proxies it,
/// sorted so the API's order does not count.
pub fn project_dns(records: &Value) -> Value {
    let mut list: Vec<Value> = records
        .as_array()
        .map(|rs| {
            rs.iter()
                .map(|r| pick(r, &["type", "name", "content", "proxied"]))
                .collect()
        })
        .unwrap_or_default();
    list.sort_by_key(|r| r.to_string());
    Value::Array(list)
}

fn read_json(dir: &Path, name: &str) -> Result<Value, String> {
    let p = dir.join(name);
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {}", p.display(), e))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {}", p.display(), e))
}

fn str_field(v: &Value, key: &str, file: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{}: no `{}`", file, key))
}

/// The committed capture: `cloudflare-tunnel.json`, `cloudflare-access.json`
/// and `cloudflare-dns.json`, projected like the live answers, with the ids
/// the API is asked with.
pub fn load_capture(dir: &Path) -> Result<(EdgeIds, EdgeState), String> {
    let tunnel = read_json(dir, "cloudflare-tunnel.json")?;
    let access = read_json(dir, "cloudflare-access.json")?;
    let dns = read_json(dir, "cloudflare-dns.json")?;
    let tunnels: Vec<Value> = tunnel
        .get("tunnels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tunnel_id = tunnels
        .first()
        .and_then(|t| t.get("id"))
        .and_then(Value::as_str)
        .ok_or("cloudflare-tunnel.json: no tunnel id")?
        .to_string();
    let ids = EdgeIds {
        account_id: str_field(&tunnel, "account_id", "cloudflare-tunnel.json")?,
        zone_id: str_field(&dns, "zone_id", "cloudflare-dns.json")?,
        tunnel_id,
    };
    let state = EdgeState {
        tunnels: Value::Array(tunnels.iter().map(|t| project_tunnel(t, t)).collect()),
        apps: project_apps(access.get("apps").unwrap_or(&Value::Null)),
        dns: project_dns(dns.get("records").unwrap_or(&Value::Null)),
    };
    Ok((ids, state))
}

fn by_key(v: &Value, key: impl Fn(&Value) -> String) -> BTreeMap<String, Value> {
    v.as_array()
        .map(|a| a.iter().map(|x| (key(x), x.clone())).collect())
        .unwrap_or_default()
}

fn name_of(v: &Value) -> String {
    v.get("name")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string()
}

/// Does this app let anyone in: a bypass decision, or an `everyone` rule
/// in an allowing policy?
fn opens_to_everyone(app: &Value) -> bool {
    app.get("policies")
        .and_then(Value::as_array)
        .is_some_and(|ps| {
            ps.iter().any(|p| {
                let decision = p.get("decision").and_then(Value::as_str).unwrap_or("");
                let everyone = p
                    .get("include")
                    .and_then(Value::as_array)
                    .is_some_and(|i| i.iter().any(|r| r.get("everyone").is_some()));
                decision == "bypass" || (decision == "allow" && everyone)
            })
        })
}

const REMEDY: &str = "if the change was meant, capture it again into captured/gateway/ (and \
                      CLOUDFLARE.md); if not, undo it in the Cloudflare dashboard and find out \
                      who made it";

/// Every difference between the capture and the live edge. Drift, except an
/// Access app that now lets everyone in where the capture did not: Broken,
/// because that is the house open to the internet.
pub fn compare_edge(captured: &EdgeState, live: &EdgeState) -> Vec<Finding> {
    let mut out = Vec::new();
    let drift = |subject: String, what: String| Finding {
        severity: Severity::Drift,
        subject,
        what,
        remedy: REMEDY.into(),
    };

    let cap_t = by_key(&captured.tunnels, name_of);
    let live_t = by_key(&live.tunnels, name_of);
    for (name, t) in &cap_t {
        match live_t.get(name) {
            None => out.push(drift(
                format!("tunnel {}", name),
                format!("the tunnel {} is gone from Cloudflare", name),
            )),
            Some(l) if l != t => out.push(drift(
                format!("tunnel {}", name),
                format!(
                    "the tunnel {}'s configuration (ingress, routing) differs from the capture",
                    name
                ),
            )),
            Some(_) => {}
        }
    }
    for name in live_t.keys().filter(|n| !cap_t.contains_key(*n)) {
        out.push(drift(
            format!("tunnel {}", name),
            format!("a tunnel {} exists that the capture does not have", name),
        ));
    }

    let cap_a = by_key(&captured.apps, name_of);
    let live_a = by_key(&live.apps, name_of);
    for (name, l) in &live_a {
        let domain = l.get("domain").and_then(Value::as_str).unwrap_or("?");
        let was = cap_a.get(name);
        if was == Some(l) {
            continue;
        }
        let newly_open = opens_to_everyone(l) && !was.is_some_and(opens_to_everyone);
        if newly_open {
            out.push(Finding {
                severity: Severity::Broken,
                subject: format!("access {}", name),
                what: format!(
                    "the Access app {} ({}) now lets everyone in, which the capture does not \
                     — whatever it covers is public",
                    name, domain
                ),
                remedy: format!(
                    "undo it in the Cloudflare dashboard now unless it was meant; {}",
                    REMEDY
                ),
            });
        } else if was.is_none() {
            out.push(drift(
                format!("access {}", name),
                format!(
                    "an Access app {} ({}) exists that the capture does not have",
                    name, domain
                ),
            ));
        } else {
            out.push(drift(
                format!("access {}", name),
                format!(
                    "the Access app {} ({}) differs from the capture: domains, session or \
                     policies",
                    name, domain
                ),
            ));
        }
    }
    for (name, c) in cap_a.iter().filter(|(n, _)| !live_a.contains_key(*n)) {
        out.push(drift(
            format!("access {}", name),
            format!(
                "the Access app {} ({}) is gone from Cloudflare — what it protected falls to \
                 the next app, or to none",
                name,
                c.get("domain").and_then(Value::as_str).unwrap_or("?")
            ),
        ));
    }

    let rec = |r: &Value| {
        format!(
            "{} {} -> {}{}",
            r.get("type").and_then(Value::as_str).unwrap_or("?"),
            r.get("name").and_then(Value::as_str).unwrap_or("?"),
            r.get("content").and_then(Value::as_str).unwrap_or("?"),
            if r.get("proxied") == Some(&Value::Bool(true)) {
                " (proxied)"
            } else {
                ""
            }
        )
    };
    let cap_d: Vec<String> = captured
        .dns
        .as_array()
        .map(|a| a.iter().map(rec).collect())
        .unwrap_or_default();
    let live_d: Vec<String> = live
        .dns
        .as_array()
        .map(|a| a.iter().map(rec).collect())
        .unwrap_or_default();
    let added: Vec<&String> = live_d.iter().filter(|r| !cap_d.contains(r)).collect();
    let gone: Vec<&String> = cap_d.iter().filter(|r| !live_d.contains(r)).collect();
    if !added.is_empty() || !gone.is_empty() {
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!(
                "new: {}",
                added
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !gone.is_empty() {
            parts.push(format!(
                "gone: {}",
                gone.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        out.push(drift(
            "dns records".into(),
            format!(
                "the DNS records differ from the capture — {}",
                parts.join(" · ")
            ),
        ));
    }
    out
}
