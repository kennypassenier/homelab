//! Homelab HOST daemon — thin shell around homelab-core (AR1).
//!
//! Provides: the real Executor (processes + files), config (TOML + env,
//! AR11), tracing (AR15), the journal file (B5), the WS server with required
//! bearer token, and the broadcast sink feeding connected clients (F2).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::process::Command;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor};
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::Journal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::{PipelineEvent, Sink};

use homelab_proto::{Command as Rpc, RpcRequest, RpcResponse, ServerMsg};

mod tls;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Config (AR11) ────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, Default)]
struct FileConfig {
    token: Option<String>,
    listen: Option<String>,
    state_dir: Option<String>,
    backup_hour: Option<u8>,
    notify_webhook: Option<String>,
    /// Bearer token sent with the notification POST, when the target needs
    /// one. Added 2026-08-31: Kenny chose to route homelab warnings through
    /// the kyu hub rather than straight at Home Assistant (R2), so that a
    /// warning survives HA being the thing that is broken — and the hub
    /// requires a token where the HA webhook did not.
    notify_auth_bearer: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    retention: Option<Vec<homelab_proto::RetentionTier>>,
    exec_enabled: Option<bool>,
    mirror_remote: Option<String>,
    opnsense_url: Option<String>,
    opnsense_cred_file: Option<String>,
    no_touch: Option<Vec<u16>>,
    gateway_vmid: Option<u16>,
    gateway_routes_dir: Option<String>,
    /// E8: ZFS snapshot+replication jobs (replaces the old cron script).
    zfs_jobs: Option<Vec<homelab_core::ops::zfs::ZfsJob>>,
    /// Where restic writes. Was a string literal in BackupCfg::default(),
    /// which meant exactly one backup target could ever be addressed while
    /// the scope asks for two (deployment project, F39 / standing rule 27).
    restic_base: Option<String>,
    /// Path on this host to the file holding the restic password.
    restic_password_file: Option<String>,
    /// Seconds a single snapshot may take. Default 4 h: a first multi-GB
    /// upload over a residential uplink is slow.
    restic_snapshot_timeout_s: Option<u64>,
    /// Seconds a single restore may take. Default 4 h, matching the snapshot
    /// side — it used to be a hardcoded 1800 (F38).
    restic_restore_timeout_s: Option<u64>,
    /// T1: directory the orchestrator writes per-stack Prometheus discovery
    /// files into. Absent = off, and the scrape list stays hand-maintained.
    metrics_targets_dir: Option<String>,
    /// T2: Grafana provisioning directory inside the gateway container.
    /// Absent = off, and dashboards stay hand-made.
    grafana_dashboards_dir: Option<String>,
}

#[derive(Clone)]
struct Config {
    token: String,
    listen: SocketAddr,
    state_dir: String,
    /// Path of the toml we loaded — SetConfig persists back to it.
    config_path: String,
    /// A6: remote exec endpoint switch. Deny-by-default; ssh-edited only
    /// (deliberately NOT in the G8 settings tab).
    exec_enabled: bool,
    /// Bearer token for the notification target, when it needs one.
    ///
    /// Deliberately here rather than in HostConfigView beside notify_webhook:
    /// that view is the settings the CLIENT can read back, and a secret does
    /// not belong in a screen. ssh-edited only, like exec_enabled.
    notify_auth_bearer: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    /// D5: git remote URL for the offsite intent mirror; None = off.
    mirror_remote: Option<String>,
    /// H2: OPNsense base url + credential file for Kea reservations.
    kea: Option<homelab_core::ops::kea::KeaCfg>,
    /// H1 (hardening): safety values configurable via host.toml so M5 can
    /// migrate the gateway / adjust the no-touch list without a release.
    /// Hardcoded DEFAULT_NO_TOUCH remains the default.
    safety: SafetyConfig,
    /// E8: declared ZFS replication jobs; empty = feature off.
    zfs_jobs: Vec<homelab_core::ops::zfs::ZfsJob>,
    /// Backup target and timeouts, resolved once from host.toml. Callers
    /// clone this and override only `tiers`.
    backup: homelab_core::ops::backup::BackupCfg,
    /// T1: where per-stack Prometheus discovery files are written.
    metrics_targets_dir: Option<String>,
    /// T2: Grafana's provisioning directory inside the gateway container.
    grafana_dashboards_dir: Option<String>,
    /// Initial mutable settings (live copy lives in AppState.settings).
    initial_settings: homelab_proto::HostConfigView,
}

fn load_config() -> Config {
    let path = std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "/etc/homelab/host.toml".into());
    let file: FileConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .unwrap_or_default();

    let token = std::env::var("HOMELAB_TOKEN")
        .ok()
        .or(file.token)
        .unwrap_or_default();
    if token.len() < 16 {
        eprintln!(
            "FATAL: token must be set (>=16 chars) via {} or HOMELAB_TOKEN",
            path
        );
        std::process::exit(1);
    }
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
    Config {
        token,
        listen,
        state_dir,
        config_path: path,
        exec_enabled: file.exec_enabled.unwrap_or(false),
        notify_auth_bearer: file.notify_auth_bearer.clone(),
        prometheus_url: file.prometheus_url.clone(),
        loki_url: file.loki_url.clone(),
        mirror_remote: file.mirror_remote,
        kea: match (file.opnsense_url, file.opnsense_cred_file) {
            (Some(base_url), Some(cred_file)) => Some(homelab_core::ops::kea::KeaCfg {
                base_url,
                cred_file,
            }),
            _ => None,
        },
        safety: {
            let mut sc = SafetyConfig::default();
            if let Some(list) = file.no_touch {
                sc.no_touch = list;
            }
            if let Some(gw) = file.gateway_vmid {
                sc.gateway_vmid = gw;
            }
            if let Some(dir) = file.gateway_routes_dir {
                sc.gateway_routes_dir = dir;
            }
            sc
        },
        zfs_jobs: file.zfs_jobs.unwrap_or_default(),
        backup: {
            let d = homelab_core::ops::backup::BackupCfg::default();
            homelab_core::ops::backup::BackupCfg {
                restic_base: file.restic_base.unwrap_or(d.restic_base),
                password_file: file.restic_password_file.unwrap_or(d.password_file),
                snapshot_timeout_s: file
                    .restic_snapshot_timeout_s
                    .unwrap_or(d.snapshot_timeout_s),
                restore_timeout_s: file.restic_restore_timeout_s.unwrap_or(d.restore_timeout_s),
                tiers: d.tiers,
            }
        },
        metrics_targets_dir: file.metrics_targets_dir,
        grafana_dashboards_dir: file.grafana_dashboards_dir,
        initial_settings: homelab_proto::HostConfigView {
            backup_hour: file.backup_hour,
            notify_webhook: file.notify_webhook,
            retention: file
                .retention
                .unwrap_or_else(homelab_core::retention::default_tiers),
        },
    }
}

/// G8: persist the mutable settings back to host.toml, atomically, keeping
/// the immutable fields (token/listen/state_dir) intact.
/// Render host.toml from the immutable config + mutable settings. Split out
/// of persist_settings so the parse→render→parse round-trip is testable
/// (gap: an early version silently dropped the OPNsense fields on every
/// settings save).
fn render_settings_toml(
    config: &Config,
    settings: &homelab_proto::HostConfigView,
) -> Result<String, String> {
    #[derive(serde::Serialize)]
    struct Out<'a> {
        token: &'a str,
        listen: String,
        state_dir: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        backup_hour: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notify_webhook: Option<&'a String>,
        retention: &'a [homelab_proto::RetentionTier],
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        exec_enabled: bool,
        // Written back for the same reason the OPNsense fields are: a
        // settings save that dropped this would silently stop every
        // notification, and the first thing you would not hear about is that.
        #[serde(skip_serializing_if = "Option::is_none")]
        notify_auth_bearer: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prometheus_url: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loki_url: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mirror_remote: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        opnsense_url: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        opnsense_cred_file: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        no_touch: Option<&'a Vec<u16>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway_vmid: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway_routes_dir: Option<&'a String>,
        #[serde(skip_serializing_if = "<[_]>::is_empty")]
        zfs_jobs: &'a [homelab_core::ops::zfs::ZfsJob],
        // Written back only when they differ from the compiled defaults, but
        // written back they must be: a settings save that drops them would
        // silently move the backup target, which is the same class of bug the
        // opnsense fields once had.
        #[serde(skip_serializing_if = "Option::is_none")]
        restic_base: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        restic_password_file: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        restic_snapshot_timeout_s: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        restic_restore_timeout_s: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        metrics_targets_dir: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        grafana_dashboards_dir: Option<&'a String>,
    }
    let bdef = homelab_core::ops::backup::BackupCfg::default();
    let out = Out {
        token: &config.token,
        listen: config.listen.to_string(),
        state_dir: &config.state_dir,
        backup_hour: settings.backup_hour,
        notify_webhook: settings.notify_webhook.as_ref(),
        retention: &settings.retention,
        exec_enabled: config.exec_enabled,
        notify_auth_bearer: config.notify_auth_bearer.as_ref(),
        prometheus_url: config.prometheus_url.as_ref(),
        loki_url: config.loki_url.as_ref(),
        mirror_remote: config.mirror_remote.as_ref(),
        opnsense_url: config.kea.as_ref().map(|k| &k.base_url),
        opnsense_cred_file: config.kea.as_ref().map(|k| &k.cred_file),
        no_touch: (config.safety.no_touch != SafetyConfig::default().no_touch)
            .then_some(&config.safety.no_touch),
        gateway_vmid: (config.safety.gateway_vmid != SafetyConfig::default().gateway_vmid)
            .then_some(config.safety.gateway_vmid),
        gateway_routes_dir: (config.safety.gateway_routes_dir
            != SafetyConfig::default().gateway_routes_dir)
            .then_some(&config.safety.gateway_routes_dir),
        zfs_jobs: &config.zfs_jobs,
        restic_base: (config.backup.restic_base != bdef.restic_base)
            .then_some(&config.backup.restic_base),
        restic_password_file: (config.backup.password_file != bdef.password_file)
            .then_some(&config.backup.password_file),
        restic_snapshot_timeout_s: (config.backup.snapshot_timeout_s != bdef.snapshot_timeout_s)
            .then_some(config.backup.snapshot_timeout_s),
        restic_restore_timeout_s: (config.backup.restore_timeout_s != bdef.restore_timeout_s)
            .then_some(config.backup.restore_timeout_s),
        metrics_targets_dir: config.metrics_targets_dir.as_ref(),
        grafana_dashboards_dir: config.grafana_dashboards_dir.as_ref(),
    };
    toml::to_string_pretty(&out).map_err(|e| e.to_string())
}

fn persist_settings(
    config: &Config,
    settings: &homelab_proto::HostConfigView,
) -> Result<(), String> {
    let raw = render_settings_toml(config, settings)?;
    let tmp = format!("{}.tmp", config.config_path);
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
    std::fs::rename(&tmp, &config.config_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: everything load_config understands must survive a
    /// settings save. Guards the bug where opnsense_url/opnsense_cred_file
    /// were silently dropped by the SETTINGS-tab save path (H2 dying on the
    /// next restart without any warning).
    #[test]
    fn settings_render_keeps_every_config_field() {
        let config = Config {
            token: "0123456789abcdef0123".into(),
            listen: "0.0.0.0:8443".parse().unwrap(),
            state_dir: "/var/lib/homelab".into(),
            config_path: "/etc/homelab/host.toml".into(),
            exec_enabled: true,
            notify_auth_bearer: Some("a-token-that-must-survive-a-save".into()),
            prometheus_url: Some("http://10.10.10.13:9090".into()),
            loki_url: Some("http://10.10.10.4:3100".into()),
            mirror_remote: Some("git@github.com:k/m.git".into()),
            kea: Some(homelab_core::ops::kea::KeaCfg {
                base_url: "https://10.10.10.1".into(),
                cred_file: "/var/lib/homelab/secrets/opnsense".into(),
            }),
            safety: SafetyConfig {
                no_touch: vec![100, 101],
                gateway_vmid: 112,
                gateway_routes_dir: "/appdata/platform/traefik-config/routes".into(),
            },
            zfs_jobs: vec![homelab_core::ops::zfs::ZfsJob {
                source: "HDD2TB".into(),
                target: "HDD18TB/REPLICA_2TB".into(),
            }],
            backup: homelab_core::ops::backup::BackupCfg {
                restic_base: "rclone:hdd:homelab-backups".into(),
                restore_timeout_s: 9_999,
                ..Default::default()
            },
            metrics_targets_dir: Some("/appdata/metrics/prometheus-config/targets".into()),
            grafana_dashboards_dir: Some("/opt/grafana/provisioning/dashboards".into()),
            initial_settings: homelab_proto::HostConfigView {
                backup_hour: Some(4),
                notify_webhook: Some("http://ha/webhook/x".into()),
                retention: homelab_core::retention::default_tiers(),
            },
        };
        let rendered = render_settings_toml(&config, &config.initial_settings).expect("render");
        let parsed: FileConfig = toml::from_str(&rendered).expect("parse back");
        assert_eq!(parsed.token.as_deref(), Some("0123456789abcdef0123"));
        // The notification bearer must survive a settings save. Dropping it
        // would stop every notification the host sends, and the first thing
        // you would not hear about is that.
        assert_eq!(
            parsed.notify_auth_bearer.as_deref(),
            Some("a-token-that-must-survive-a-save")
        );
        assert_eq!(parsed.backup_hour, Some(4));
        // E8: settings saves must not drop the zfs jobs (same class of bug
        // as the opnsense fields once had).
        assert_eq!(
            parsed.zfs_jobs.as_deref(),
            Some(
                &[homelab_core::ops::zfs::ZfsJob {
                    source: "HDD2TB".into(),
                    target: "HDD18TB/REPLICA_2TB".into(),
                }][..]
            )
        );
        assert_eq!(
            parsed.notify_webhook.as_deref(),
            Some("http://ha/webhook/x")
        );
        assert_eq!(parsed.exec_enabled, Some(true));
        // F39: a settings save must not silently move the backup target back
        // to the compiled default.
        assert_eq!(
            parsed.restic_base.as_deref(),
            Some("rclone:hdd:homelab-backups")
        );
        assert_eq!(parsed.restic_restore_timeout_s, Some(9_999));
        // Values left at the default stay out of the file rather than being
        // frozen into it.
        assert_eq!(parsed.restic_password_file, None);
        assert_eq!(parsed.restic_snapshot_timeout_s, None);
        assert_eq!(
            parsed.mirror_remote.as_deref(),
            Some("git@github.com:k/m.git")
        );
        assert_eq!(parsed.opnsense_url.as_deref(), Some("https://10.10.10.1"));
        assert_eq!(
            parsed.opnsense_cred_file.as_deref(),
            Some("/var/lib/homelab/secrets/opnsense")
        );
        assert_eq!(parsed.retention.as_ref().map(|r| r.len()), Some(3));
        assert_eq!(parsed.no_touch, Some(vec![100, 101]));
        assert_eq!(parsed.gateway_vmid, Some(112));
        assert_eq!(
            parsed.gateway_routes_dir.as_deref(),
            Some("/appdata/platform/traefik-config/routes")
        );
    }

    /// H8: the probe layer feeds real data — stale backups and a dead
    /// offsite remote must surface, healthy state must not.
    #[tokio::test]
    async fn doctor_probes_surface_stale_backup_and_dead_offsite() {
        use homelab_core::executor::{CmdOutput, MockExecutor};
        let now = 1_800_000_000u64;
        let exec = MockExecutor::new();
        exec.seed_file(
            "/var/lib/homelab/state.json",
            &format!(
                r#"{{"schema_version":1,"stacks":{{"synctest":{{"vmid":108,"hostname":"108-app-synctest","apps":["syncthing"],"applied_at":1,"last_backup":{}}}}}}}"#,
                now - 80 * 3600
            ),
        );
        exec.respond_always("pct status 108", CmdOutput::ok("status: running"));
        exec.respond_always(
            "listremotes",
            CmdOutput::ok(
                "gdrive:
",
            ),
        );
        exec.respond_always(
            "lsd gdrive:homelab-backups",
            CmdOutput::failed(3, "token expired"),
        );
        let probes = gather_probes(&exec, "/var/lib/homelab", None, now).await;
        assert_eq!(probes.managed_stacks.len(), 1);
        assert_eq!(probes.managed_stacks[0].backup_age_h, Some(80));
        assert!(probes.managed_stacks[0].container_present);
        assert!(probes.offsite_configured);
        assert!(!probes.offsite_token_valid, "expired token must show");
        // And the diagnosis flags both problems.
        let checks = homelab_core::doctor::diagnose(&probes);
        assert!(checks
            .iter()
            .any(|c| c.health != homelab_core::doctor::Health::Ok));
    }

    #[test]
    fn h12_scheduler_clock_logic() {
        // Weird `date` output never silently disables the scheduler.
        assert_eq!(parse_local_hour("04\n"), Some(4));
        assert_eq!(parse_local_hour("garbage"), None);
        assert_eq!(parse_local_hour("99"), None);
        let now = 1_800_000_000u64;
        assert!(backup_due(4, 4, now - 25 * 3600, now));
        assert!(!backup_due(4, 5, now - 25 * 3600, now), "wrong hour");
        assert!(!backup_due(4, 4, now - 3600, now), "backed up an hour ago");
        assert!(backup_due(4, 4, 0, now), "never backed up");
    }

    #[test]
    fn h10_nightly_plan_always_includes_host_meta() {
        // The bug this test was written for: the host-meta backup existed as
        // code but no scheduler path ever reached it, so the vault (holding
        // the ONLY copy of the restic password), state.json and the TLS
        // material were never backed up.
        let now = 1_800_000_000u64;
        let fresh = now - 3600; // backed up an hour ago
        let stale = now - 25 * 3600;

        // No stack is due — the host's own crown jewels still get a snapshot.
        let plan = nightly_plan(4, 4, now, &[("a".into(), true, fresh)], 0, now, false);
        assert_eq!(plan, vec![NightlyTask::HostMeta]);

        // Due stacks come first, host-meta closes the run.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[("a".into(), true, stale), ("b".into(), true, stale)],
            0,
            now,
            false,
        );
        assert_eq!(
            plan,
            vec![
                NightlyTask::Stack("a".into()),
                NightlyTask::Stack("b".into()),
                NightlyTask::HostMeta
            ]
        );

        // Already snapshotted this run — not repeated on the next 20-min tick.
        let plan = nightly_plan(4, 4, now, &[("a".into(), true, fresh)], fresh, now, false);
        assert!(plan.is_empty());

        // Wrong hour: nothing at all.
        assert!(nightly_plan(4, 5, now, &[("a".into(), true, stale)], 0, now, false).is_empty());

        // H8: a parked stack sits out, but the host-meta backup does not
        // depend on any stack being active.
        let plan = nightly_plan(4, 4, now, &[("a".into(), false, stale)], 0, now, false);
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
    }

    #[test]
    fn e8_zfs_only_when_configured() {
        let now = 1_800_000_000u64;
        let stale = now - 25 * 3600;
        // No jobs declared → the feature is simply off.
        let plan = nightly_plan(4, 4, now, &[], stale, stale, false);
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
        // Declared → runs once a night, after the host-meta snapshot.
        let plan = nightly_plan(4, 4, now, &[], stale, stale, true);
        assert_eq!(plan, vec![NightlyTask::HostMeta, NightlyTask::Zfs]);
        // Already ran this cycle → not repeated on the next 20-min tick.
        let plan = nightly_plan(4, 4, now, &[], stale, now - 3600, true);
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
    }

    #[test]
    fn h12_bearer_check() {
        assert!(bearer_ok(
            Some("Bearer secret-token-123"),
            "secret-token-123"
        ));
        assert!(!bearer_ok(Some("Bearer wrong"), "secret-token-123"));
        assert!(
            !bearer_ok(Some("secret-token-123"), "secret-token-123"),
            "scheme required"
        );
        assert!(!bearer_ok(None, "secret-token-123"));
    }

    #[test]
    fn h16_capacity_numbers_parse_and_sum() {
        let free = "               total        used        free\nMem:           15908        9911        1268\nSwap:           8191         512        7679\n";
        let mut hs = homelab_core::state::HostState::default();
        let mk = |mem: u32| {
            let mut m = homelab_core::manifest::StackManifest {
                registry_login: None,
                retention: None,
                data_mounts: Vec::new(),
                stack_name: "x".into(),
                vmid: 108,
                hostname: "108-app-x".into(),
                network: homelab_core::manifest::NetworkSpec {
                    ip: "10.10.10.8/24".into(),
                    gateway: "g".into(),
                    bridge: "b".into(),
                    vlan: None,
                },
                resources: homelab_core::manifest::ResourceSpec {
                    cores: 2,
                    memory_mb: mem,
                    swap_mb: 0,
                    disk_gb: 4,
                    storage: "s".into(),
                },
                lxc: homelab_core::manifest::LxcSpec {
                    template: "t".into(),
                    unprivileged: true,
                    features: String::new(),
                    protection: false,
                    gpu: false,
                    vpn: false,
                },
                boot: homelab_core::manifest::BootSpec {
                    onboot: true,
                    order: None,
                },
                storage: vec![],
                apps: vec![],
            };
            m.hostname = m.canonical_hostname();
            m
        };
        hs.stacks.insert(
            "a".into(),
            homelab_core::state::StackState {
                vmid: 108,
                hostname: "108-app-a".into(),
                apps: vec![],
                applied_at: 0,
                last_backup: 0,
                applied_hash: String::new(),
                manifest: Some(mk(1024)),
                enabled: true,
                native: None,
                natives: Vec::new(),
            },
        );
        hs.stacks.insert(
            "b".into(),
            homelab_core::state::StackState {
                vmid: 109,
                hostname: "109-app-b".into(),
                apps: vec![],
                applied_at: 0,
                last_backup: 0,
                applied_hash: String::new(),
                manifest: Some(mk(4096)),
                enabled: true,
                native: None,
                natives: Vec::new(),
            },
        );
        let (total, used, committed, cores, load1) =
            capacity_numbers(free, "12\n", "2.53 1.80 1.20 2/500 12345", &hs);
        assert_eq!(total, 15908);
        assert_eq!(used, 9911);
        assert_eq!(committed, 5120, "sum of manifest RAM ceilings");
        assert_eq!(cores, 12);
        assert_eq!(load1, 253);
    }
}

// ── Real executor (AR2) ─────────────────────────────────────────────────────

struct RealExecutor;

#[async_trait]
impl Executor for RealExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        // Transcript emission is handled by core's TracingExecutor inside the
        // pipeline; here we only trace at the log level for non-pipeline calls.
        let rendered = cmd.rendered();
        tracing::trace!("run {}", rendered);
        let fut = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();
        let out = tokio::time::timeout(Duration::from_secs(cmd.timeout_s), fut)
            .await
            .map_err(|_| CoreError::Timeout {
                rendered: rendered.clone(),
                seconds: cmd.timeout_s,
            })?
            .map_err(|e| CoreError::Other(format!("spawn {}: {}", rendered, e)))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(CmdOutput {
            stdout,
            stderr,
            code: out.status.code().unwrap_or(-1),
        })
    }

    /// Atomic by contract (AR4): write a temp file, fsync, rename over.
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        use std::os::unix::fs::PermissionsExt;
        let path = path.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), CoreError> {
            let p = std::path::Path::new(&path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::State(e.to_string()))?;
            }
            let tmp = format!("{}.tmp", path);
            {
                let mut f =
                    std::fs::File::create(&tmp).map_err(|e| CoreError::State(e.to_string()))?;
                f.write_all(content.as_bytes())
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.sync_all().map_err(|e| CoreError::State(e.to_string()))?;
            }
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
                .map_err(|e| CoreError::State(e.to_string()))?;
            std::fs::rename(&tmp, &path).map_err(|e| CoreError::State(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Other(e.to_string()))?
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| CoreError::State(format!("{}: {}", path, e)))
    }

    async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

// ── Sink + journal adapters ─────────────────────────────────────────────────

struct BroadcastSink {
    log_tx: broadcast::Sender<ServerMsg>,
}

impl Sink for BroadcastSink {
    fn emit(&self, event: PipelineEvent) {
        let msg = match event {
            PipelineEvent::Line { level, source, msg } => {
                tracing::info!(source = %source, "{}", msg);
                ServerMsg::Log {
                    level: level.into(),
                    source,
                    msg,
                }
            }
            PipelineEvent::StepStarted { op, step } => ServerMsg::Log {
                level: homelab_proto::LogLevel::Info,
                source: "HOST".into(),
                msg: format!("[sync][run ] {} :: {}", op, step),
            },
            PipelineEvent::StepFinished { op, step, changed } => ServerMsg::Log {
                level: homelab_proto::LogLevel::Info,
                source: "HOST".into(),
                msg: format!(
                    "[sync][exit] {} :: {} :: {}",
                    op,
                    step,
                    if changed { "changed" } else { "ok (no change)" }
                ),
            },
            PipelineEvent::Bytes {
                op,
                label,
                done,
                total,
            } => ServerMsg::Transfer {
                op,
                label,
                done,
                total,
            },
        };
        let _ = self.log_tx.send(msg);
    }
}

/// B5/AR13: append-only JSONL journal; "running" records land before a step
/// executes, so an interrupted operation is visible after restart.
struct FileJournal {
    path: String,
}

impl Journal for FileJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = serde_json::json!({"ts": ts, "op": op, "step": step, "status": status});
        if let Some(parent) = std::path::Path::new(&self.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Server ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    config: Config,
    log_tx: broadcast::Sender<ServerMsg>,
    op_lock: Arc<Mutex<()>>, // AR12: mutations strictly serial
    /// G8: live mutable settings (scheduler hour, webhook, retention).
    settings: Arc<std::sync::RwLock<homelab_proto::HostConfigView>>,
    /// H13: failure-repeat damping for F3 notifications.
    damper: Arc<std::sync::Mutex<homelab_core::notify::NotifyDamper>>,
}

/// B7: minimal sd_notify — tell systemd we're alive without pulling in a
/// crate. No-op when NOTIFY_SOCKET is unset (dev runs).
fn sd_notify(msg: &str) {
    if let Ok(sock) = std::env::var("NOTIFY_SOCKET") {
        let addr = if let Some(stripped) = sock.strip_prefix('@') {
            format!("\0{}", stripped)
        } else {
            sock
        };
        if let Ok(s) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = s.send_to(msg.as_bytes(), addr);
        }
    }
}

#[tokio::main]
async fn main() {
    // H5: the self-update gate runs `staged --selfcheck` before installing.
    // Prove we can execute at all and report our version, then exit.
    if std::env::args().any(|a| a == "--selfcheck") {
        println!("{}", VERSION);
        std::process::exit(0);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = load_config();
    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState {
        config: config.clone(),
        log_tx,
        op_lock: Arc::new(Mutex::new(())),
        settings: Arc::new(std::sync::RwLock::new(config.initial_settings.clone())),
        damper: Arc::new(std::sync::Mutex::new(
            homelab_core::notify::NotifyDamper::new(20 * 3600),
        )),
    };

    // AR13: surface any operation the previous run left mid-flight.
    let mut interrupted: Vec<String> = Vec::new();
    if let Ok(journal) = std::fs::read_to_string(format!("{}/journal.jsonl", config.state_dir)) {
        for (op, step) in homelab_core::incidents::interrupted_ops(&journal) {
            tracing::warn!(
                "interrupted operation '{}' at step '{}' — re-running it is safe (idempotent)",
                op,
                step
            );
            interrupted.push(format!("{} @ {}", op, step));
        }
    }

    // F3: boot notification — after a power cut or crash-restart, Home
    // Assistant hears that the daemon is back, which version runs, and
    // whether anything was left mid-flight. Delayed so the network is up.
    {
        let boot_state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let payload = serde_json::json!({
                "source": "homelab-host",
                "op": "host-online",
                "label": "boot",
                "ok": interrupted.is_empty(),
                "error": if interrupted.is_empty() { None } else {
                    Some(format!("interrupted: {}", interrupted.join("; ")))
                },
                "version": VERSION,
            })
            .to_string();
            notify_raw(&boot_state, &RealExecutor, payload).await;
        });
    }

    // E4: nightly scheduler — backups for every managed stack + auto-policy
    // updates, driven from state.json manifests (no client needed). Reads the
    // live settings each tick, so G8 edits apply without a restart.
    {
        let sched_state = state.clone();
        tokio::spawn(async move { scheduler_loop(sched_state).await });
        match config.initial_settings.backup_hour {
            Some(hour) => info!(
                "scheduler armed: daily backup + auto-updates at {:02}:00",
                hour
            ),
            None => info!("scheduler idle (backup_hour not set)"),
        }
    }

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(|| async { VERSION }))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state);

    // A4: TLS with a self-signed cert; the client pins this fingerprint.
    let (certs, fingerprint) =
        tls::ensure_cert(&config.state_dir, "homelab-host").expect("tls cert");
    info!(
        "homelab-host v{} listening on {} (TLS)",
        VERSION, config.listen
    );
    info!("TLS fingerprint SHA256:{}", fingerprint);
    let tls_config =
        axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.cert_pem, &certs.key_pem)
            .await
            .expect("load tls");

    // H5: accept the self-update only after surviving 5s of real serving —
    // a binary that binds and then dies must leave the marker for OnFailure.
    let marker = format!("{}/selfupdate.pending", config.state_dir);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if std::path::Path::new(&marker).exists() {
            let _ = std::fs::remove_file(&marker);
            info!("self-update accepted — now running v{}", VERSION);
        }
    });

    // B7: tell systemd we're ready, then feed its watchdog. If this loop
    // ever stops (deadlock/hang), systemd kills and restarts the daemon.
    sd_notify("READY=1");
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            sd_notify("WATCHDOG=1");
        }
    });

    axum_server::bind_rustls(config.listen, tls_config)
        .serve(app.into_make_service())
        .await
        .expect("serve");
}

/// H16: parse capacity numbers (C6) from `free -m`, `nproc` and
/// /proc/loadavg + committed RAM from the stored manifests. Pure, testable.
fn capacity_numbers(
    free_out: &str,
    nproc_out: &str,
    loadavg: &str,
    hs: &homelab_core::state::HostState,
) -> (u32, u32, u32, u16, u32) {
    let mem_line: Vec<u64> = free_out
        .lines()
        .find(|l| l.starts_with("Mem:"))
        .map(|l| {
            l.split_whitespace()
                .skip(1)
                .filter_map(|v| v.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let total = mem_line.first().copied().unwrap_or(0) as u32;
    let used = mem_line.get(1).copied().unwrap_or(0) as u32;
    let committed: u32 = hs
        .stacks
        .values()
        .filter_map(|s| s.manifest.as_ref())
        .map(|m| m.resources.memory_mb)
        .sum();
    let cores = nproc_out.trim().parse::<u16>().unwrap_or(0);
    let load1 = loadavg
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 100.0).round() as u32)
        .unwrap_or(0);
    (total, used, committed, cores, load1)
}

/// H12: pure scheduler decisions, extracted so the clock logic is testable.
/// `local_hour: None` (a failed/weird `date`) skips LOUDLY via the caller —
/// the old code folded it into 255 and silently never fired.
fn parse_local_hour(date_stdout: &str) -> Option<u8> {
    date_stdout.trim().parse::<u8>().ok().filter(|h| *h < 24)
}

fn backup_due(cfg_hour: u8, local_hour: u8, last_backup: u64, now: u64) -> bool {
    local_hour == cfg_hour && now.saturating_sub(last_backup) >= 20 * 3600
}

/// One unit of work in a nightly run.
#[derive(Debug, PartialEq, Eq)]
enum NightlyTask {
    /// Backup + auto-update this stack.
    Stack(String),
    /// H10: snapshot the host's own crown jewels (vault, state, TLS, intent
    /// repo). ALWAYS part of a nightly run, even when no stack is due —
    /// secrets change on deploys, not on backups.
    HostMeta,
    /// E8: ZFS snapshots + replication of the declared jobs.
    Zfs,
}

/// H12 pattern: the whole nightly decision as a pure function, so "does the
/// host-meta backup actually run?" is a test instead of an assumption.
/// `stacks` is (name, enabled, last_backup).
fn nightly_plan(
    cfg_hour: u8,
    local_hour: u8,
    now: u64,
    stacks: &[(String, bool, u64)],
    last_host_meta: u64,
    last_zfs: u64,
    zfs_configured: bool,
) -> Vec<NightlyTask> {
    let mut plan = Vec::new();
    for (name, enabled, last_backup) in stacks {
        // H8: parked stacks sit out the nightly rotation entirely.
        if *enabled && backup_due(cfg_hour, local_hour, *last_backup, now) {
            plan.push(NightlyTask::Stack(name.clone()));
        }
    }
    if backup_due(cfg_hour, local_hour, last_host_meta, now) {
        plan.push(NightlyTask::HostMeta);
    }
    if zfs_configured && backup_due(cfg_hour, local_hour, last_zfs, now) {
        plan.push(NightlyTask::Zfs);
    }
    plan
}

/// H12: bearer check, extracted for testing.
fn bearer_ok(header: Option<&str>, token: &str) -> bool {
    header
        .map(|v| v == format!("Bearer {}", token))
        .unwrap_or(false)
}

/// E4: check every 20 minutes; when the local hour matches `hour` and a
/// stack's last backup is >20h old, run backup (E1) then auto-updates (D9)
/// for that stack. Uses the same op machinery as RPCs (op-lock, incidents,
/// notifications), so a client-triggered deploy never overlaps.
async fn scheduler_loop(state: AppState) {
    let exec = RealExecutor;
    loop {
        tokio::time::sleep(Duration::from_secs(20 * 60)).await;
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
        let (hour, tiers) = {
            let s = state.settings.read().unwrap();
            match s.backup_hour {
                Some(h) => (h, s.retention.clone()),
                None => continue, // scheduler disabled
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Host-local hour without pulling in chrono: read from `date`.
        let local_hour = match exec.run(&Cmd::new("date", &["+%H"], 10)).await {
            Ok(out) => parse_local_hour(&out.stdout),
            Err(_) => None,
        };
        let Some(local_hour) = local_hour else {
            tracing::error!("scheduler: cannot determine local hour ('date' failed) — nightly run skipped THIS TICK; investigate");
            continue;
        };
        if local_hour != hour {
            continue;
        }
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let snapshot = match store.load().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("scheduler: state unreadable — skipping this tick: {}", e);
                continue;
            }
        };
        // H10: one plan per tick — stacks that are due, then the host's own
        // crown jewels. Decided by a pure function so it is unit-tested.
        let stack_inputs: Vec<(String, bool, u64)> = snapshot
            .stacks
            .iter()
            .map(|(n, st)| (n.clone(), st.enabled, st.last_backup))
            .collect();
        let plan = nightly_plan(
            hour,
            local_hour,
            now,
            &stack_inputs,
            snapshot.last_host_meta,
            snapshot.last_zfs,
            !state.config.zfs_jobs.is_empty(),
        );
        for (name, st) in snapshot.stacks {
            if !plan.contains(&NightlyTask::Stack(name.clone())) {
                if !st.enabled {
                    // H8: parked stack — no nightly backup, no auto-update.
                    info!("scheduler: stack {} is disabled — skipped", name);
                }
                continue;
            }
            // C7: native stacks get the in-container backup + supervised
            // self-update instead of the compose pair; same bookkeeping,
            // same H8 auto-disable on a failed night.
            if st.is_native() {
                info!(
                    "scheduler: nightly run for {} ({} native service(s))",
                    name,
                    st.natives.len()
                );
                // T5: several services share the container, so every one of
                // them is backed up and updated. One failure fails the night
                // for the stack — the H8 auto-disable below is deliberately
                // per stack, because they share a container and a fate.
                let mut backup_ok = true;
                let mut update_ok = true;
                for native in st.natives.clone() {
                    let n1 = native.clone();
                    let cfg = homelab_core::ops::backup::BackupCfg {
                        tiers: tiers.clone(),
                        ..state.config.backup.clone()
                    };
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-backup-native", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::backup_native(ctx, &n1, &cfg).await
                        })
                    })
                    .await;
                    backup_ok &= r.ok;
                    let n2 = native.clone();
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-update-native", |ctx| {
                        Box::pin(
                            async move { homelab_core::ops::native::update_native(ctx, &n2).await },
                        )
                    })
                    .await;
                    update_ok &= r.ok;
                }
                if backup_ok {
                    if let Ok(mut s) = store.load().await {
                        if let Some(rec) = s.stacks.get_mut(&name) {
                            rec.last_backup = now;
                        }
                        let _ = store.save(s).await;
                    }
                }
                if !backup_ok || !update_ok {
                    let mut parked = false;
                    if let Ok(mut s) = store.load().await {
                        if let Some(rec) = s.stacks.get_mut(&name) {
                            if rec.enabled {
                                rec.enabled = false;
                                parked = true;
                                tracing::warn!(
                                    "scheduler: nightly run for {} FAILED — stack auto-disabled (H8); investigate, then re-enable with `homelab enable {}`",
                                    name, name
                                );
                            }
                        }
                        let _ = store.save(s).await;
                    }
                    if parked {
                        notify_auto_disabled(
                            &state,
                            &exec,
                            &name,
                            "nightly run failed — stack parked (H8): no backup, no update, no onboot until re-enabled",
                        )
                        .await;
                    }
                }
                continue;
            }
            let Some(manifest) = st.manifest else {
                tracing::warn!("scheduler: stack {} has no stored manifest — skipped", name);
                continue;
            };
            info!("scheduler: nightly run for {}", name);
            let m1 = manifest.clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            let backup_report = run_mutating_op(&state, &exec, 0, "scheduled-backup", |ctx| {
                Box::pin(async move { homelab_core::ops::backup::backup(ctx, &m1, &cfg).await })
            })
            .await;
            if backup_report.ok {
                // Record last_backup so tomorrow's check is accurate.
                if let Ok(mut s) = store.load().await {
                    if let Some(rec) = s.stacks.get_mut(&name) {
                        rec.last_backup = now;
                    }
                    let _ = store.save(s).await;
                }
            }
            let m2 = manifest.clone();
            let update_report = run_mutating_op(&state, &exec, 0, "scheduled-update", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::update::update(ctx, &m2, None, true).await },
                )
            })
            .await;
            // H8: a failed nightly run flips the flag off — one loud message,
            // then silence instead of a fresh failure every night. State-only:
            // onboot and the running containers are untouched, so a transient
            // failure can never keep a stack from surviving a host reboot.
            if !backup_report.ok || !update_report.ok {
                let mut parked = false;
                if let Ok(mut s) = store.load().await {
                    if let Some(rec) = s.stacks.get_mut(&name) {
                        if rec.enabled {
                            rec.enabled = false;
                            parked = true;
                            tracing::warn!(
                                "scheduler: nightly run for {} FAILED — stack auto-disabled (H8); investigate, then re-enable with `homelab enable {}`",
                                name, name
                            );
                        }
                    }
                    let _ = store.save(s).await;
                }
                if parked {
                    notify_auto_disabled(
                        &state,
                        &exec,
                        &name,
                        "nightly run failed — stack parked (H8): no backup, no update, no onboot until re-enabled",
                    )
                    .await;
                }
            }
        }

        // H10: the host's own crown jewels — the secrets vault (holding the
        // only copy of the restic password), state.json, TLS material and the
        // intent repo. Runs even when no stack was due; without it, losing the
        // host disk loses the keys to every backup we ever made.
        if plan.contains(&NightlyTask::HostMeta) {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..state.config.backup.clone()
            };
            let report = run_mutating_op(&state, &exec, 0, "host-meta-backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await },
                )
            })
            .await;
            if report.ok {
                if let Ok(mut s) = store.load().await {
                    s.last_host_meta = now;
                    let _ = store.save(s).await;
                }
            } else {
                tracing::error!(
                    "scheduler: host-meta backup FAILED — the vault/state/TLS snapshot is the recovery path for a lost host disk; investigate now"
                );
            }
        }

        // E8: ZFS snapshots + replication of the big pools.
        if plan.contains(&NightlyTask::Zfs) {
            let jobs = state.config.zfs_jobs.clone();
            let report = run_mutating_op(&state, &exec, 0, "zfs-replicate", |ctx| {
                Box::pin(async move { homelab_core::ops::zfs::replicate(ctx, &jobs, &tiers).await })
            })
            .await;
            if report.ok {
                if let Ok(mut s) = store.load().await {
                    s.last_zfs = now;
                    let _ = store.save(s).await;
                }
            } else {
                tracing::error!("scheduler: ZFS replication FAILED — investigate; the old cron script used to fail silently, this one does not");
            }
        }

        // Y4: after the night's work, hold the record against the machine.
        // Unconditional, because the findings this exists for are precisely
        // the ones that produce no failure of their own: a stack whose
        // hostname drifted, a backup that quietly stopped, a route that leads
        // nowhere. Stack-file facts are the client's to supply, so the
        // nightly pass runs without them.
        if !plan.is_empty() {
            let live = gather_live_facts(&exec, &state, &[]).await;
            if let Ok(snapshot) = store.load().await {
                let findings = homelab_core::ops::fleetcheck::evaluate(
                    &snapshot,
                    &live,
                    now,
                    homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
                    homelab_core::ops::fleetcheck::GrowthLimits::default(),
                );
                if findings.is_empty() {
                    info!("fleet check: repo and reality agree");
                } else {
                    tracing::warn!(
                        "fleet check: {} finding(s)\n{}",
                        findings.len(),
                        render_findings(&findings)
                    );
                    // The finding text is already in the log above; the
                    // webhook exists so it leaves the machine.
                    notify_raw(
                        &state,
                        &exec,
                        serde_json::json!({
                            "op": "fleet-check",
                            "ok": false,
                            "error": render_findings(&findings),
                            "version": env!("CARGO_PKG_VERSION"),
                        })
                        .to_string(),
                    )
                    .await;
                }
            }
        }
    }
}

async fn ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authed = bearer_ok(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
    );
    if !authed {
        return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
    }
    ws.on_upgrade(move |socket| ws_session(socket, state))
        .into_response()
}

async fn ws_session(socket: WebSocket, state: AppState) {
    let (mut tx, mut rx) = socket.split();
    let hello = ServerMsg::Hello {
        version: VERSION.into(),
        proto: homelab_proto::PROTO_VERSION,
    };
    let _ = tx
        .send(Message::Text(serde_json::to_string(&hello).unwrap()))
        .await;

    let mut log_rx = state.log_tx.subscribe();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(msg) = log_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() { break; }
                }
                Some(msg) = out_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() { break; }
                }
                else => break,
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = rx.next().await {
        let Ok(req) = serde_json::from_str::<RpcRequest>(&text) else {
            continue;
        };
        let resp = handle_rpc(&state, req).await;
        let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
    }
    forward.abort();
}

/// D5: push the intent repo to the offsite mirror, detached — a failing
/// push logs and is retried after the next operation (and by the periodic
/// tick), never blocking the operation that triggered it.
fn spawn_mirror_push(state: &AppState) {
    let Some(remote) = state.config.mirror_remote.clone() else {
        return;
    };
    let repo = format!("{}/repo", state.config.state_dir);
    tokio::spawn(async move {
        if let Err(e) = homelab_core::ops::mirror::mirror_push(&RealExecutor, &repo, &remote).await
        {
            tracing::warn!("mirror push failed (will retry): {}", e);
        }
    });
}

/// A stack has just been parked by H8, which is the moment it stops being
/// protected — no nightly backup, no update, no onboot.
///
/// It used to be a `tracing::warn!` and nothing else. On 2026-08-31 the
/// metrics stack parked itself after the run that stopped Alertmanager, and
/// it stayed out of every nightly protection until Kenny happened to ask why
/// a dashboard was empty. A stack silently losing its safety net is precisely
/// the class of silence this project exists to remove, so it now reaches him
/// the same way a failed operation does.
///
/// The op name carries the stack, matching the convention the damper relies
/// on: two stacks parking on the same night are two notifications, not one.
async fn notify_auto_disabled(state: &AppState, exec: &RealExecutor, stack: &str, why: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let op = format!("stack-disabled-{}", stack);
    if !state
        .damper
        .lock()
        .unwrap()
        .should_send(&op, false, Some(why), now)
    {
        return;
    }
    let payload = homelab_core::notify::op_payload(&op, stack, false, Some(why));
    notify_raw(state, exec, payload).await;
}

/// F3: best-effort webhook to Home Assistant after every mutating operation.
/// Runs through the executor (curl) so it is visible in traces and never
/// blocks or fails the operation itself.
async fn notify(
    state: &AppState,
    exec: &RealExecutor,
    label: &str,
    report: &homelab_core::runner::OperationReport,
) {
    let error = report
        .error
        .as_ref()
        .map(|e| format!("{} :: {}", e.what, e.why));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // H13: identical repeat failures inside the window are damped.
    if !state
        .damper
        .lock()
        .unwrap()
        .should_send(&report.op, report.ok, error.as_deref(), now)
    {
        return;
    }
    let payload = homelab_core::notify::op_payload(&report.op, label, report.ok, error.as_deref());
    notify_raw(state, exec, payload).await;
}

/// Lower-level webhook POST used by notify() and the boot notification.
async fn notify_raw(state: &AppState, exec: &RealExecutor, payload: String) {
    let url = match state.settings.read().unwrap().notify_webhook.clone() {
        Some(u) => u,
        None => return,
    };
    let bearer = state.config.notify_auth_bearer.clone();
    let auth = bearer.map(|t| format!("authorization: Bearer {}", t));
    let mut args: Vec<&str> = vec![
        "-m",
        "5",
        "-s",
        "-o",
        "/dev/null",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
    ];
    if let Some(a) = auth.as_deref() {
        args.push("-H");
        args.push(a);
    }
    args.push("-d");
    args.push(&payload);
    args.push(&url);
    let _ = exec.run(&Cmd::new("curl", &args, 10)).await;
}

/// Run any mutating operation under the op-lock (AR12) with uniform incident
/// bundling on failure (AR14). The closure receives the OpCtx and returns the
/// OperationReport.
async fn run_mutating_op<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    let _guard = state.op_lock.lock().await;
    let broadcast = BroadcastSink {
        log_tx: state.log_tx.clone(),
    };
    let sink = homelab_core::incidents::RecordingSink::new(&broadcast);
    let journal = FileJournal {
        path: format!("{}/journal.jsonl", state.config.state_dir),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ctx = OpCtx {
        exec,
        sink: &sink,
        journal: &journal,
        safety: state.config.safety.clone(),
        state_dir: state.config.state_dir.clone(),
        now_unix: now,
        kea: state.config.kea.clone(),
        metrics_targets_dir: state.config.metrics_targets_dir.clone(),
        grafana_dashboards_dir: state.config.grafana_dashboards_dir.clone(),
        backup: state.config.backup.clone(),
    };
    let report = op(&ctx).await;
    notify(state, exec, label, &report).await; // F3, best-effort
    if report.ok {
        spawn_mirror_push(state); // D5, best-effort + detached
    }
    if report.ok {
        RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "{} complete — {} step(s), {} changed",
                label,
                report.steps.len(),
                report.steps.iter().filter(|s| s.changed).count()
            ),
        }
    } else {
        let err = report
            .error
            .clone()
            .unwrap_or(homelab_core::error::OperatorError {
                what: format!("{} failed", label),
                why: "unknown".into(),
                remedy: "see transcript".into(),
            });
        error!("{} failed: {} — {}", label, err.what, err.why);
        let versions = format!("host={}\nproto={}\n", VERSION, homelab_proto::PROTO_VERSION);
        let bundle = homelab_core::incidents::write_bundle(
            exec,
            &state.config.state_dir,
            now,
            &report,
            &sink.events(),
            &versions,
        )
        .await;
        let bundle_note = match bundle {
            Ok(dir) => format!(" :: incident bundle {}", dir),
            Err(e) => format!(" :: (bundle write failed: {})", e),
        };
        RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "{} :: {} :: remedy: {}{}",
                err.what, err.why, err.remedy, bundle_note
            ),
        }
    }
}

/// C7: look up an adopted native stack's manifest in state. Error strings
/// carry the remedy, per standing rule 11.
async fn native_from_state(
    state_dir: &str,
    stack: &str,
) -> Result<Vec<homelab_core::native::NativeServiceManifest>, String> {
    let store = homelab_core::state::StateStore::new(&RealExecutor, state_dir);
    let snapshot = store
        .load()
        .await
        .map_err(|e| format!("state unreadable: {}", e))?;
    match snapshot.stacks.get(stack) {
        Some(st) if st.is_native() => Ok(st.natives.clone()),
        Some(_) => Err(format!(
            "stack '{}' is a compose stack, not a native service :: use the regular backup/update verbs",
            stack
        )),
        None => Err(format!(
            "stack '{}' is not in host state :: adopt it first (homelab adopt stacks/{})",
            stack, stack
        )),
    }
}

/// Y4: read off the machine what the pure comparison needs. Kept separate so
/// the judgement stays testable without a fleet.
async fn gather_live_facts(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
) -> homelab_core::ops::fleetcheck::LiveFacts {
    use homelab_core::executor::{Cmd, Executor};
    let mut facts = homelab_core::ops::fleetcheck::LiveFacts {
        stack_files: stack_files.to_vec(),
        ..Default::default()
    };
    if let Ok(out) = exec.run(&Cmd::new("pct", &["list"], 30)).await {
        for line in out.stdout.lines().skip(1) {
            let mut cols = line.split_whitespace();
            if let (Some(vmid), Some(_status)) = (cols.next(), cols.next()) {
                if let (Ok(vmid), Some(name)) = (vmid.parse::<u16>(), cols.last()) {
                    facts.containers.push((vmid, name.to_string()));
                }
            }
        }
    }
    // Gateway routes: read every fragment, pull out the address it forwards
    // to, and ask whether anything is listening there. A route that resolves
    // to nothing is only ever found by someone who needs it.
    let gw = state.config.safety.gateway_vmid.to_string();
    let dir = &state.config.safety.gateway_routes_dir;
    let script = format!(
        "for f in {}/*.yml; do echo \"### $(basename $f)\"; cat \"$f\"; done 2>/dev/null",
        dir
    );
    if let Ok(out) = exec
        .run(&Cmd::new(
            "pct",
            &["exec", &gw, "--", "sh", "-c", &script],
            60,
        ))
        .await
    {
        let mut current = String::new();
        for line in out.stdout.lines() {
            if let Some(name) = line.strip_prefix("### ") {
                current = name.to_string();
                continue;
            }
            let t = line.trim();
            let target = t
                .strip_prefix("- url:")
                .or_else(|| t.strip_prefix("- address:"))
                .map(|v| v.trim().trim_matches('"').to_string());
            if let Some(target) = target {
                let hostport =
                    homelab_core::ops::fleetcheck::probe_hostport(target.trim_matches('"'));
                // bash, not sh: /dev/tcp is a bash feature and the shell in
                // these containers is dash, which reports "Directory
                // nonexistent" for every address. The first run of this check
                // called every route in the house dead, Jellyfin included —
                // a check that always fires is worse than none, because it
                // teaches you to stop reading it.
                let probe = format!(
                    "timeout 3 bash -c 'echo > /dev/tcp/{}' 2>/dev/null && echo up || echo down",
                    hostport
                );
                let answered = exec
                    .run(&Cmd::new(
                        "pct",
                        &["exec", &gw, "--", "sh", "-c", &probe],
                        15,
                    ))
                    .await
                    .map(|o| o.stdout.contains("up"))
                    .unwrap_or(false);
                facts.routes.push(homelab_core::ops::fleetcheck::RouteFact {
                    file: current.clone(),
                    target,
                    answered,
                });
            }
        }
    }

    // G3: what each managed container's resources look like right now.
    //
    // One `pct exec` per container emitting key=value lines, rather than six
    // round trips each. Every value has a fallback of 0 so a container that
    // answers half the questions still reports the half it knows — a check
    // that gives up on partial data is a check nobody trusts.
    const PROBE: &str = concat!(
        "df -P / | awk 'NR==2{gsub(\"%\",\"\",$5); print \"disk=\"$5}'; ",
        "free -m | awk '/^Mem:/{if($2>0) printf \"mem=%d\\n\", ($3*100)/$2} /^Swap:/{print \"swap=\"$3}'; ",
        "echo \"journal=$(du -sm /var/log/journal 2>/dev/null | cut -f1)\"; ",
        "echo \"dockerlogs=$(du -sm /var/lib/docker/containers 2>/dev/null | cut -f1)\"; ",
        // Both halves of the guard must be present. Checking only one is how
        // a half-guarded container reads as guarded.
        "if ls /etc/systemd/journald.conf.d/*.conf >/dev/null 2>&1 && ",
        "grep -q max-size /etc/docker/daemon.json 2>/dev/null; ",
        "then echo guards=1; else echo guards=0; fi"
    );
    // Every container on the hypervisor except the untouchable ones — not
    // only the stacks this orchestrator has adopted.
    //
    // The first version read host state instead, which is the defensible
    // choice for anything that ACTS. It is the wrong one for a check that
    // only looks: it covered 5 of the 9 containers here, and the four it
    // could not see (104, 105, 106, 111) are the oldest and fullest on the
    // machine. That is the same blind spot as the guards themselves —
    // a safeguard that quietly applies to almost nothing.
    //
    // The no-touch list is still honoured absolutely, so the report never
    // invites action on a guest that is out of bounds.
    let no_touch = &state.config.safety.no_touch;
    for (vmid, hostname) in facts
        .containers
        .iter()
        .filter(|(v, _)| !no_touch.contains(v))
        .map(|(v, h)| (*v, h.clone()))
        .collect::<Vec<_>>()
    {
        let vs = vmid.to_string();
        let Ok(out) = exec
            .run(&Cmd::new(
                "pct",
                &["exec", &vs, "--", "sh", "-c", PROBE],
                45,
            ))
            .await
        else {
            continue;
        };
        let mut g = homelab_core::ops::fleetcheck::GrowthFact {
            vmid,
            hostname,
            ..Default::default()
        };
        // Did the probe actually run inside the container? The `guards` line
        // is unconditional, so its absence means the shell never got there —
        // a stopped guest, a template, an exec that failed.
        let mut probed = false;
        for line in out.stdout.lines() {
            let Some((k, v)) = line.trim().split_once('=') else {
                continue;
            };
            match k {
                "disk" => g.disk_used_pct = v.parse().unwrap_or(0),
                "mem" => g.mem_used_pct = v.parse().unwrap_or(0),
                "swap" => g.swap_used_mb = v.parse().unwrap_or(0),
                "journal" => g.journal_mb = v.parse().unwrap_or(0),
                "dockerlogs" => g.docker_logs_mb = v.parse().unwrap_or(0),
                "guards" => {
                    g.guards = v == "1";
                    probed = true;
                }
                _ => {}
            }
        }
        // Not examined is not the same as examined and found wanting. The
        // first live run of the widened check reported the three golden
        // templates (997, 998, 999) as having no runaway guards. They have
        // them — baked in at build time — but they are stopped, so `pct
        // exec` produced nothing and every field kept its zero default,
        // which read as an unguarded container. A check that invents a
        // finding out of a failed measurement is worse than one that misses:
        // it spends the reader's trust to say something false.
        if probed {
            facts.growth.push(g);
        }
    }

    // W3: the configured shape of every managed container, from `pct config`
    // rather than from inside it — a container that does not start on boot is
    // exactly the one you find stopped after the reboot that should have
    // started it, and `pct exec` cannot ask a stopped guest anything.
    for (vmid, hostname) in facts
        .containers
        .iter()
        .filter(|(v, _)| !no_touch.contains(v))
        .map(|(v, h)| (*v, h.clone()))
        .collect::<Vec<_>>()
    {
        let vs = vmid.to_string();
        let Ok(out) = exec.run(&Cmd::new("pct", &["config", &vs], 30)).await else {
            continue;
        };
        if !out.success() {
            continue;
        }
        facts.boot.push(homelab_core::ops::fleetcheck::BootFact {
            vmid,
            hostname,
            live: homelab_core::ops::reconcile::parse(&out.stdout),
        });
    }

    // Is each stack's safety net actually attached? The most expensive class
    // of failure here is not a service falling over, it is a mechanism that
    // runs, reports success and is wired to nothing — see CoverageFact.
    //
    // Both questions are skipped when their address is not configured. An
    // unasked question must never become a finding: that is how a check earns
    // the right to be believed.
    let prom = state.config.prometheus_url.clone();
    let loki = state.config.loki_url.clone();
    if prom.is_some() || loki.is_some() {
        if let Ok(snapshot) =
            homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir)
                .load()
                .await
        {
            for (name, st) in &snapshot.stacks {
                let mut c = homelab_core::ops::fleetcheck::CoverageFact {
                    stack: name.clone(),
                    ..Default::default()
                };
                if let Some(base) = prom.as_deref() {
                    let q = format!(
                        "{}/api/v1/query?query=max(up%7Bstack%3D%22{}%22%7D)",
                        base.trim_end_matches('/'),
                        name
                    );
                    c.scraped = Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"1\""))
                            .unwrap_or(false),
                    );
                }
                // Only ask about logs where logs are expected. A native
                // service with no promtail ships none by design, and a
                // finding it can never clear is worse than no finding.
                let ships_logs = st
                    .manifest
                    .as_ref()
                    .map(|m| m.apps.iter().any(|a| a == "promtail"))
                    .unwrap_or(false);
                if let (Some(base), true) = (loki.as_deref(), ships_logs) {
                    let q = format!(
                        "{}/loki/api/v1/query?query=sum(count_over_time(%7Bstack%3D%22{}%22%7D%5B1h%5D))",
                        base.trim_end_matches('/'),
                        name
                    );
                    c.logs_recent = Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"value\""))
                            .unwrap_or(false),
                    );
                }
                facts.coverage.push(c);
            }
        }
    }
    facts
}

/// A backup is a backup, whoever asked for it. The scheduler recorded
/// `last_backup` and the on-demand paths did not, so a stack backed up by
/// hand still read as never backed up — which is exactly what the fleet check
/// reported about kyu minutes after I had backed it up myself.
async fn record_backup_time(state: &AppState, stack: &str) {
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    if let Ok(mut s) = store.load().await {
        if let Some(rec) = s.stacks.get_mut(stack) {
            rec.last_backup = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = store.save(s).await;
        }
    }
}

fn render_findings(findings: &[homelab_core::ops::fleetcheck::Finding]) -> String {
    use homelab_core::ops::fleetcheck::Severity;
    if findings.is_empty() {
        return "fleet check: repo and reality agree".into();
    }
    let mut s = format!("fleet check: {} finding(s)\n", findings.len());
    for f in findings {
        s.push_str(&format!(
            "  [{}] {} — {}\n      remedy: {}\n",
            match f.severity {
                Severity::Broken => "broken",
                Severity::Drift => "drift",
            },
            f.subject,
            f.what,
            f.remedy
        ));
    }
    s
}

async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor;
    match req.command {
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
        },
        Rpc::Status => {
            let out = exec.run(&Cmd::new("pct", &["list"], 30)).await;
            let listing = out.map(|o| o.stdout).unwrap_or_else(|e| e.to_string());
            let managed = exec
                .read_file(&format!("{}/state.json", state.config.state_dir))
                .await
                .unwrap_or_else(|_| "{}".into());
            RpcResponse {
                id: req.id,
                ok: true,
                message: format!("pct list:\n{}\nmanaged state:\n{}", listing, managed),
            }
        }
        Rpc::DeployStack(spec) => {
            run_mutating_op(state, &exec, req.id, "deploy", |ctx| {
                Box::pin(async move { deploy(ctx, &spec).await })
            })
            .await
        }
        Rpc::DestroyStack { manifest, confirm } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy(
                        ctx,
                        &manifest.stack_name,
                        manifest.vmid,
                        &confirm,
                    )
                    .await
                })
            })
            .await
        }
        Rpc::BackupStack(manifest) => {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: state.settings.read().unwrap().retention.clone(),
                ..state.config.backup.clone()
            };
            let stack = manifest.stack_name.clone();
            let resp = run_mutating_op(state, &exec, req.id, "backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await },
                )
            })
            .await;
            if resp.ok {
                record_backup_time(state, &stack).await;
            }
            resp
        }
        Rpc::RestoreStack { manifest, snapshot } => {
            // The configured target and timeout, not the compiled defaults:
            // this is the path where a hardcoded 1800 s used to kill a large
            // restore over Google Drive at thirty minutes (F38).
            let cfg = state.config.backup.clone();
            run_mutating_op(state, &exec, req.id, "restore", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::backup::restore(ctx, &manifest, &cfg, &snapshot).await
                })
            })
            .await
        }
        Rpc::UpdateStack { manifest, app } => {
            run_mutating_op(state, &exec, req.id, "update", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::update::update(ctx, &manifest, app.as_deref(), false).await
                })
            })
            .await
        }
        Rpc::PatchFleet => {
            // Targets come from state.json — only stacks we deployed.
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await.unwrap_or_default();
            let targets: Vec<(String, u16)> = snapshot
                .stacks
                .iter()
                .map(|(name, st)| (name.clone(), st.vmid))
                .collect();
            run_mutating_op(state, &exec, req.id, "patch", |ctx| {
                Box::pin(async move { homelab_core::ops::patch::patch_fleet(ctx, &targets).await })
            })
            .await
        }
        Rpc::ApplyResources(manifest) => {
            run_mutating_op(state, &exec, req.id, "resize", |ctx| {
                Box::pin(async move { homelab_core::ops::resize::hot_apply(ctx, &manifest).await })
            })
            .await
        }
        Rpc::BackupHostMeta => {
            let tiers = state.settings.read().unwrap().retention.clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers,
                ..state.config.backup.clone()
            };
            let resp = run_mutating_op(state, &exec, req.id, "host-meta-backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await },
                )
            })
            .await;
            if resp.ok {
                let store =
                    homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
                if let Ok(mut s) = store.load().await {
                    s.last_host_meta = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = store.save(s).await;
                }
            }
            resp
        }
        Rpc::AdoptService(m) => {
            run_mutating_op(state, &exec, req.id, "adopt", |ctx| {
                Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
            })
            .await
        }
        Rpc::BackupNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok(services) => {
                    let tiers = state.settings.read().unwrap().retention.clone();
                    let cfg = homelab_core::ops::backup::BackupCfg {
                        tiers,
                        ..state.config.backup.clone()
                    };
                    // T5: the stack may hold several services; back up each,
                    // and report the first failure rather than the last.
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                    };
                    for m in services {
                        let cfg = cfg.clone();
                        let r = run_mutating_op(state, &exec, req.id, "backup-native", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::backup_native(ctx, &m, &cfg).await
                            })
                        })
                        .await;
                        let failed = !r.ok;
                        if resp.ok || failed {
                            resp = r;
                        }
                        if failed {
                            break;
                        }
                    }
                    if resp.ok {
                        record_backup_time(state, &stack).await;
                    }
                    resp
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                },
            }
        }
        Rpc::UpdateNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok(services) => {
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                    };
                    for m in services {
                        let r = run_mutating_op(state, &exec, req.id, "update-native", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::update_native(ctx, &m).await
                            })
                        })
                        .await;
                        let failed = !r.ok;
                        if resp.ok || failed {
                            resp = r;
                        }
                        if failed {
                            break;
                        }
                    }
                    resp
                }
                Err(msg) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: msg,
                },
            }
        }
        Rpc::ForgetStack { stack } => {
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let mut snapshot = match store.load().await {
                Ok(s) => s,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: format!("state unreadable: {}", e),
                    }
                }
            };
            let Some(entry) = snapshot.stacks.get(&stack).cloned() else {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("no stack '{}' in host state", stack),
                };
            };
            // The safety rule: only a record whose container no longer
            // answers to that hostname may be forgotten. A live one being
            // forgotten would go silently unbacked-up.
            let live = exec
                .run(&homelab_core::executor::Cmd::new("pct", &["list"], 30))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            if live
                .lines()
                .any(|l| l.split_whitespace().any(|w| w == entry.hostname))
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "'{}' still names a live container ({}) :: this record is current, not stale — destroy the stack or rename it first",
                        stack, entry.hostname
                    ),
                };
            }
            snapshot.stacks.remove(&stack);
            match store.save(snapshot).await {
                Ok(()) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "forgot '{}' (was vmid {} as {}) — the container was not touched",
                        stack, entry.vmid, entry.hostname
                    ),
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("could not write state: {}", e),
                },
            }
        }
        Rpc::ApplyGuards { vmid } => {
            // A1 still governs: the guards write files and restart docker, so
            // an untouchable guest is untouchable here too.
            if state.config.safety.no_touch.contains(&vmid) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "vmid {} is on the no-touch list :: this list is the one thing that is never worked around",
                        vmid
                    ),
                };
            }
            run_mutating_op(state, &exec, req.id, "apply-guards", |ctx| {
                Box::pin(async move {
                    let mut runner =
                        homelab_core::runner::Runner::new("apply-guards", ctx.sink, ctx.journal);
                    match runner
                        .step("guards", || async {
                            homelab_core::ops::guards::apply(ctx.exec, ctx.sink, vmid).await?;
                            Ok(homelab_core::runner::StepOutcome::Changed)
                        })
                        .await
                    {
                        Ok(_) => {
                            runner.log(
                                homelab_core::sink::Level::Info,
                                format!(
                                    "[guards] {} — docker log caps, journald limits, logrotate, apt autoclean, weekly prune",
                                    vmid
                                ),
                            );
                            runner.finish_ok()
                        }
                        Err(e) => runner.finish_err("guards", &e),
                    }
                })
            })
            .await
        }
        Rpc::FleetCheck { stack_files } => {
            let live = gather_live_facts(&exec, state, &stack_files).await;
            let snapshot =
                match homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir)
                    .load()
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: format!("state unreadable: {}", e),
                        }
                    }
                };
            let findings = homelab_core::ops::fleetcheck::evaluate(
                &snapshot,
                &live,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
                homelab_core::ops::fleetcheck::GrowthLimits::default(),
            );
            RpcResponse {
                id: req.id,
                ok: findings.is_empty(),
                message: render_findings(&findings),
            }
        }
        Rpc::ZfsReplicate => {
            let tiers = state.settings.read().unwrap().retention.clone();
            let jobs = state.config.zfs_jobs.clone();
            let resp = run_mutating_op(state, &exec, req.id, "zfs-replicate", |ctx| {
                Box::pin(async move { homelab_core::ops::zfs::replicate(ctx, &jobs, &tiers).await })
            })
            .await;
            if resp.ok {
                let store =
                    homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
                if let Ok(mut s) = store.load().await {
                    s.last_zfs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = store.save(s).await;
                }
            }
            resp
        }
        Rpc::SetStackEnabled { stack, enabled } => {
            run_mutating_op(state, &exec, req.id, "set-enabled", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::enable::set_enabled(ctx, &stack, enabled).await
                })
            })
            .await
        }
        Rpc::ListTemplates => {
            // C5: discovery instead of hardcoded strings. Two sources: OS
            // tarballs (pveam) and clonable golden template containers.
            let tarballs = exec
                .run(&Cmd::new("pveam", &["list", "local"], 60))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let clones = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", "grep -l '^template: 1' /etc/pve/lxc/*.conf 2>/dev/null | while read f; do v=$(basename $f .conf); h=$(grep '^hostname:' $f | cut -d' ' -f2); echo \"clone:$v  $h\"; done"],
                    30,
                ))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let mut msg = String::from("clonable golden templates (fast):\n");
            msg.push_str(if clones.trim().is_empty() {
                "  (none — run 'homelab template-build')\n"
            } else {
                &clones
            });
            msg.push_str("\nOS templates (full bootstrap):\n");
            for line in tarballs.lines().skip(1) {
                if let Some(name) = line.split_whitespace().next() {
                    msg.push_str(&format!("  {}\n", name));
                }
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: msg,
            }
        }
        Rpc::BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
        } => {
            run_mutating_op(state, &exec, req.id, "template-build", |ctx| {
                Box::pin(async move {
                    let cfg = homelab_core::ops::template::TemplateCfg {
                        temp_vmid,
                        version,
                        unprivileged,
                        ..Default::default()
                    };
                    homelab_core::ops::template::build_template(ctx, &cfg).await
                })
            })
            .await
        }
        Rpc::ExecIn { vmid, command } => {
            if let Err(e) = homelab_core::safety::exec_guard(
                state.config.exec_enabled,
                &SafetyConfig::default(),
                vmid,
            ) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                };
            }
            // A6: audit every invocation before running it.
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let audit = format!("{} exec vmid={} cmd={:?}\n", ts, vmid, command);
            let audit_path = format!("{}/audit.log", state.config.state_dir);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&audit_path)
                .and_then(|mut f| {
                    use std::io::Write as _;
                    f.write_all(audit.as_bytes())
                });
            info!("A6 exec vmid={} cmd={:?}", vmid, command);
            match homelab_core::executor::pct_sh(&exec, vmid, &command, 120).await {
                Ok(out) => RpcResponse {
                    id: req.id,
                    ok: out.success(),
                    message: format!(
                        "exit {}\n{}{}",
                        out.code,
                        out.stdout,
                        if out.stderr.is_empty() {
                            String::new()
                        } else {
                            format!("--- stderr ---\n{}", out.stderr)
                        }
                    ),
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                },
            }
        }
        Rpc::GetApplied { stack } => {
            // D6: the applied intent lives in the host repo; secrets never
            // do (A5), so this is safe to return.
            if stack.is_empty()
                || !stack
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "invalid stack name".into(),
                };
            }
            let dir = format!("{}/repo/stacks/{}", state.config.state_dir, stack);
            let mut files: Vec<homelab_proto::FileBlob> = Vec::new();
            fn walk(
                base: &std::path::Path,
                dir: &std::path::Path,
                out: &mut Vec<homelab_proto::FileBlob>,
            ) {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            walk(base, &p, out);
                        } else if let Ok(content) = std::fs::read_to_string(&p) {
                            out.push(homelab_proto::FileBlob {
                                path: p.strip_prefix(base).unwrap().to_string_lossy().into_owned(),
                                content,
                                mode: None,
                            });
                        }
                    }
                }
            }
            walk(
                std::path::Path::new(&dir),
                std::path::Path::new(&dir),
                &mut files,
            );
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&files).unwrap_or_else(|_| "[]".into()),
            }
        }
        Rpc::GetConfig => {
            let view = state.settings.read().unwrap().clone();
            let _ = state.log_tx.send(ServerMsg::Config(Box::new(view)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "config".into(),
            }
        }
        Rpc::SetConfig(view) => {
            // Validate before persisting.
            if let Some(h) = view.backup_hour {
                if h > 23 {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: "backup_hour must be 0-23".into(),
                    };
                }
            }
            if view.retention.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "retention needs at least one tier".into(),
                };
            }
            match persist_settings(&state.config, &view) {
                Ok(()) => {
                    *state.settings.write().unwrap() = *view;
                    info!("settings updated via G8");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: "settings saved and applied".into(),
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("persist settings: {}", e),
                },
            }
        }
        Rpc::SelfUpdateHost { binary_b64 } => {
            use base64::Engine as _;
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&binary_b64) {
                Ok(b) => b,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: format!("bad binary payload: {}", e),
                    }
                }
            };
            let cfg = homelab_core::ops::selfupdate::SelfUpdateCfg::default();
            // Stage outside the op so the (large) write is done before the
            // op-lock is taken. Raw bytes, not write_file (which is text).
            if let Err(e) = std::fs::write(&cfg.staged, &bytes).and_then(|()| {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cfg.staged, std::fs::Permissions::from_mode(0o755))
            }) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("stage binary: {}", e),
                };
            }
            info!("self-update requested: staged {} bytes", bytes.len());
            run_mutating_op(state, &exec, req.id, "self-update", |ctx| {
                Box::pin(async move { homelab_core::ops::selfupdate::self_update(ctx, &cfg).await })
            })
            .await
        }
        Rpc::Doctor => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let probes = gather_probes(
                &exec,
                &state.config.state_dir,
                state.config.mirror_remote.as_deref(),
                now,
            )
            .await;
            let checks = homelab_core::doctor::diagnose(&probes);
            let overall = homelab_core::doctor::overall(&checks);
            let mut msg = format!("doctor: {:?}\n", overall);
            for c in &checks {
                msg.push_str(&format!("  [{:?}] {} — {}\n", c.health, c.name, c.detail));
                if let Some(r) = &c.remedy {
                    msg.push_str(&format!("        ↳ {}\n", r));
                }
            }
            RpcResponse {
                id: req.id,
                ok: overall != homelab_core::doctor::Health::Fail,
                message: msg,
            }
        }
        Rpc::GetState => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let hs = store.load().await.unwrap_or_default();
            // H16: real capacity numbers (C6) — free -m, nproc, loadavg,
            // and committed RAM summed from the stored manifests.
            let free_out = exec
                .run(&Cmd::new("free", &["-m"], 15))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let nproc_out = exec
                .run(&Cmd::new("nproc", &[], 15))
                .await
                .map(|o| o.stdout)
                .unwrap_or_default();
            let loadavg = exec.read_file("/proc/loadavg").await.unwrap_or_default();
            let cap = capacity_numbers(&free_out, &nproc_out, &loadavg, &hs);
            let df = exec
                .run(&Cmd::new(
                    "df",
                    &["--output=pcent", &state.config.state_dir],
                    20,
                ))
                .await
                .ok()
                .and_then(|o| {
                    o.stdout
                        .lines()
                        .nth(1)
                        .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                })
                .unwrap_or(0);
            let (_, fingerprint) = tls::ensure_cert(&state.config.state_dir, "homelab-host")
                .unwrap_or((
                    tls::CertPaths {
                        cert_pem: String::new(),
                        key_pem: String::new(),
                    },
                    "unknown".into(),
                ));
            let stacks = hs
                .stacks
                .values()
                .map(|s| homelab_proto::StackView {
                    name: s
                        .hostname
                        .rsplit("-app-")
                        .next()
                        .unwrap_or(&s.hostname)
                        .to_string(),
                    vmid: s.vmid,
                    hostname: s.hostname.clone(),
                    apps: s
                        .apps
                        .iter()
                        .map(|a| homelab_proto::AppView {
                            name: a.clone(),
                            running: true,
                            restarts: 0,
                        })
                        .collect(),
                    drift: false, // computed client-side from applied_hash
                    applied_hash: s.applied_hash.clone(),
                    env_sealed: true,
                    online: true,
                    enabled: s.enabled,
                })
                .collect();
            let fleet = homelab_proto::FleetState {
                host: homelab_proto::HostView {
                    name: "pve-01".into(),
                    cpu_pct: 0,
                    ram_pct: 0,
                    disk_pct: df,
                    tls_fingerprint: fingerprint,
                    ram_total_mb: cap.0,
                    ram_used_mb: cap.1,
                    ram_committed_mb: cap.2,
                    cores_total: cap.3,
                    load1_x100: cap.4,
                },
                stacks,
            };
            let _ = state.log_tx.send(ServerMsg::State(Box::new(fleet)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "state".into(),
            }
        }
        Rpc::Incidents => {
            let dir = format!("{}/incidents", state.config.state_dir);
            let list = std::fs::read_dir(&dir)
                .map(|rd| {
                    let mut names: Vec<String> = rd
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    names
                })
                .unwrap_or_default();
            RpcResponse {
                id: req.id,
                ok: true,
                message: if list.is_empty() {
                    "no incidents recorded".into()
                } else {
                    format!("incidents:\n  {}", list.join("\n  "))
                },
            }
        }
    }
}

/// Gather doctor probes (F6). I/O stays here; the verdict logic is in core.
/// H8 hardening: the probe layer now feeds REAL data — backup freshness per
/// stack from state.json, offsite reachability via a quick rclone listing,
/// mirror lag via unpushed-commit count. Generic over the executor so the
/// healthy/broken matrix is testable with MockExecutor.
async fn gather_probes(
    exec: &dyn Executor,
    state_dir: &str,
    mirror_remote: Option<&str>,
    now_unix: u64,
) -> homelab_core::doctor::Probes {
    use homelab_core::doctor::{Probes, StackProbe};
    let state_raw = exec.read_file(&format!("{}/state.json", state_dir)).await;
    let state_parses = state_raw
        .as_ref()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
        .unwrap_or(true);

    // Per-stack backup freshness + container presence from state.json.
    let mut managed_stacks = Vec::new();
    if let Ok(raw) = state_raw.as_ref() {
        if let Ok(hs) = serde_json::from_str::<homelab_core::state::HostState>(raw) {
            for (name, st) in &hs.stacks {
                let present = exec
                    .run(&Cmd::new("pct", &["status", &st.vmid.to_string()], 20))
                    .await
                    .map(|o| o.success())
                    .unwrap_or(false);
                managed_stacks.push(StackProbe {
                    name: name.clone(),
                    backup_age_h: (st.last_backup > 0)
                        .then(|| now_unix.saturating_sub(st.last_backup) / 3600),
                    container_present: present,
                    env_sealed: true,
                });
            }
        }
    }

    // Offsite: is the gdrive remote configured, and does a cheap listing work?
    let remotes = exec
        .run(&Cmd::new("rclone", &["listremotes"], 20))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let offsite_configured = remotes.lines().any(|l| l.trim() == "gdrive:");
    let offsite_token_valid = offsite_configured
        && exec
            .run(&Cmd::new(
                "rclone",
                &[
                    "lsd",
                    "gdrive:homelab-backups",
                    "--max-depth",
                    "1",
                    "--contimeout",
                    "10s",
                ],
                30,
            ))
            .await
            .map(|o| o.success())
            .unwrap_or(false);

    // Mirror lag: commits not yet on the mirror remote.
    let repo = format!("{}/repo", state_dir);
    let mirror_behind = match mirror_remote {
        None => None,
        Some(_) => exec
            .run(&Cmd::new(
                "git",
                &[
                    "-C",
                    &repo,
                    "rev-list",
                    "--count",
                    "--branches",
                    "--not",
                    "--remotes=mirror",
                ],
                30,
            ))
            .await
            .ok()
            .and_then(|o| o.stdout.trim().parse::<u32>().ok()),
    };
    let interrupted = std::fs::read_to_string(format!("{}/journal.jsonl", state_dir))
        .map(|j| {
            homelab_core::incidents::interrupted_ops(&j)
                .into_iter()
                .map(|(op, _)| op)
                .collect()
        })
        .unwrap_or_default();
    // Host disk free % via df on the state dir.
    let disk = exec
        .run(&Cmd::new("df", &["--output=pcent", state_dir], 20))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                .map(|used| 100u64.saturating_sub(used))
        });
    Probes {
        host_disk_free_pct: disk,
        state_parses,
        managed_stacks,
        offsite_configured,
        offsite_token_valid,
        mirror_behind,
        interrupted_ops: interrupted,
    }
}
