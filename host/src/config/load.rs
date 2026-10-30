//! Reading the settings: the resolved `Config`, loading from file and environment, the startup checks and persisting a change.

use super::*;

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
