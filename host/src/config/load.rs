//! Reading the settings: the resolved `Config`, loading from file and environment, the startup checks and persisting a change.

use super::*;
use homelab_core::safety::SafetyConfig;
use std::net::SocketAddr;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) token: String,
    /// arch-tokens: scoped tokens from `[[tokens]]`, validated at start.
    pub(crate) tokens: Vec<TokenEntry>,
    pub(crate) listen: SocketAddr,
    pub(crate) state_dir: String,
    /// Path of the toml we loaded — SetConfig persists back to it.
    pub(crate) config_path: String,
    /// A6: remote exec endpoint switch. Deny-by-default; ssh-edited only
    /// (deliberately NOT in the G8 settings tab).
    pub(crate) exec_enabled: bool,
    /// Bearer token for the notification target, when it needs one.
    ///
    /// Deliberately here rather than in HostConfigView beside notify_webhook:
    /// that view is the settings the CLIENT can read back, and a secret does
    /// not belong in a screen. ssh-edited only, like exec_enabled.
    pub(crate) notify_auth_bearer: Option<String>,
    /// G14: how long a passed restore drill counts for. B3 says quarterly;
    /// the default is 90 days. Kenny's, not the author's — a house that
    /// changes little wants it longer, one being rebuilt wants it shorter.
    pub(crate) restore_drill_interval_s: u64,
    /// fix-62: where the nightly restore drill restores a repository to — a
    /// data pool, not the root disk. "none" is not a valid value (unlike
    /// staging, the drill always needs somewhere to restore to); an absent
    /// `host.toml` key falls back to `DEFAULT_DRILL_SCRATCH_DIR`.
    pub(crate) restore_drill_scratch_dir: String,
    /// G16: the second route, tried when the first one does not answer 2xx.
    ///
    /// Y2 sends notifications through kyu so an HA outage cannot lose them.
    /// Its one carved-out exception was "kyu itself is down" — which is
    /// exactly what happens while the orchestrator is updating kyu, and the
    /// message lost in that window is the one saying the update failed. Point
    /// this straight at Home Assistant.
    pub(crate) notify_fallback_webhook: Option<String>,
    /// Credential for that second route, when it needs one. The HA webhook
    /// does not; something else might.
    pub(crate) notify_fallback_auth_bearer: Option<String>,
    /// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
    /// LAN-only self-signed certificate (PEM) an `https://` route is pinned
    /// to — the host's own copy, for curl's `--cacert`. Required once any
    /// notify route is `https://` (`startup_problems`); absent otherwise.
    pub(crate) notify_tls_cert: Option<String>,
    /// The SHA-256 of that certificate's DER form, lowercase hex. Checked
    /// against the file on disk at every start: a mismatch means the file
    /// changed since it was pinned (the hub's certificate was regenerated,
    /// or the file is the wrong one), and is refused the same way a
    /// changed host certificate refuses the client (`reconcile_pin`).
    pub(crate) notify_tls_fingerprint: Option<String>,
    /// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01):
    /// the read-only Cloudflare API token the host's nightly edge check
    /// uses, mirroring the workstation's `~/.config/cloudflare/kp-soft.token`
    /// (`client/src/edge.rs`). A secret: changed over ssh or the dashboard's
    /// secret field, never shown back. Absent = the nightly comparison
    /// reports "not configured", not broken.
    pub(crate) cloudflare_token: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    pub(crate) prometheus_url: Option<String>,
    pub(crate) loki_url: Option<String>,
    /// fix-93: where the log question is asked; see the file field.
    pub(crate) loki_vmid: Option<u16>,
    /// Window for the log-coverage question; see the file field.
    pub(crate) logs_window: String,
    /// D5: git remote URL for the offsite intent mirror; None = off.
    /// The latch environment the Secrets page writes to; None until set.
    pub(crate) latch_env: Option<String>,
    /// The managed tools, each with its table name filled in.
    pub(crate) tools: Vec<homelab_core::tools::spec::ToolSpec>,
    pub(crate) mirror_remote: Option<String>,
    /// H1 (hardening): safety values configurable via host.toml so M5 can
    /// migrate the gateway / adjust the no-touch list without a release.
    /// Hardcoded DEFAULT_NO_TOUCH remains the default.
    pub(crate) safety: SafetyConfig,
    /// E8: declared ZFS replication jobs; empty = feature off.
    pub(crate) zfs_jobs: Vec<homelab_core::ops::zfs::ZfsJob>,
    pub(crate) watched_backups: Vec<WatchedBackup>,
    pub(crate) device_backups: Vec<homelab_core::ops::devicebackup::DeviceBackup>,
    /// Backup target and timeouts, resolved once from host.toml. Callers
    /// clone this and override only `tiers`.
    pub(crate) backup: homelab_core::ops::backup::BackupCfg,
    /// D60: the pull-through cache in the house. Absent = images keep naming
    /// their own origin, which is also what happens when it does not answer.
    pub(crate) registry_cache: Option<homelab_core::ops::registry_cache::CacheCfg>,
    /// rule-20: the fleet default log rotation; see `FileConfig`.
    pub(crate) default_log_rotation: Option<homelab_core::ops::guards::FleetLogRotationDefault>,
    /// T1: where per-stack Prometheus discovery files are written.
    pub(crate) metrics_targets_dir: Option<String>,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// dashboard's own address (CT 120), from which the firewall step of a
    /// deploy derives an inbound rule per stack for every tile that opens on
    /// that stack's own container. Absent or empty = feature off, and no
    /// rule is derived.
    pub(crate) tile_watch_source: Option<String>,
    /// trusted-sources (Kenny, 2026-10-22): addresses every stack firewall
    /// lets in on every port (`firewall::trusted_rules`). Absent = none.
    pub(crate) trusted_sources: Vec<String>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    pub(crate) ask_timeout_s: u64,
    /// Decision "23 constants": [`homelab_core::ops::fleetcheck::PATCH_THRESHOLD_S`] by default.
    pub(crate) patch_threshold_s: u64,
    /// [`homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S`] by default.
    pub(crate) backup_max_age_s: u64,
    /// [`homelab_core::ops::fleetcheck::HOST_META_MAX_AGE_S`] by default.
    pub(crate) host_meta_max_age_s: u64,
    /// [`homelab_core::ops::fleetcheck::NIGHTLY_REPORT_REPEAT_S`] by default.
    pub(crate) nightly_report_repeat_s: u64,
    /// [`homelab_core::incidents::BUNDLE_MAX_AGE_DAYS`] by default.
    pub(crate) incident_bundle_max_age_days: u64,
    /// [`homelab_core::incidents::BUNDLE_MAX_COUNT`] by default.
    pub(crate) incident_bundle_max_count: usize,
    /// gap-26: [`homelab_core::incidents::JOURNAL_MAX_BYTES`] by default.
    pub(crate) journal_max_bytes: u64,
    /// fix-122: [`homelab_core::logring::LOG_RING_MAX_BYTES`] by default.
    /// Restart-applied, like `journal_max_bytes` beside it: the ring
    /// writer's cap is read once, at `init_production_logging`.
    pub(crate) log_ring_max_bytes: u64,
    /// fix-122: the daemon's startup filter — `RUST_LOG` wins over this when
    /// set, exactly as it always has. Live-reloadable afterwards through
    /// `settings.log_level` and `AppState::log_filter`; this field is read
    /// only once, to seed the subscriber.
    pub(crate) log_level: String,
    /// [`homelab_core::ops::pins::UPSTREAM_MAX_AGE_S`] by default.
    pub(crate) upstream_max_age_s: u64,
    /// Y1: how many stack backups the nightly round runs at once. Measured
    /// 2026-09-02: a full round took ~38 minutes for thirteen stacks, of
    /// which only ~6 minutes was writing data — the rest was small questions
    /// to Google Drive, each waiting on a round-trip rather than on
    /// bandwidth, which is exactly the kind of waiting that overlaps.
    ///
    /// Configurable rather than a constant, and not only on principle: a
    /// backup pauses its containers for a clean snapshot, so this number is
    /// also "how much of the house may be briefly still at 04:00". Kenny
    /// chose three (form Y4): about a third of the wait, and never more than
    /// three services quiet at once.
    pub(crate) backup_concurrency: usize,
    /// fix-96: the second repository set's dataset; None = no second copy.
    pub(crate) second_copy_dataset: Option<String>,
    /// fix-96: how often a repository's check reads data.
    pub(crate) integrity_data_read_interval_s: u64,
    /// feat-platform-2: seconds between two status readings.
    pub(crate) status_interval_s: u64,
    /// feat-platform-3: how many of the newest lines `CurrentOp` returns.
    pub(crate) recent_lines: usize,
    /// arch-history: how long and how large history.jsonl may grow.
    pub(crate) history_max_age_s: u64,
    pub(crate) history_max_bytes: usize,
    /// Decision notify-detail: the dashboard's public address.
    pub(crate) dashboard_url: String,
    /// replace-kuma: the dashboard's health address, when watched.
    pub(crate) watch_url: Option<String>,
    /// Owner decision "default plus per tile": how often `watch_url` is
    /// asked, and the fleet default handed to the dashboard's own watch.
    pub(crate) watch_interval_s: u64,
    /// How long `watch_url` (or, by fleet default, a tile) may fail before
    /// it counts as down.
    pub(crate) watch_down_after_s: u64,
    /// fix-184: the stack `watch_url` answers for; see `FileConfig`.
    pub(crate) watch_stack: Option<String>,
    /// rule-20: warn/critical thresholds for pve's own capacity readings.
    pub(crate) capacity_thresholds: homelab_core::ops::fleetcheck::HostCapacityThresholds,
    /// rule-20: Prometheus' own TSDB cap, in MiB; None = not asked.
    pub(crate) tsdb_retention_size_mib: Option<u64>,
    /// Initial mutable settings (live copy lives in AppState.settings).
    pub(crate) initial_settings: homelab_proto::HostConfigView,
    /// host.toml as it was read; a settings save writes this back with only
    /// the settings changed.
    pub(crate) file: FileConfig,
}

/// F186: a misplaced `host.toml` key silently lands under the wrong table
/// header and serde drops it as unknown — see REGISTER.md. Found by reading
/// the file into `FileConfig` and writing that back: whatever the round trip
/// lost, no field reads (config-four-representations). Returned as dotted
/// paths, e.g. `registry_cache.upstreams[0].opnsense_url`. A file that does
/// not deserialize returns nothing here; loading it fails loudly.
pub(crate) fn unknown_keys(raw: &toml::Table) -> Vec<String> {
    fn walk(out: &mut Vec<String>, raw: &toml::Value, read: Option<&toml::Value>, path: &str) {
        match (raw, read) {
            (toml::Value::Table(r), Some(toml::Value::Table(k))) => {
                for (key, v) in r {
                    let p = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    match k.get(key) {
                        Some(kv) => walk(out, v, Some(kv), &p),
                        None => out.push(p),
                    }
                }
            }
            (toml::Value::Array(r), Some(toml::Value::Array(k))) => {
                for (i, v) in r.iter().enumerate() {
                    walk(out, v, k.get(i), &format!("{}[{}]", path, i));
                }
            }
            _ => {}
        }
    }
    let Ok(file) = raw.clone().try_into::<FileConfig>() else {
        return Vec::new();
    };
    let Ok(read) = toml::Table::try_from(&file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(
        &mut out,
        &toml::Value::Table(raw.clone()),
        Some(&toml::Value::Table(read)),
        "",
    );
    out.sort();
    out
}

pub(crate) fn load_config() -> Config {
    load_config_from(
        std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "/etc/homelab/host.toml".into()),
    )
}

pub(crate) fn load_config_from(path: String) -> Config {
    let file = read_file_config(&path);
    let as_read = file.clone();
    let (token, log_level, tokens) = start_checks(&file, &path);
    let safety = safety_config(&file);
    let backup = backup_config(&file);

    let listen = std::env::var("HOMELAB_LISTEN")
        .ok()
        .or(file.listen)
        .unwrap_or_else(|| "0.0.0.0:8443".into())
        .parse()
        .expect("listen must be host:port");
    let state_dir = std::env::var("HOMELAB_STATE_DIR")
        .ok()
        .or(file.state_dir)
        .unwrap_or_else(|| "/var/lib/homelab".into());
    let cfg = Config {
        token,
        tokens,
        listen,
        state_dir,
        config_path: path,
        exec_enabled: file.exec_enabled.unwrap_or(false),
        notify_auth_bearer: file.notify_auth_bearer.clone(),
        restore_drill_interval_s: file
            .restore_drill_interval_s
            .unwrap_or(homelab_core::ops::restoredrill::DEFAULT_DRILL_INTERVAL_S),
        restore_drill_scratch_dir: file
            .restore_drill_scratch_dir
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                homelab_core::ops::restoredrill::DEFAULT_DRILL_SCRATCH_DIR.to_string()
            }),
        notify_fallback_webhook: file.notify_fallback_webhook.clone(),
        notify_fallback_auth_bearer: file.notify_fallback_auth_bearer.clone(),
        notify_tls_cert: file.notify_tls_cert.clone(),
        notify_tls_fingerprint: file.notify_tls_fingerprint.clone(),
        cloudflare_token: file.cloudflare_token.clone(),
        prometheus_url: file.prometheus_url.clone(),
        loki_url: file.loki_url.clone(),
        loki_vmid: file.loki_vmid,
        logs_window: file.logs_window.clone().unwrap_or_else(default_logs_window),
        latch_env: file
            .latch_env
            .clone()
            .or_else(|| std::env::var("HOMELAB_LATCH_ENV").ok())
            .filter(|e| !e.trim().is_empty()),
        tools: file
            .tools
            .iter()
            .flatten()
            .map(|(name, spec)| homelab_core::tools::spec::ToolSpec {
                name: name.clone(),
                ..spec.clone()
            })
            .collect(),
        mirror_remote: file.mirror_remote,
        safety,
        zfs_jobs: file.zfs_jobs.unwrap_or_default(),
        watched_backups: file.watched_backups.unwrap_or_default(),
        device_backups: file.device_backups.unwrap_or_default(),
        // D60: absent from host.toml = no cache, which is the same behaviour
        // the fleet had before there was one.
        registry_cache: file.registry_cache,
        // rule-20: absent from host.toml = no fleet default, same as before.
        default_log_rotation: file.default_log_rotation,
        // rule-20: absent from host.toml = the audit's own defaults.
        capacity_thresholds: file.capacity_thresholds.unwrap_or_default(),
        tsdb_retention_size_mib: file.tsdb_retention_size_mib,
        backup,
        metrics_targets_dir: file.metrics_targets_dir,
        tile_watch_source: file.tile_watch_source.filter(|s| !s.trim().is_empty()),
        trusted_sources: file
            .trusted_sources
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect(),
        backup_concurrency: file
            .backup_concurrency
            .unwrap_or_else(default_backup_concurrency),
        ask_timeout_s: file.ask_timeout_s.unwrap_or_else(default_ask_timeout_s),
        patch_threshold_s: file
            .patch_threshold_s
            .unwrap_or(homelab_core::ops::fleetcheck::PATCH_THRESHOLD_S),
        backup_max_age_s: file
            .backup_max_age_s
            .unwrap_or(homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S),
        host_meta_max_age_s: file
            .host_meta_max_age_s
            .unwrap_or(homelab_core::ops::fleetcheck::HOST_META_MAX_AGE_S),
        nightly_report_repeat_s: file
            .nightly_report_repeat_s
            .unwrap_or(homelab_core::ops::fleetcheck::NIGHTLY_REPORT_REPEAT_S),
        incident_bundle_max_age_days: file
            .incident_bundle_max_age_days
            .unwrap_or(homelab_core::incidents::BUNDLE_MAX_AGE_DAYS),
        incident_bundle_max_count: file
            .incident_bundle_max_count
            .unwrap_or(homelab_core::incidents::BUNDLE_MAX_COUNT),
        journal_max_bytes: file
            .journal_max_bytes
            .unwrap_or(homelab_core::incidents::JOURNAL_MAX_BYTES as u64),
        log_ring_max_bytes: file
            .log_ring_max_bytes
            .unwrap_or(homelab_core::logring::LOG_RING_MAX_BYTES as u64),
        log_level: log_level.clone(),
        upstream_max_age_s: file
            .upstream_max_age_s
            .unwrap_or(homelab_core::ops::pins::UPSTREAM_MAX_AGE_S),
        second_copy_dataset: file.second_copy_dataset.clone(),
        integrity_data_read_interval_s: file
            .integrity_data_read_interval_s
            .unwrap_or(homelab_core::ops::secondcopy::DEFAULT_DATA_READ_INTERVAL_S),
        status_interval_s: file.status_interval_s.unwrap_or(60),
        recent_lines: file.recent_lines.unwrap_or(2000),
        history_max_age_s: file.history_days.unwrap_or(90) * 86_400,
        history_max_bytes: file.history_max_mib.unwrap_or(16) * 1024 * 1024,
        dashboard_url: file
            .dashboard_url
            .clone()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| homelab_core::notify::DEFAULT_DASHBOARD_URL.to_string()),
        watch_url: file.watch_url.clone().filter(|u| !u.trim().is_empty()),
        watch_interval_s: file.watch_interval_s.unwrap_or(60),
        watch_down_after_s: file.watch_down_after_s.unwrap_or(300),
        watch_stack: file.watch_stack.clone().filter(|s| !s.trim().is_empty()),
        initial_settings: homelab_proto::HostConfigView {
            backup_hour: file.backup_hour,
            notify_webhook: file.notify_webhook,
            retention: file
                .retention
                .unwrap_or_else(homelab_core::retention::default_tiers),
            log_level: log_level.clone(),
        },
        file: as_read,
    };
    warn_at_load(&cfg);
    cfg
}

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

/// feat-settings-1: the start-up validation's refusals for a file, without
/// the process exit, so a settings change is held to the same checks
/// before it is written. `load_config_from` exits on the same list.
pub(crate) fn startup_problems(file: &FileConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = &file.token
        && t.len() < 16
    {
        out.push("token must be at least 16 characters".into());
    }
    if file.status_interval_s.is_some_and(|s| s < 10) {
        out.push(
            "status_interval_s must be at least 10 (each reading runs one probe per container)"
                .into(),
        );
    }
    if let Err(e) = validate_tokens(file.tokens.as_deref().unwrap_or_default()) {
        out.push(e);
    }
    for (name, spec) in file.tools.iter().flatten() {
        let named = homelab_core::tools::spec::ToolSpec {
            name: name.clone(),
            ..spec.clone()
        };
        if let Err(e) = named.validate() {
            out.push(e);
        }
    }
    if let Some(l) = &file.listen
        && l.parse::<SocketAddr>().is_err()
    {
        out.push(format!("listen {:?} must be host:port", l));
    }
    if file.backup_hour.is_some_and(|h| h > 23) {
        out.push("backup_hour must be 0-23".into());
    }
    if file.retention.as_ref().is_some_and(|r| r.is_empty()) {
        out.push("retention needs at least one tier".into());
    }
    if let Some(level) = &file.log_level
        && !level.trim().is_empty()
        && !log_level_is_valid(level)
    {
        out.push(format!(
            "log_level {:?} is not a tracing/EnvFilter directive (e.g. \"info\", \"debug\")",
            level
        ));
    }
    for job in file.zfs_jobs.iter().flatten() {
        if let Some(p) = homelab_core::ops::zfs::job_problems(job) {
            out.push(format!("zfs_jobs {} → {}: {}", job.source, job.target, p));
        }
    }
    // fix-126 (TLS to the message hub, owner decision 2026-10-01): an
    // `https://` notify route with no pin would fall back to curl's system
    // CA bundle, which a LAN self-signed certificate never passes — refused
    // here, at the config, rather than discovered as "every notification
    // fails" at 04:00. `http://` stays accepted unconditionally (migration
    // is stepwise); the actual fingerprint-on-disk match is a boot-time
    // warning (`load_config_from`), not refused here, since it needs to
    // read the file.
    let https_route = [
        file.notify_webhook.as_deref(),
        file.notify_fallback_webhook.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|u| u.starts_with("https://"));
    if https_route && (file.notify_tls_cert.is_none() || file.notify_tls_fingerprint.is_none()) {
        out.push(
            "an https:// notify route needs notify_tls_cert and notify_tls_fingerprint — the \
             LAN hub's certificate is self-signed, so curl trusts nothing unless it is pinned"
                .into(),
        );
    }
    out
}
