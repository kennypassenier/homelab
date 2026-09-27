//! fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): the Cloudflare
//! edge against the capture in `captured/gateway/`.
//!
//! The capture was read once, on 2026-09-20, and nothing read it again. A
//! dashboard click, or a compromised Cloudflare session, that flips the
//! wildcard Access app to bypass would have made the house public while
//! every monitor stayed green. The API's answers are projected onto the
//! capture's own shape (e-mail addresses redacted the same way) and any
//! difference is a finding; one that opens a name to everyone is Broken.

use std::path::PathBuf;

use homelab_core::ops::edge::*;
use homelab_core::ops::fleetcheck::Severity;
use serde_json::{json, Value};

fn captured_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../captured/gateway")
}

/// The API's answers as measured read-only on 2026-09-27, with the real
/// addresses in them (the projection redacts them).
fn api_tunnel() -> (Value, Value) {
    (
        json!({"id": "b027d052-9f97-43e5-9272-eeb336d3266e", "name": "kp-soft.dev-tunnel",
               "status": "degraded", "created_at": "2025-01-01T00:00:00Z"}),
        json!({"config": {"ingress": [
                  {"service": "http://10.10.10.4:80", "hostname": "*.kp-soft.dev",
                   "originRequest": {"httpHostHeader": "", "noHappyEyeballs": true}},
                  {"service": "http_status:404"}],
               "warp-routing": {"enabled": false}}, "version": 7}),
    )
}

fn emails() -> Value {
    json!([{"email": {"email": "a@example.invalid"}},
           {"email": {"email": "b@example.invalid"}},
           {"email": {"email": "c@example.invalid"}}])
}

fn api_apps() -> Value {
    json!([
        {"id": "x1", "name": "Kobo services", "domain": "ha.kp-soft.dev",
         "self_hosted_domains": ["ha.kp-soft.dev", "trmnl.kp-soft.dev"],
         "type": "self_hosted", "session_duration": "24h", "aud": "zzz",
         "policies": [
            {"id": "p1", "name": "Toegang kpsoft", "decision": "allow", "include": emails(),
             "require": [], "exclude": [], "precedence": 1},
            {"id": "p2", "name": "kobo-token", "decision": "non_identity",
             "include": [{"service_token": {"token_id": "a836cb28-4c6e-440c-ac2b-b4317ee0b44c"}}],
             "require": [], "exclude": []}]},
        {"id": "x2", "name": "sp", "domain": "sp.kp-soft.dev",
         "self_hosted_domains": ["sp.kp-soft.dev"], "type": "self_hosted",
         "session_duration": "24h",
         "policies": [{"name": "SuperSync Bypass", "decision": "bypass",
                       "include": [{"everyone": {}}], "require": [], "exclude": []}]},
        {"id": "x3", "name": "Homelab", "domain": "*.kp-soft.dev",
         "self_hosted_domains": ["*.kp-soft.dev"], "type": "self_hosted",
         "session_duration": "730h",
         "policies": [{"name": "Toegang kpsoft", "decision": "allow", "include": emails(),
                       "require": [], "exclude": []}]}
    ])
}

fn api_dns() -> Value {
    json!([
        {"id": "d2", "type": "CNAME", "name": "kp-soft.dev",
         "content": "b027d052-9f97-43e5-9272-eeb336d3266e.cfargotunnel.com", "proxied": true,
         "ttl": 1},
        {"id": "d1", "type": "CNAME", "name": "*.kp-soft.dev",
         "content": "b027d052-9f97-43e5-9272-eeb336d3266e.cfargotunnel.com", "proxied": true,
         "ttl": 1}
    ])
}

fn live() -> EdgeState {
    let (t, c) = api_tunnel();
    EdgeState {
        tunnels: json!([project_tunnel(&t, &c)]),
        apps: project_apps(&api_apps()),
        dns: project_dns(&api_dns()),
    }
}

#[test]
fn fix_143_the_committed_capture_loads_with_the_ids_to_ask() {
    let (ids, cap) = load_capture(&captured_dir()).expect("capture loads");
    assert_eq!(ids.account_id, "19c7db90b03ef77b410fce31ba5624bf");
    assert_eq!(ids.zone_id, "f53290db3a400e6b3de07eda03c76267");
    assert_eq!(ids.tunnel_id, "b027d052-9f97-43e5-9272-eeb336d3266e");
    assert_eq!(cap.apps.as_array().map(Vec::len), Some(3));
    assert_eq!(cap.dns.as_array().map(Vec::len), Some(2));
}

#[test]
fn fix_143_an_edge_equal_to_the_capture_says_nothing() {
    let (_, cap) = load_capture(&captured_dir()).unwrap();
    // Tunnel health, ids, versions and record order are not configuration.
    let got = compare_edge(&cap, &live());
    assert!(got.is_empty(), "{got:#?}");
}

#[test]
fn fix_143_the_wildcard_app_flipped_to_bypass_is_broken() {
    let (_, cap) = load_capture(&captured_dir()).unwrap();
    let mut apps = api_apps();
    apps[2]["policies"][0]["decision"] = json!("bypass");
    apps[2]["policies"][0]["include"] = json!([{"everyone": {}}]);
    let l = EdgeState {
        apps: project_apps(&apps),
        ..live()
    };
    let got = compare_edge(&cap, &l);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert_eq!(got[0].severity, Severity::Broken);
    assert!(got[0].what.contains("Homelab"), "{}", got[0].what);
    assert!(got[0].what.contains("*.kp-soft.dev"), "{}", got[0].what);
}

#[test]
fn fix_143_an_added_address_a_removed_app_and_a_new_record_are_drift() {
    let (_, cap) = load_capture(&captured_dir()).unwrap();
    let mut apps = api_apps();
    apps[2]["policies"][0]["include"]
        .as_array_mut()
        .unwrap()
        .push(json!({"email": {"email": "d@example.invalid"}}));
    apps.as_array_mut().unwrap().remove(1);
    let mut dns = api_dns();
    dns.as_array_mut()
        .unwrap()
        .push(json!({"type": "A", "name": "x.kp-soft.dev",
        "content": "203.0.113.9", "proxied": false}));
    let l = EdgeState {
        apps: project_apps(&apps),
        dns: project_dns(&dns),
        ..live()
    };
    let got = compare_edge(&cap, &l);
    let text: Vec<String> = got
        .iter()
        .map(|f| format!("{:?} {} {}", f.severity, f.subject, f.what))
        .collect();
    assert!(
        got.iter().all(|f| f.severity == Severity::Drift),
        "{text:#?}"
    );
    for needle in ["Homelab", "sp", "x.kp-soft.dev"] {
        assert!(
            text.iter().any(|t| t.contains(needle)),
            "names {needle}: {text:#?}"
        );
    }
}

#[test]
fn fix_143_a_changed_ingress_is_drift() {
    let (_, cap) = load_capture(&captured_dir()).unwrap();
    let (t, mut c) = api_tunnel();
    c["config"]["ingress"][0]["service"] = json!("http://10.10.10.99:80");
    let l = EdgeState {
        tunnels: json!([project_tunnel(&t, &c)]),
        ..live()
    };
    let got = compare_edge(&cap, &l);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert_eq!(got[0].severity, Severity::Drift);
    assert!(
        got[0].what.contains("kp-soft.dev-tunnel"),
        "{}",
        got[0].what
    );
}

#[test]
fn fix_143_addresses_are_redacted_like_the_capture() {
    let v = redact_emails(&json!({"email": {"email": "someone@example.invalid"}, "n": "x"}));
    assert_eq!(v, json!({"email": {"email": "<email>"}, "n": "x"}));
}
