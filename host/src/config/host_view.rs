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

/// feat-settings-1: the new text of host.toml with `changes` applied, or
/// every reason it is refused. Pure. Refused: a file that moved since it was
/// read (`expect_sha256`), a key the dashboard may not change
/// (`homelab_core::hostconfig`), a value of the wrong shape, a key or
/// sub-key the host does not read, and anything the start-up validation
/// would stop the host for.
pub(crate) fn apply_host_config_changes(
    raw: &str,
    changes: &std::collections::BTreeMap<String, serde_json::Value>,
    expect_sha256: &str,
) -> Result<(String, homelab_proto::HostConfigSaved), String> {
    let now = homelab_core::manifest::sha256_hex(raw.as_bytes());
    if now != expect_sha256 {
        return Err(
            "host.toml changed since the dashboard read it (over ssh, or a TUI settings save); \
             read it again and redo the change"
                .into(),
        );
    }
    if changes.is_empty() {
        return Err("no change was sent".into());
    }
    let mut why = Vec::new();
    for (key, value) in changes {
        if let Err(e) = homelab_core::hostconfig::check_value(key, value) {
            why.push(e);
        }
    }
    if !why.is_empty() {
        return Err(why.join("; "));
    }
    let mut table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("host.toml does not parse as TOML: {}", e))?
    };
    let before_unknown = unknown_keys(&table);
    for (key, value) in changes {
        if value.is_null() {
            table.remove(key);
        } else {
            let v = json_to_toml(value).map_err(|e| format!("{}: {}", key, e))?;
            table.insert(key.clone(), v);
        }
    }
    let file: FileConfig = table
        .clone()
        .try_into()
        .map_err(|e| format!("the new host.toml is not a valid host config: {}", e))?;
    // A misspelt sub-key would be dropped in silence (F186).
    let stray: Vec<String> = unknown_keys(&table)
        .into_iter()
        .filter(|k| !before_unknown.contains(k))
        .collect();
    if !stray.is_empty() {
        why.push(format!(
            "the host does not read {} — check the spelling",
            stray.join(", ")
        ));
    }
    why.extend(startup_problems(&file));
    if !why.is_empty() {
        return Err(why.join("; "));
    }
    let text = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;
    let (mut live, mut restart) = (Vec::new(), Vec::new());
    for key in changes.keys() {
        match homelab_core::hostconfig::key_info(key).map(|k| k.apply) {
            Some(homelab_core::hostconfig::Apply::Live) => live.push(key.clone()),
            _ => restart.push(key.clone()),
        }
    }
    Ok((
        text.clone(),
        homelab_proto::HostConfigSaved {
            sha256: homelab_core::manifest::sha256_hex(text.as_bytes()),
            live,
            restart,
        },
    ))
}

/// fix-110: the new text of host.toml with `declared` (`config/host.toml`,
/// the repository's whole non-secret table) laid over `raw` (the host's own
/// current file), or every reason it is refused — the whole-file twin of
/// [`apply_host_config_changes`]. Pure. Refused the same way: a file that
/// moved since it was read (`expect_sha256`, when given), `declared`
/// setting a secret key (never allowed in the repository), a value of the
/// wrong shape, a key the host does not read, and anything the start-up
/// validation would stop the host for.
pub(crate) fn apply_host_config_whole(
    raw: &str,
    declared: &str,
    expect_sha256: Option<&str>,
    allow: &[String],
) -> Result<(String, homelab_proto::HostConfigSaved), String> {
    if let Some(expect) = expect_sha256 {
        let now = homelab_core::manifest::sha256_hex(raw.as_bytes());
        if now != expect {
            return Err(
                "host.toml changed since config/host.toml was read (over ssh, or a TUI \
                 settings save); read it again and redo `homelab host apply`"
                    .into(),
            );
        }
    }
    let declared_table: toml::Table = toml::from_str(declared)
        .map_err(|e| format!("config/host.toml does not parse as TOML: {e}"))?;
    let current_table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("host.toml does not parse as TOML: {e}"))?
    };
    let merged = homelab_core::hostconfig::apply_declared(&declared_table, &current_table)?;
    // fix-191: only the keys the sender named may move.
    let unannounced = homelab_core::hostconfig::unannounced_changes(&merged, &current_table, allow);
    if !unannounced.is_empty() {
        return Err(format!(
            "config/host.toml would also change {} on the host, which this apply did not name \
             — the working copy and the host disagree there; bring config/host.toml in line \
             with the host first, or run `homelab host apply`, which shows every change and \
             asks before it names them",
            unannounced.join(", ")
        ));
    }
    let file: FileConfig = merged
        .clone()
        .try_into()
        .map_err(|e| format!("the new host.toml is not a valid host config: {e}"))?;
    let stray = unknown_keys(&merged);
    let mut why: Vec<String> = if stray.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "config/host.toml sets {} — the host does not read it; check the spelling",
            stray.join(", ")
        )]
    };
    why.extend(startup_problems(&file));
    if !why.is_empty() {
        return Err(why.join("; "));
    }
    let text = toml::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    // Only the keys whose value actually moved between the host's current
    // file and the merged result — not every key `declared` names, which
    // would call nearly the whole file "restart" on every save and bury
    // the one key someone actually changed (a whole-file apply has no
    // `changes` map the way `SetHostConfig` does, so the diff is taken
    // here instead).
    let changed: std::collections::BTreeSet<&String> =
        merged.keys().chain(current_table.keys()).collect();
    let (mut live, mut restart) = (Vec::new(), Vec::new());
    for key in changed {
        if merged.get(key) == current_table.get(key) {
            continue;
        }
        match homelab_core::hostconfig::key_info(key).map(|k| k.apply) {
            Some(homelab_core::hostconfig::Apply::Live) => live.push(key.clone()),
            _ => restart.push(key.clone()),
        }
    }
    Ok((
        text.clone(),
        homelab_proto::HostConfigSaved {
            sha256: homelab_core::manifest::sha256_hex(text.as_bytes()),
            live,
            restart,
        },
    ))
}

/// Replace host.toml whole: a temp file with mode 0600 from the first byte
/// (it carries the bearer token, H21), then a rename.
pub(crate) fn write_config_file(path: &str, raw: &str) -> Result<(), String> {
    let tmp = format!("{}.tmp", path);
    // 0600 from the first byte — the file carries the bearer token (H21).
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(raw.as_bytes()).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}
