//! feat-settings-1: the settings page's view of host.toml, and the check of
//! a change before it goes to the host. The key table is homelab-core's
//! (`hostconfig`), which the host consults again when the change arrives.
//! A table-shaped key is edited as a TOML fragment and turned into JSON
//! here. arch-self: a key that can cut the dashboard off is never sent; a
//! key that can take its route or the backups down needs its name typed.
//! Pure.

use std::collections::{BTreeMap, BTreeSet};

use homelab_core::hostconfig::{self, Access, KeyInfo, Kind, KEYS};
use homelab_proto::HostConfigFile;
use serde::{Deserialize, Serialize};

use super::actions::Refusal;

/// One field of the page.
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    #[serde(flatten)]
    pub info: KeyInfo,
    /// Whether the file sets the key (otherwise the default applies).
    pub set: bool,
    /// The value as JSON; null when unset or secret.
    pub value: serde_json::Value,
    /// For a table-shaped key: the value as a TOML fragment to edit.
    pub toml: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub path: String,
    pub sha256: String,
    pub fields: Vec<Field>,
    /// Keys the file sets that the host does not read.
    pub unknown: Vec<String>,
}

/// A value as `key = …` TOML (a table as `[key]` / `[[key]]` sections).
pub fn toml_fragment(key: &str, value: &serde_json::Value) -> String {
    if value.is_null() {
        return String::new();
    }
    let v: Result<toml::Value, _> = serde_json::from_value(value.clone());
    let Ok(v) = v else {
        return String::new();
    };
    let mut t = toml::Table::new();
    t.insert(key.to_string(), v);
    toml::to_string_pretty(&t).unwrap_or_default()
}

/// A TOML fragment the person typed for one key, as JSON. The fragment
/// must set that key and nothing else; empty removes it.
pub fn parse_fragment(key: &str, text: &str) -> Result<serde_json::Value, String> {
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    let t: toml::Table =
        toml::from_str(text).map_err(|e| format!("{key}: the TOML does not read: {e}"))?;
    let extra: Vec<&String> = t.keys().filter(|k| k.as_str() != key).collect();
    if !extra.is_empty() {
        return Err(format!(
            "{key}: the fragment also sets {}; write only {key} here",
            extra
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let v = t
        .get(key)
        .ok_or_else(|| format!("{key}: the fragment does not set {key}"))?;
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// The page, from what the host answered.
pub fn page(file: &HostConfigFile) -> Page {
    let fields = KEYS
        .iter()
        .map(|info| {
            let value = file.values.get(info.key).cloned();
            let set = value.is_some() || file.secrets_set.iter().any(|k| k == info.key);
            let value = value.unwrap_or(serde_json::Value::Null);
            let toml = (info.kind == Kind::Table
                && !matches!(info.access, Access::Secret | Access::DashboardSecret))
            .then(|| toml_fragment(info.key, &value));
            Field {
                info: *info,
                set,
                value,
                toml,
            }
        })
        .collect();
    Page {
        path: file.path.clone(),
        sha256: file.sha256.clone(),
        fields,
        unknown: file.unknown.clone(),
    }
}

/// What the page sends.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub expect_sha256: String,
    /// Key → new value (null removes the key).
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
    /// Key → TOML fragment, for table-shaped keys.
    #[serde(default)]
    pub fragments: BTreeMap<String, String>,
    /// The names typed for keys that ask a second confirmation.
    #[serde(default)]
    pub confirms: BTreeSet<String>,
}

/// The change as the host takes it, or why it is refused.
pub fn check(change: Change) -> Result<BTreeMap<String, serde_json::Value>, Refusal> {
    let refused = |why: String, fix: &str| Refusal::new("the host settings", why, fix);
    if change.expect_sha256.len() != 64 {
        return Err(refused(
            "the change does not say which version of host.toml it was made on".into(),
            "reload the settings page",
        ));
    }
    let mut out = change.values;
    for (key, text) in &change.fragments {
        if out.contains_key(key) {
            return Err(refused(format!("{key} is sent twice"), "send it once"));
        }
        out.insert(
            key.clone(),
            parse_fragment(key, text).map_err(|e| refused(e, "correct the TOML"))?,
        );
    }
    if out.is_empty() {
        return Err(refused(
            "nothing was changed".into(),
            "change a value first",
        ));
    }
    let mut why = Vec::new();
    for (key, value) in &out {
        if let Err(e) = hostconfig::check_value(key, value) {
            why.push(e);
            continue;
        }
        let info = hostconfig::key_info(key).map(|k| k.access);
        if info == Some(Access::Confirm) && !change.confirms.contains(key) {
            why.push(format!(
                "{key} can take the dashboard's route or the backups down; type its name to confirm"
            ));
        }
    }
    if !why.is_empty() {
        return Err(refused(why.join("; "), "correct the marked fields"));
    }
    Ok(out)
}
