//! Saving a settings change back into host.toml without touching the rest of the file.

use super::*;

/// G8: render host.toml for a settings save: the file as it was read, with
/// only the settings the TUI may change replaced.
///
/// fix-134 (config-four-representations, see REGISTER.md): edits in place
/// with `toml_edit`, touching only the three keys a settings save actually
/// changes — everything else in the file, comments and key order included,
/// passes through byte-for-byte.
pub(crate) fn render_settings_toml(
    config: &Config,
    settings: &homelab_proto::HostConfigView,
) -> Result<String, String> {
    // feat-settings-1: the file as it is on disk now, so a key the dashboard
    // changed since the host started (`SetHostConfig`) is not written back
    // to its start-up value by a TUI settings save. The file as read at start
    // (re-rendered, so still comment-free) when it cannot be read now.
    let raw = std::fs::read_to_string(&config.config_path)
        .unwrap_or_else(|_| toml::to_string_pretty(&config.file).unwrap_or_default());
    let mut doc = raw
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("does not parse as TOML: {e}"))?;

    match settings.backup_hour {
        Some(h) => doc["backup_hour"] = toml_edit::value(i64::from(h)),
        None => {
            doc.remove("backup_hour");
        }
    }
    match &settings.notify_webhook {
        Some(w) => doc["notify_webhook"] = toml_edit::value(w.as_str()),
        None => {
            doc.remove("notify_webhook");
        }
    }
    let mut tiers = toml_edit::ArrayOfTables::new();
    for tier in &settings.retention {
        let mut table = toml_edit::Table::new();
        table.insert("every_days", toml_edit::value(i64::from(tier.every_days)));
        if let Some(span) = tier.span_days {
            table.insert("span_days", toml_edit::value(i64::from(span)));
        }
        tiers.push(table);
    }
    doc["retention"] = toml_edit::Item::ArrayOfTables(tiers);

    Ok(doc.to_string())
}

pub(crate) fn persist_settings(
    config: &Config,
    settings: &homelab_proto::HostConfigView,
) -> Result<(), String> {
    let raw = render_settings_toml(config, settings)?;
    write_config_file(&config.config_path, &raw)
}
