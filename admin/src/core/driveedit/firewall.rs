//! Edit forms: the firewall form: rules, model and body.

use super::*;

/// `ruleFromValues`.
pub fn rule_from_values(v: &Values) -> Value {
    let s = |k: &str| text_of(v.get(k)).trim().to_string();
    let dir = if s("dir") == "out" { "out" } else { "in" };
    let mut r = Map::new();
    r.insert("dir".into(), json!(dir));
    let action = s("action");
    r.insert(
        "action".into(),
        json!(if action.is_empty() {
            "ACCEPT".to_string()
        } else {
            action
        }),
    );
    if !s("peer").is_empty() {
        r.insert(
            if dir == "in" { "source" } else { "dest" }.into(),
            json!(s("peer")),
        );
    }
    if !s("proto").is_empty() {
        r.insert("proto".into(), json!(s("proto")));
    }
    if !s("dport").is_empty() {
        let d: String = s("dport").chars().filter(|c| !c.is_whitespace()).collect();
        r.insert("dport".into(), json!(d));
    }
    if !s("note").is_empty() {
        r.insert("note".into(), json!(s("note")));
    }
    let c = text_of(v.get("comment")).trim_end().to_string();
    if !c.trim().is_empty() {
        r.insert("comment".into(), json!(c));
    }
    Value::Object(r)
}

pub(super) fn clean_rule(r: &Value) -> Value {
    let mut out = Map::new();
    out.insert("dir".into(), r["dir"].clone());
    out.insert("action".into(), r["action"].clone());
    for k in ["source", "dest", "proto", "dport", "comment", "note"] {
        let v = &r[k];
        if v.is_null() {
            continue;
        }
        let t = text_of(Some(v));
        if t.trim().is_empty() {
            continue;
        }
        out.insert(
            k.into(),
            if k == "comment" {
                v.clone()
            } else {
                json!(t.trim())
            },
        );
    }
    Value::Object(out)
}

/// `firewallModel`: a stack without a firewall starts closed and off.
pub fn firewall_model(fw: &Value) -> Value {
    let or = |k: &str, d: &str| fw[k].as_str().unwrap_or(d).to_string();
    let rules: Vec<Value> = fw["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, rule)| json!({ "origin": i, "rule": rule }))
        .collect();
    json!({
        "enabled": fw["enabled"].as_bool().unwrap_or(false),
        "comment": or("comment", ""),
        "policy_in": or("policy_in", "DROP"),
        "policy_out": or("policy_out", "ACCEPT"),
        "management_open": or("management_open", ""),
        "rules": rules,
    })
}

/// `firewallBody`: the edit the server takes.
pub fn firewall_body(m: &Value) -> Value {
    let comment = m["comment"].as_str().unwrap_or("");
    let mgmt = m["management_open"].as_str().unwrap_or("").trim();
    let rules: Vec<Value> = m["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| json!({ "origin": r["origin"], "rule": clean_rule(&r["rule"]) }))
        .collect();
    json!({
        "kind": "firewall",
        "enabled": m["enabled"].as_bool().unwrap_or(false),
        "comment": if comment.trim().is_empty() { Value::Null } else { json!(comment) },
        "policy_in": m["policy_in"],
        "policy_out": m["policy_out"],
        "management_open": if mgmt.is_empty() { Value::Null } else { json!(mgmt) },
        "rules": rules,
    })
}

/// `firewallChanged`.
pub fn firewall_changed(m: &Value, fw: &Value) -> bool {
    firewall_body(m) != firewall_body(&firewall_model(fw))
}

/// `ruleSummary`: one rule in words.
pub fn rule_summary(r: &Value) -> String {
    let dir = r["dir"].as_str().unwrap_or("in");
    let (peer, word) = if dir == "in" {
        (&r["source"], "from")
    } else {
        (&r["dest"], "to")
    };
    let mut parts = vec![
        dir.to_uppercase(),
        r["action"].as_str().unwrap_or("").to_string(),
    ];
    match peer.as_str().filter(|p| !p.is_empty()) {
        Some(p) => parts.push(format!("{word} {p}")),
        None => parts.push(format!("{word} anywhere")),
    }
    if let Some(p) = r["proto"].as_str().filter(|p| !p.is_empty()) {
        parts.push(p.to_string());
    }
    if let Some(p) = r["dport"].as_str().filter(|p| !p.is_empty()) {
        parts.push(format!("port {p}"));
    }
    parts.join(" ")
}

/// `ruleFields`: the rule dialog, for a new rule or the one edited.
pub fn rule_fields(r: Option<&Value>) -> Vec<Field> {
    let from = |name: &str| -> Option<Value> {
        let r = r?;
        let v = match name {
            "peer" => {
                if r["dir"] == "in" {
                    r["source"].clone()
                } else {
                    r["dest"].clone()
                }
            }
            n => r[n].clone(),
        };
        (!v.is_null()).then_some(v)
    };
    es().rule
        .iter()
        .map(|d| {
            let mut f = d.field();
            let peer_default = (d.name == "peer" && r.is_some()).then(|| json!(""));
            f.current = from(&d.name).or(peer_default).or(f.current);
            f
        })
        .collect()
}
