//! Applying a change to host.toml: one key at a time, or a whole declared file.

use super::*;

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
