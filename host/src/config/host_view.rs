//! The host-config view the dashboard edits: the live table, applying a change or a whole file, and the watch-URL rules.

use super::*;

/// JSON as a TOML value; null has no TOML form.
pub(crate) fn json_to_toml(v: &serde_json::Value) -> Result<toml::Value, String> {
    Ok(match v {
        serde_json::Value::Null => return Err("null inside a value has no TOML form".into()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64().ok_or("not a number")?),
        },
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(a) => {
            toml::Value::Array(a.iter().map(json_to_toml).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(o) => {
            let mut t = toml::Table::new();
            for (k, x) in o {
                if !x.is_null() {
                    t.insert(k.clone(), json_to_toml(x)?);
                }
            }
            toml::Value::Table(t)
        }
    })
}

/// fix-110 (fix-170: also drops host-held keys): the non-secret, non-host-held
/// keys host.toml sets right now, as JSON — for the fleet check's comparison
/// with `config/host.toml` (`evaluate_host_config_drift`). `tokens` is left
/// out here the same way a secret is, since `config/host.toml` never
/// declares it either — comparing the two would always show drift.
/// Unlike `host_config_view` this never fails: an unreadable or unparsable
/// file is simply "sets nothing", which is also true of a host that has
/// never had one.
pub(crate) fn live_host_config_table(
    raw: &str,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(table) = toml::from_str::<toml::Table>(raw) else {
        return out;
    };
    for (key, v) in &table {
        if homelab_core::hostconfig::is_secret(key) || homelab_core::hostconfig::is_host_held(key) {
            continue;
        }
        if let Ok(json) = serde_json::to_value(v) {
            out.insert(key.clone(), homelab_core::hostconfig::redact(key, &json));
        }
    }
    out
}

/// feat-settings-1: host.toml as `GetHostConfig` answers it, from the file's
/// text (pure: the caller reads the file).
pub(crate) fn host_config_view(
    path: &str,
    raw: &str,
) -> Result<homelab_proto::HostConfigFile, String> {
    let table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("{} does not parse as TOML: {}", path, e))?
    };
    let mut values = std::collections::BTreeMap::new();
    let mut secrets_set = Vec::new();
    for (key, v) in &table {
        if homelab_core::hostconfig::is_secret(key) {
            secrets_set.push(key.clone());
            continue;
        }
        let json = serde_json::to_value(v).map_err(|e| e.to_string())?;
        values.insert(key.clone(), homelab_core::hostconfig::redact(key, &json));
    }
    let (known_keys, defaults) = homelab_core::hostconfig::host_facts();
    Ok(homelab_proto::HostConfigFile {
        path: path.to_string(),
        sha256: homelab_core::manifest::sha256_hex(raw.as_bytes()),
        values,
        secrets_set,
        unknown: unknown_keys(&table),
        known_keys,
        defaults,
    })
}

/// fix-false-alarm (2026-09-30): the URL to reach once before a
/// `SetHostConfig` save is accepted, when `changes` sets `watch_url`.
/// `None` = nothing to check: the key is absent from this save, or set to
/// empty — turning the watch off is always allowed, since it can never
/// itself cause a false alarm.
pub(crate) fn watch_url_to_check(
    changes: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Option<String> {
    let s = changes.get("watch_url")?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// The reason to refuse the save because the host's own request to `url`
/// did not succeed (`reached` is what running the same probe
/// `watch_dashboard` uses — `curl -sf -m 10` — reported), or None when the
/// save may proceed. Pure: the caller does the actual request.
pub(crate) fn watch_url_refusal(url: &str, reached: bool) -> Option<String> {
    if reached {
        return None;
    }
    Some(format!(
        "the host cannot reach {url}: curl -sf -m 10 did not get an answer — deploy the \
         firewall rule that opens it first (or fix the address), then save this again; an \
         address the host cannot reach would page as \"the dashboard is down\" five minutes \
         after every save"
    ))
}
