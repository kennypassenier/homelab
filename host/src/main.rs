//! Homelab HOST daemon — thin shell around homelab-core (AR1).
//!
//! Provides: the real Executor (processes + files), config (TOML + env,
//! AR11), tracing (AR15), the journal file (B5), the WS server with required
//! bearer token, and the broadcast sink feeding connected clients (F2).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
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

/// One externally-made backup to keep an eye on.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct WatchedBackup {
    /// What the nightly report calls it.
    name: String,
    /// An rclone path, e.g. `gdrive:homelab-backups/OPNSense-backups`.
    rclone_path: String,
    /// Older than this and it becomes a finding. OPNsense runs its backup
    /// cron at 01:00 with up to an hour of jitter, so 26 leaves room for a
    /// late night without crying wolf.
    #[serde(default = "default_watched_max_age_hours")]
    max_age_hours: u64,
}

fn default_watched_max_age_hours() -> u64 {
    26
}

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
    /// G14: how long a passed restore drill counts for. B3 says quarterly;
    /// the default is 90 days. Kenny's, not the author's — a house that
    /// changes little wants it longer, one being rebuilt wants it shorter.
    restore_drill_interval_s: Option<u64>,
    /// G16: the second route, tried when the first one does not answer 2xx.
    ///
    /// Y2 sends notifications through kyu so an HA outage cannot lose them.
    /// Its one carved-out exception was "kyu itself is down" — which is
    /// exactly what happens while the orchestrator is updating kyu, and the
    /// message lost in that window is the one saying the update failed. Point
    /// this straight at Home Assistant.
    notify_fallback_webhook: Option<String>,
    /// Credential for that second route, when it needs one. The HA webhook
    /// does not; something else might.
    notify_fallback_auth_bearer: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    /// How far back the log-coverage question looks. A quiet service is not
    /// a broken one: `homepage` and `kp-soft` ship a few hundred lines a day
    /// and none at all in a given hour, and the first version of this check
    /// reported both as "logs are going nowhere". A check that alarms on
    /// healthy silence is a check that gets ignored, and then the real
    /// silence goes unnoticed too.
    logs_window: Option<String>,
    retention: Option<Vec<homelab_proto::RetentionTier>>,
    exec_enabled: Option<bool>,
    mirror_remote: Option<String>,
    no_touch: Option<Vec<u16>>,
    /// fix-120 (api-token-is-root, 2026-09-27): vmids a deploy may make a
    /// privileged container. Absent = the ones the fleet runs today
    /// (`homelab_core::safety::FLEET_PRIVILEGED_VMIDS`). ssh-edited only,
    /// like exec_enabled: a policy the token could change is no policy.
    privileged_vmids: Option<Vec<u16>>,
    /// fix-120: host directories `data_mounts:` may borrow. Absent = the ones
    /// the fleet uses today (`FLEET_DATA_MOUNT_ROOTS`).
    data_mount_roots: Option<Vec<String>>,
    gateway_vmid: Option<u16>,
    gateway_routes_dir: Option<String>,
    /// Route A (Kenny, form J1, 2026-09-02): devices this suite may not
    /// touch, that can nevertheless hand over their own configuration. One
    /// GET per night, straight into restic. Absent = nothing is fetched.
    device_backups: Option<Vec<homelab_core::ops::devicebackup::DeviceBackup>>,
    /// O1: backups this orchestrator does not make but does watch. Each is
    /// a folder on an rclone remote that some device outside the suite
    /// writes to — today the OPNsense plugin that uploads the router's
    /// configuration every night. Absent = nothing is watched, which is what
    /// every fleet had before this existed.
    watched_backups: Option<Vec<WatchedBackup>>,
    /// E8: ZFS snapshot+replication jobs (replaces the old cron script).
    zfs_jobs: Option<Vec<homelab_core::ops::zfs::ZfsJob>>,
    /// D60: `[registry_cache] host = "10.10.10.17"` plus one `[[registry_cache.upstreams]]`
    /// per mirrored registry. Absent = no cache.
    registry_cache: Option<homelab_core::ops::registry_cache::CacheCfg>,
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
    /// T51: Homepage's `services.yaml` on the host, rendered from the
    /// gateway's route fragments. Absent = the front page stays hand-made,
    /// which is how it came to be zero bytes.
    homepage_services_file: Option<String>,
    /// T49: the file the Uptime Kuma seeder reads its generated half from.
    /// Absent = the watch list stays whatever a hand-run script last made,
    /// which is how a monitor came to report Uptime Kuma itself as down from
    /// an address it had left that morning (F157).
    kuma_monitors_file: Option<String>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    #[serde(default = "default_ask_timeout_s")]
    ask_timeout_s: u64,
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
    #[serde(default = "default_backup_concurrency")]
    backup_concurrency: usize,
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
    /// G14: how long a passed restore drill counts for. B3 says quarterly;
    /// the default is 90 days. Kenny's, not the author's — a house that
    /// changes little wants it longer, one being rebuilt wants it shorter.
    restore_drill_interval_s: u64,
    /// G16: the second route, tried when the first one does not answer 2xx.
    ///
    /// Y2 sends notifications through kyu so an HA outage cannot lose them.
    /// Its one carved-out exception was "kyu itself is down" — which is
    /// exactly what happens while the orchestrator is updating kyu, and the
    /// message lost in that window is the one saying the update failed. Point
    /// this straight at Home Assistant.
    notify_fallback_webhook: Option<String>,
    /// Credential for that second route, when it needs one. The HA webhook
    /// does not; something else might.
    notify_fallback_auth_bearer: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    /// Window for the log-coverage question; see the file field.
    logs_window: String,
    /// D5: git remote URL for the offsite intent mirror; None = off.
    mirror_remote: Option<String>,
    /// H1 (hardening): safety values configurable via host.toml so M5 can
    /// migrate the gateway / adjust the no-touch list without a release.
    /// Hardcoded DEFAULT_NO_TOUCH remains the default.
    safety: SafetyConfig,
    /// E8: declared ZFS replication jobs; empty = feature off.
    zfs_jobs: Vec<homelab_core::ops::zfs::ZfsJob>,
    watched_backups: Vec<WatchedBackup>,
    device_backups: Vec<homelab_core::ops::devicebackup::DeviceBackup>,
    /// Backup target and timeouts, resolved once from host.toml. Callers
    /// clone this and override only `tiers`.
    backup: homelab_core::ops::backup::BackupCfg,
    /// D60: the pull-through cache in the house. Absent = images keep naming
    /// their own origin, which is also what happens when it does not answer.
    registry_cache: Option<homelab_core::ops::registry_cache::CacheCfg>,
    /// T1: where per-stack Prometheus discovery files are written.
    metrics_targets_dir: Option<String>,
    /// T2: Grafana's provisioning directory inside the gateway container.
    grafana_dashboards_dir: Option<String>,
    /// T51: Homepage's `services.yaml` on the host, rendered from the
    /// gateway's route fragments. Absent = the front page stays hand-made,
    /// which is how it came to be zero bytes.
    homepage_services_file: Option<String>,
    /// T49: the file the Uptime Kuma seeder reads its generated half from.
    /// Absent = the watch list stays whatever a hand-run script last made,
    /// which is how a monitor came to report Uptime Kuma itself as down from
    /// an address it had left that morning (F157).
    kuma_monitors_file: Option<String>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    ask_timeout_s: u64,
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
    backup_concurrency: usize,
    /// Initial mutable settings (live copy lives in AppState.settings).
    initial_settings: homelab_proto::HostConfigView,
}

/// ── F186: keys the daemon would otherwise ignore in silence. ──────────
///
/// `host.toml` is hand-edited, and TOML's rule that bare keys belong to the
/// table header above them makes one specific mistake invisible: append a
/// setting at the end of the file and it becomes a field of whatever table
/// happened to be last. That is not hypothetical. On 2026-09-02 the two
/// OPNsense keys sat under the final `[[registry_cache.upstreams]]` entry,
/// so `kea` was None and the whole static-address feature was off, on a host
/// whose operator had every reason to believe it was on. The file's own
/// comment warns about the trap; the warning was written after the first
/// time and did not prevent the second.
///
/// Serde ignores unknown fields by default, which is what makes it silent.
/// These lists are hand-maintained because Rust has no reflection, and the
/// failure direction is deliberately the safe one: a field added to
/// `FileConfig` but forgotten here produces a loud false "unknown key",
/// never a silently swallowed real one.
const KNOWN_TOP: &[&str] = &[
    "token",
    "listen",
    "state_dir",
    "backup_hour",
    "notify_webhook",
    "notify_auth_bearer",
    "restore_drill_interval_s",
    "notify_fallback_webhook",
    "notify_fallback_auth_bearer",
    "prometheus_url",
    "loki_url",
    "logs_window",
    "retention",
    "exec_enabled",
    "mirror_remote",
    "no_touch",
    "privileged_vmids",
    "data_mount_roots",
    "gateway_vmid",
    "gateway_routes_dir",
    "zfs_jobs",
    "registry_cache",
    "restic_base",
    "restic_password_file",
    "restic_snapshot_timeout_s",
    "restic_restore_timeout_s",
    "metrics_targets_dir",
    "grafana_dashboards_dir",
    "homepage_services_file",
    "kuma_monitors_file",
    "ask_timeout_s",
    "backup_concurrency",
    "watched_backups",
    "device_backups",
];
const KNOWN_REGISTRY_CACHE: &[&str] = &["host", "upstreams", "pull_timeout_secs"];
const KNOWN_UPSTREAM: &[&str] = &["registry", "port"];
const KNOWN_ZFS_JOB: &[&str] = &["source", "target"];
const KNOWN_WATCHED_BACKUP: &[&str] = &["name", "rclone_path", "max_age_hours"];
const KNOWN_DEVICE_BACKUP: &[&str] = &["name", "url", "cred_file", "filename", "pin", "ca_file"];
const KNOWN_RETENTION: &[&str] = &["every_days", "keep", "span_days"];

/// Every key in `raw` that no field of `FileConfig` will ever read, as
/// dotted paths. An empty result means the file says exactly what it looks
/// like it says.
fn unknown_keys(raw: &toml::Table) -> Vec<String> {
    fn table(out: &mut Vec<String>, t: &toml::Table, known: &[&str], path: &str) {
        for k in t.keys() {
            if !known.contains(&k.as_str()) {
                out.push(if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                });
            }
        }
    }
    fn array_of(out: &mut Vec<String>, v: Option<&toml::Value>, known: &[&str], path: &str) {
        let Some(arr) = v.and_then(|v| v.as_array()) else {
            return;
        };
        for (i, item) in arr.iter().enumerate() {
            if let Some(t) = item.as_table() {
                table(out, t, known, &format!("{}[{}]", path, i));
            }
        }
    }

    let mut out = Vec::new();
    table(&mut out, raw, KNOWN_TOP, "");
    array_of(&mut out, raw.get("zfs_jobs"), KNOWN_ZFS_JOB, "zfs_jobs");
    array_of(&mut out, raw.get("retention"), KNOWN_RETENTION, "retention");
    array_of(
        &mut out,
        raw.get("watched_backups"),
        KNOWN_WATCHED_BACKUP,
        "watched_backups",
    );
    array_of(
        &mut out,
        raw.get("device_backups"),
        KNOWN_DEVICE_BACKUP,
        "device_backups",
    );
    if let Some(rc) = raw.get("registry_cache").and_then(|v| v.as_table()) {
        table(&mut out, rc, KNOWN_REGISTRY_CACHE, "registry_cache");
        array_of(
            &mut out,
            rc.get("upstreams"),
            KNOWN_UPSTREAM,
            "registry_cache.upstreams",
        );
    }
    out.sort();
    out
}

fn load_config() -> Config {
    load_config_from(
        std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "/etc/homelab/host.toml".into()),
    )
}

fn load_config_from(path: String) -> Config {
    // A missing file is legal — every field has a default. A file that
    // exists but does not parse is not: the old code answered that with
    // `FileConfig::default()`, so a single typo turned every configured
    // feature off at once and said nothing (F186).
    let file: FileConfig = match std::fs::read_to_string(&path) {
        Err(_) => FileConfig::default(),
        Ok(raw) => {
            let parsed: toml::Table = match toml::from_str(&raw) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("FATAL: {} does not parse as TOML: {}", path, e);
                    std::process::exit(1);
                }
            };
            for key in unknown_keys(&parsed) {
                eprintln!(
                    "WARNING: {}: '{}' is not a setting this daemon reads and is being \
                     ignored. A key written after a [table] header belongs to that table \
                     - move it above the first one.",
                    path, key
                );
            }
            match toml::Table::try_into::<FileConfig>(parsed) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("FATAL: {} is not a valid host config: {}", path, e);
                    std::process::exit(1);
                }
            }
        }
    };

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
    let cfg = Config {
        token,
        listen,
        state_dir,
        config_path: path,
        exec_enabled: file.exec_enabled.unwrap_or(false),
        notify_auth_bearer: file.notify_auth_bearer.clone(),
        restore_drill_interval_s: file
            .restore_drill_interval_s
            .unwrap_or(homelab_core::ops::restoredrill::DEFAULT_DRILL_INTERVAL_S),
        notify_fallback_webhook: file.notify_fallback_webhook.clone(),
        notify_fallback_auth_bearer: file.notify_fallback_auth_bearer.clone(),
        prometheus_url: file.prometheus_url.clone(),
        loki_url: file.loki_url.clone(),
        logs_window: file.logs_window.clone().unwrap_or_else(default_logs_window),
        mirror_remote: file.mirror_remote,
        safety: {
            let mut sc = SafetyConfig::default();
            // F8: the file ADDS to the compiled list, it does not replace it.
            // It used to assign, so `no_touch = [200]` in host.toml would
            // have quietly dropped Home Assistant and the router out of
            // protection — a typo away from making the two machines this
            // project may never touch touchable. The list is law and its home
            // is `core/src/safety.rs`; config can only widen it.
            if let Some(list) = file.no_touch {
                for vmid in list {
                    if !sc.no_touch.contains(&vmid) {
                        sc.no_touch.push(vmid);
                    }
                }
            }
            if let Some(gw) = file.gateway_vmid {
                sc.gateway_vmid = gw;
            }
            if let Some(dir) = file.gateway_routes_dir {
                sc.gateway_routes_dir = dir;
            }
            // fix-120: the daemon always runs with a policy; without the
            // keys it is what the fleet uses today, so nothing is refused
            // that deployed yesterday.
            sc.privileged_vmids = Some(
                file.privileged_vmids
                    .unwrap_or_else(|| homelab_core::safety::FLEET_PRIVILEGED_VMIDS.to_vec()),
            );
            sc.data_mount_roots = Some(file.data_mount_roots.unwrap_or_else(|| {
                homelab_core::safety::FLEET_DATA_MOUNT_ROOTS
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            }));
            sc
        },
        zfs_jobs: file.zfs_jobs.unwrap_or_default(),
        watched_backups: file.watched_backups.unwrap_or_default(),
        device_backups: file.device_backups.unwrap_or_default(),
        // D60: absent from host.toml = no cache, which is the same behaviour
        // the fleet had before there was one.
        registry_cache: file.registry_cache,
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
        homepage_services_file: file.homepage_services_file,
        kuma_monitors_file: file.kuma_monitors_file,
        backup_concurrency: file.backup_concurrency,
        ask_timeout_s: file.ask_timeout_s,
        initial_settings: homelab_proto::HostConfigView {
            backup_hour: file.backup_hour,
            notify_webhook: file.notify_webhook,
            retention: file
                .retention
                .unwrap_or_else(homelab_core::retention::default_tiers),
        },
    };
    // F259: a watcher whose path no writer targets watches nothing — it
    // reports "holds no files at all" forever while looking like a working
    // check. Said out loud at load, where every other config fault is said.
    for w in orphan_watchers(
        &cfg.watched_backups,
        &cfg.device_backups,
        &cfg.backup.restic_base,
    ) {
        tracing::warn!(
            "watched_backups entry {} matches no device_backups writer — it will report \
             'holds no files at all' whatever happens",
            w
        );
    }
    // fix-126: said at every start until the route moves to TLS.
    for route in plaintext_bearer_routes(
        cfg.initial_settings.notify_webhook.as_deref(),
        cfg.notify_auth_bearer.as_deref(),
        cfg.notify_fallback_webhook.as_deref(),
        cfg.notify_fallback_auth_bearer.as_deref(),
    ) {
        tracing::warn!(
            "notification route {} sends its bearer token over plain HTTP — anything on that \
             network segment can read it; an https:// route (kyu behind Traefik) closes this",
            route
        );
    }
    cfg
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
        // G16: the second notification route and its credential. Same
        // reasoning as the line above, one step further: losing the fallback
        // silently would leave exactly the window Y2 carved out — kyu being
        // restarted by the very operation whose failure you need to hear.
        // G14: Kenny's interval for the restore drill. A save that dropped
        // it would quietly reset the rehearsal to the default — not
        // dangerous, and exactly the kind of silent revert F208 was about.
        #[serde(skip_serializing_if = "Option::is_none")]
        restore_drill_interval_s: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notify_fallback_webhook: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notify_fallback_auth_bearer: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prometheus_url: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loki_url: Option<&'a String>,
        logs_window: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mirror_remote: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        no_touch: Option<&'a Vec<u16>>,
        // fix-120: a settings save that dropped these would quietly put the
        // fleet defaults back in place of what Kenny wrote.
        #[serde(skip_serializing_if = "Option::is_none")]
        privileged_vmids: Option<&'a Vec<u16>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data_mount_roots: Option<&'a Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway_vmid: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway_routes_dir: Option<&'a String>,
        #[serde(skip_serializing_if = "<[_]>::is_empty")]
        zfs_jobs: &'a [homelab_core::ops::zfs::ZfsJob],
        // F208: these three were absent, so every settings save silently
        // wiped them — the pull-through cache the media deploy leans on, the
        // router-backup watch, and the device backups. A struct that renders
        // the whole file must know the whole file.
        #[serde(skip_serializing_if = "Option::is_none")]
        registry_cache: Option<&'a homelab_core::ops::registry_cache::CacheCfg>,
        #[serde(skip_serializing_if = "<[_]>::is_empty")]
        watched_backups: &'a [WatchedBackup],
        #[serde(skip_serializing_if = "<[_]>::is_empty")]
        device_backups: &'a [homelab_core::ops::devicebackup::DeviceBackup],
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
        homepage_services_file: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kuma_monitors_file: Option<&'a String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        backup_concurrency: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ask_timeout_s: Option<u64>,
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
        restore_drill_interval_s: Some(config.restore_drill_interval_s),
        notify_fallback_webhook: config.notify_fallback_webhook.as_ref(),
        notify_fallback_auth_bearer: config.notify_fallback_auth_bearer.as_ref(),
        prometheus_url: config.prometheus_url.as_ref(),
        loki_url: config.loki_url.as_ref(),
        logs_window: (config.logs_window != default_logs_window()).then_some(&config.logs_window),
        mirror_remote: config.mirror_remote.as_ref(),
        no_touch: (config.safety.no_touch != SafetyConfig::default().no_touch)
            .then_some(&config.safety.no_touch),
        privileged_vmids: config
            .safety
            .privileged_vmids
            .as_ref()
            .filter(|v| v.as_slice() != homelab_core::safety::FLEET_PRIVILEGED_VMIDS),
        data_mount_roots: config.safety.data_mount_roots.as_ref().filter(|v| {
            !v.iter()
                .map(String::as_str)
                .eq(homelab_core::safety::FLEET_DATA_MOUNT_ROOTS.iter().copied())
        }),
        gateway_vmid: (config.safety.gateway_vmid != SafetyConfig::default().gateway_vmid)
            .then_some(config.safety.gateway_vmid),
        gateway_routes_dir: (config.safety.gateway_routes_dir
            != SafetyConfig::default().gateway_routes_dir)
            .then_some(&config.safety.gateway_routes_dir),
        zfs_jobs: &config.zfs_jobs,
        registry_cache: config.registry_cache.as_ref(),
        watched_backups: &config.watched_backups,
        device_backups: &config.device_backups,
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
        homepage_services_file: config.homepage_services_file.as_ref(),
        kuma_monitors_file: config.kuma_monitors_file.as_ref(),
        backup_concurrency: (config.backup_concurrency != default_backup_concurrency())
            .then_some(config.backup_concurrency),
        ask_timeout_s: (config.ask_timeout_s != default_ask_timeout_s())
            .then_some(config.ask_timeout_s),
    };
    toml::to_string_pretty(&out).map_err(|e| e.to_string())
}

/// Y1: how many backups actually run at once, given what the config says.
///
/// Zero is the case worth guarding. `backup_concurrency = 0` in host.toml
/// reads like "no limit" to the person typing it and means "run none" to the
/// stream that consumes it — so a typo meant to go faster would silently
/// produce a night with no backups at all, and the only sign would be
/// `last_backup` never moving. Clamped to at least one: slow is recoverable,
/// silent is not.
fn effective_concurrency(configured: usize) -> usize {
    configured.max(1)
}

/// Two minutes: long enough to read a question and decide, short enough
/// that a window left open does not hold the global op lock all night.
fn default_ask_timeout_s() -> u64 {
    120
}

/// Kenny's choice (form Y4, 2026-09-02): three at a time. About a third of
/// the wait, and never more than three services briefly quiet at 04:00.
fn default_backup_concurrency() -> usize {
    3
}

/// A day. The coverage question is asked nightly and on demand, so a stack
/// that has shipped nothing in twenty-four hours has genuinely stopped —
/// while an hour of quiet is normal for a service nobody browsed.
fn default_logs_window() -> String {
    "24h".into()
}

/// Only `<digits><unit>` reaches the query string. The window is pasted into
/// a hand-built URL, and a value out of a config file is not a value to trust
/// with that: anything else falls back to the default rather than producing a
/// query that silently means something other than it says.
fn sane_window(w: &str) -> String {
    let ok = w.len() >= 2
        && w.chars().next().is_some_and(|c| c.is_ascii_digit())
        && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())
        && matches!(w.chars().last(), Some('s' | 'm' | 'h' | 'd' | 'w'));
    if ok {
        w.to_string()
    } else {
        default_logs_window()
    }
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
    /// fix-51 (expert panel, state-writes-race, 2026-09-27): two writers of
    /// one path shared the temp name `<path>.tmp`. One removed the other's
    /// half-written temp file, or `create_new` failed with EEXIST, and the
    /// error was thrown away by `let _ = store.save(..)`. Concurrent writes of
    /// one path must all succeed and leave one whole version behind.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fix_51_concurrent_writes_of_one_path_all_succeed_and_stay_whole() {
        use homelab_core::executor::Executor;
        let dir = std::env::temp_dir().join(format!("homelab-fix51-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json").to_string_lossy().into_owned();
        for round in 0..20 {
            let writes = (0..8).map(|i| {
                let path = path.clone();
                tokio::spawn(async move {
                    let body = format!("{}-{};", round, i).repeat(20_000);
                    super::RealExecutor.write_file(&path, &body, 0o644).await
                })
            });
            for w in writes.collect::<Vec<_>>() {
                w.await
                    .unwrap()
                    .expect("every concurrent write of one path succeeds");
            }
            let got = std::fs::read_to_string(&path).unwrap();
            let piece = &got[..=got.find(';').expect("content present")];
            assert_eq!(got, piece.repeat(20_000), "one writer's content, whole");
        }
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "state.json")
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(leftovers.is_empty(), "no temp files left: {:?}", leftovers);
    }

    /// fix-52 (expert panel, background-tasks-unsupervised, 2026-09-27): the
    /// scheduler was spawned and its handle dropped, so one panic in it ended
    /// the nightly backups for good while the daemon kept serving and the
    /// watchdog kept being fed. A dead scheduler must end the process with a
    /// failure, so systemd restarts it with a live one.
    #[tokio::test]
    async fn fix_52_a_dead_scheduler_ends_the_daemon_with_a_failure() {
        let scheduler = tokio::spawn(async { panic!("scheduler bug") });
        let code = tokio::time::timeout(
            Duration::from_secs(5),
            super::supervise(
                std::future::pending::<std::io::Result<()>>(),
                scheduler,
                std::future::pending::<()>(),
                Arc::new(Mutex::new(())),
                Duration::from_secs(1),
            ),
        )
        .await
        .expect("supervision ends when the scheduler dies");
        assert_ne!(code, 0, "and it ends as a failure");
    }

    /// fix-52 and adoption norm N1: SIGTERM lets the running operation finish
    /// (up to a bound) and then exits 0, instead of killing a step mid-way.
    #[tokio::test]
    async fn fix_52_sigterm_waits_for_the_running_operation_then_exits_zero() {
        let op_lock = Arc::new(Mutex::new(()));
        let held = op_lock.clone().lock_owned().await;
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f2 = finished.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            f2.store(true, std::sync::atomic::Ordering::SeqCst);
            drop(held);
        });
        let scheduler = tokio::spawn(std::future::pending::<()>());
        let code = tokio::time::timeout(
            Duration::from_secs(5),
            super::supervise(
                std::future::pending::<std::io::Result<()>>(),
                scheduler,
                std::future::ready(()),
                op_lock.clone(),
                Duration::from_secs(3),
            ),
        )
        .await
        .expect("supervision ends on SIGTERM");
        assert_eq!(code, 0);
        assert!(
            finished.load(std::sync::atomic::Ordering::SeqCst),
            "it waited for the operation holding the lock"
        );
        // Having waited, it keeps the lock: nothing queued after the signal
        // starts before the process exits.
        assert!(op_lock.try_lock().is_err(), "the lock stays held");
        // A hung operation does not hold the exit forever.
        let op_lock = Arc::new(Mutex::new(()));
        let _stuck = op_lock.clone().lock_owned().await;
        let scheduler = tokio::spawn(std::future::pending::<()>());
        let code = tokio::time::timeout(
            Duration::from_secs(5),
            super::supervise(
                std::future::pending::<std::io::Result<()>>(),
                scheduler,
                std::future::ready(()),
                op_lock,
                Duration::from_millis(100),
            ),
        )
        .await
        .expect("the drain wait is bounded");
        assert_eq!(code, 0);
    }

    /// fix-53 (expert panel, timeout-leaves-container-work-running,
    /// 2026-09-27): a timeout killed only the direct child (`pct`), not what
    /// it had started (`lxc-attach` and the script), so a "timed out" step
    /// went on changing the container. Everything the command started must
    /// be gone once the timeout is reported.
    #[tokio::test]
    async fn fix_53_a_timeout_kills_everything_the_command_started() {
        use homelab_core::executor::Executor;
        let pidfile =
            std::env::temp_dir().join(format!("homelab-fix53-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);
        let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
        let got = super::RealExecutor
            .run(&Cmd::new("sh", &["-c", &script], 1))
            .await;
        assert!(
            matches!(got, Err(CoreError::Timeout { .. })),
            "the wait ends as a timeout: {:?}",
            got
        );
        let pid = std::fs::read_to_string(&pidfile)
            .expect("the script recorded its child")
            .trim()
            .to_string();
        let _ = std::fs::remove_file(&pidfile);
        // An exit is not instant; a zombie awaiting its reaper counts as gone.
        let mut alive = true;
        for _ in 0..20 {
            let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).unwrap_or_default();
            alive = !stat.is_empty() && !stat.contains(") Z ");
            if !alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if alive {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid])
                .status();
        }
        assert!(
            !alive,
            "the command's own child {} outlived the timeout",
            pid
        );
    }

    /// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): a
    /// frame the host could not parse was logged whole. A deploy frame carries
    /// every `.env` value of its stack (`DeploySpec.env` is the secrets
    /// channel) and a staging frame tens of MB of base64, so one version skew
    /// put a stack's secrets, or a whole binary, into one pve journal line.
    #[test]
    fn fix_55_an_unparseable_frame_is_logged_without_its_body() {
        let secret = "hunter2-db-password";
        let frame = format!(
            r#"{{"id":7,"cmd":"deploy_stack","manifest":{{"vmid":"not-a-number"}},"env":{{"app":{{"DB_PASSWORD":"{}"}}}},"blob":"{}"}}"#,
            secret,
            "A".repeat(100_000)
        );
        let e = serde_json::from_str::<RpcRequest>(&frame).unwrap_err();
        let line = super::unparseable_frame_line(&e, &frame);
        assert!(!line.contains(secret), "{}", line);
        assert!(line.len() < 500, "{} bytes", line.len());
        assert!(line.contains("deploy_stack"), "names the method: {}", line);
        assert!(
            line.contains(&frame.len().to_string()),
            "and the size: {}",
            line
        );
        // serde's own message quotes the offending value; that value may be a
        // secret too, so only its position is kept.
        let e = serde_json::from_str::<RpcRequest>(r#"{"id":"s3cr3t-token","cmd":"ping"}"#)
            .unwrap_err();
        let line = super::unparseable_frame_line(&e, r#"{"id":"s3cr3t-token","cmd":"ping"}"#);
        assert!(!line.contains("s3cr3t-token"), "{}", line);
    }

    /// covers: F208
    ///
    /// G1 of the Phase-7 gate. Saving a setting from the TUI rewrites the
    /// whole of host.toml from the `Out` struct, so a field `Out` does not
    /// know is a field that DISAPPEARS on save. Three were missing when this
    /// was measured: the pull-through cache the media deploy leans on, the
    /// router-backup watch, and the device backups.
    ///
    /// The old guard test could not catch it — it set those fields to empty
    /// in its own fixture and never asserted them afterwards, so it passed
    /// on exactly this bug. This one reads both structs out of the source,
    /// which is the same trick `known_top_lists_every_field_of_file_config`
    /// uses, and cannot drift.
    #[test]
    fn the_settings_writer_knows_every_field_the_config_has() {
        let src = include_str!("main.rs");
        fn fields(src: &str, marker: &str, end: &str) -> Vec<String> {
            let start = src
                .find(marker)
                .unwrap_or_else(|| panic!("{} not found", marker));
            let body = &src[start..];
            let body = &body[..body.find(end).expect("unterminated struct")];
            let mut out = Vec::new();
            for line in body.lines().skip(1) {
                let t = line.trim();
                if t.starts_with("//") || t.starts_with("#[") || t.is_empty() {
                    continue;
                }
                if let Some(name) = t.split(':').next() {
                    let name = name.trim();
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    {
                        out.push(name.to_string());
                    }
                }
            }
            out
        }

        let file_cfg = fields(src, "struct FileConfig {", "\n}");
        let written = fields(src, "    struct Out<'a> {", "\n    }");
        assert!(
            file_cfg.len() > 20 && written.len() > 20,
            "the parser broke, not the code: {} vs {}",
            file_cfg.len(),
            written.len()
        );

        let dropped: Vec<&String> = file_cfg.iter().filter(|f| !written.contains(f)).collect();
        assert!(
            dropped.is_empty(),
            "these settings would be WIPED from host.toml the next time Kenny \
             saves anything from the TUI: {:?}",
            dropped
        );
    }

    /// V5 (Kenny, 2026-09-02): the two key lists were hand-maintained
    /// because Rust cannot enumerate its own struct fields, and he asked for
    /// that cost to go away rather than be accepted. It can: the source is
    /// available at compile time, so the list can be checked against the
    /// struct itself.
    ///
    /// A field added to `FileConfig` and forgotten in `KNOWN_TOP` used to
    /// produce a loud false "unknown key" at startup — safe, but only
    /// noticed by whoever read the log. Now it is a failing test, at build
    /// time, naming the field.
    #[test]
    fn known_top_lists_every_field_of_file_config() {
        let src = include_str!("main.rs");
        let start = src
            .find("struct FileConfig {")
            .expect("FileConfig struct not found — this test parses it");
        let body = &src[start..];
        let end = body.find("\n}").expect("unterminated FileConfig struct");
        let body = &body[..end];

        let mut fields: Vec<&str> = Vec::new();
        for line in body.lines().skip(1) {
            let t = line.trim();
            if t.starts_with("//") || t.starts_with("#[") || t.is_empty() {
                continue;
            }
            if let Some(name) = t.split(':').next() {
                let name = name.trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    fields.push(name);
                }
            }
        }
        assert!(
            fields.len() > 20,
            "parsed only {} fields — the parser broke, not the list",
            fields.len()
        );

        let missing: Vec<&&str> = fields.iter().filter(|f| !KNOWN_TOP.contains(f)).collect();
        assert!(
            missing.is_empty(),
            "FileConfig has field(s) absent from KNOWN_TOP, so host.toml would \
             report them as unknown settings: {:?}",
            missing
        );

        let stale: Vec<&&str> = KNOWN_TOP.iter().filter(|k| !fields.contains(k)).collect();
        assert!(
            stale.is_empty(),
            "KNOWN_TOP names key(s) FileConfig no longer has: {:?}",
            stale
        );
    }

    /// F8: host.toml may only ADD to the no-touch list. Assigning used to be
    /// possible, which meant one line in a hand-edited file could drop VM 100
    /// and VM 101 out of protection without a word.
    #[test]
    fn the_config_can_widen_the_no_touch_list_but_never_shrink_it() {
        let raw = "token = \"0123456789abcdef0123\"\nno_touch = [200, 201]\n";
        std::fs::write("/tmp/homelab-f8-test.toml", raw).unwrap();
        std::env::set_var("HOMELAB_CONFIG", "/tmp/homelab-f8-test.toml");
        let cfg = load_config();
        std::env::remove_var("HOMELAB_CONFIG");

        for compiled in homelab_core::safety::DEFAULT_NO_TOUCH {
            assert!(
                cfg.safety.no_touch.contains(compiled),
                "config dropped {} out of the no-touch list",
                compiled
            );
        }
        assert!(cfg.safety.no_touch.contains(&200));
        assert!(cfg.safety.no_touch.contains(&201));
    }

    /// fix-36: a vmid host.toml adds to the no-touch list is refused by
    /// remote exec too, not only by the operations.
    #[test]
    fn fix_36_remote_exec_refuses_a_vmid_the_config_added_to_the_no_touch_list() {
        let raw = "token = \"0123456789abcdef0123\"\nexec_enabled = true\nno_touch = [200]\n";
        // Loaded by path, not through HOMELAB_CONFIG: tests run in parallel
        // and another test sets that variable.
        std::fs::write("/tmp/homelab-fix36-test.toml", raw).unwrap();
        let cfg = load_config_from("/tmp/homelab-fix36-test.toml".into());
        assert!(
            exec_allowed(&cfg, 200).is_err(),
            "vmid 200 is on the configured no-touch list; exec must refuse it"
        );
        assert!(exec_allowed(&cfg, 101).is_err());
        assert!(exec_allowed(&cfg, 150).is_ok());
    }

    /// fix-120 (expert panel, api-token-is-root, 2026-09-27): without the two
    /// keys the host runs with what the fleet uses today, so the policy lands
    /// without refusing a single existing deploy; with them it runs with
    /// exactly what the file says.
    #[test]
    fn fix_120_host_policy_defaults_to_the_fleet_and_follows_host_toml() {
        let dir = std::env::temp_dir().join(format!("homelab-fix120-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bare = dir.join("bare.toml");
        std::fs::write(&bare, "token = \"0123456789abcdef0123\"\n").unwrap();
        let cfg = load_config_from(bare.to_string_lossy().into_owned());
        assert_eq!(
            cfg.safety.privileged_vmids.as_deref(),
            Some(homelab_core::safety::FLEET_PRIVILEGED_VMIDS)
        );
        assert_eq!(
            cfg.safety.data_mount_roots,
            Some(
                homelab_core::safety::FLEET_DATA_MOUNT_ROOTS
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            )
        );
        let set = dir.join("set.toml");
        std::fs::write(
            &set,
            "token = \"0123456789abcdef0123\"\nprivileged_vmids = [106]\n\
             data_mount_roots = [\"/HDD18TB/media\"]\n",
        )
        .unwrap();
        let cfg = load_config_from(set.to_string_lossy().into_owned());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(cfg.safety.privileged_vmids, Some(vec![106]));
        assert_eq!(
            cfg.safety.data_mount_roots,
            Some(vec!["/HDD18TB/media".to_string()])
        );
    }

    /// fix-120: the token compare is the digest compare, and a wrong token of
    /// any length is refused.
    #[test]
    fn fix_120_the_bearer_check_refuses_every_near_miss() {
        let token = "0123456789abcdef0123";
        assert!(bearer_ok(Some("Bearer 0123456789abcdef0123"), token));
        for bad in [
            "Bearer 0123456789abcdef012",
            "Bearer 0123456789abcdef01234",
            "Bearer 0123456789abcdef0124",
            "Bearer ",
            "bearer 0123456789abcdef0123",
            "",
        ] {
            assert!(!bearer_ok(Some(bad), token), "{:?}", bad);
        }
    }

    /// fix-120: a connection refused for its token used to leave no trace at
    /// all, so a probe from a compromised container or a stolen token tried
    /// from a new machine was invisible. Every refusal is counted with the
    /// address it came from, for `homelab doctor`.
    #[tokio::test]
    async fn fix_120_a_refused_connection_is_counted_with_its_peer() {
        let path = format!("/tmp/homelab-fix120-router-{}.toml", std::process::id());
        std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
        let state = test_state(load_config_from(path));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = app_router(state.clone());
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });
        let refused = tokio_tungstenite::connect_async(format!("ws://{}/api/ws", addr)).await;
        assert!(refused.is_err(), "no token, no session");
        let seen = state.auth_failures.snapshot();
        assert_eq!(seen.count, 1, "the refusal is counted");
        assert!(
            seen.last_peer
                .as_deref()
                .unwrap_or("")
                .starts_with("127.0.0.1"),
            "and names where it came from: {:?}",
            seen.last_peer
        );
    }

    /// F186: the exact file that was live on 2026-09-02. Two OPNsense keys
    /// were appended after the last `[[registry_cache.upstreams]]` table, so
    /// TOML made them fields of the lscr.io mirror and serde dropped them —
    /// the daemon ran with `kea: None` and nobody could see why.
    #[test]
    fn misplaced_keys_are_reported_not_swallowed() {
        let raw = r#"
token = "0123456789abcdef0123"
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003

opnsense_url = "https://10.10.5.1"
opnsense_cred_file = "/var/lib/homelab/secrets/opnsense.cred"
"#;
        let t: toml::Table = toml::from_str(raw).unwrap();

        // Serde's own view: the file parses cleanly and says nothing. That
        // silence is what made the fault invisible, and it is still the
        // behaviour — which is why the check below has to exist. (The two
        // keys were removed from the daemon altogether on 2026-09-02, form
        // V4; the fixture stays as it was found, because the trap it
        // demonstrates belongs to TOML, not to those particular settings.)
        let _parsed: FileConfig = t.clone().try_into().unwrap();

        assert_eq!(
            unknown_keys(&t),
            vec![
                "registry_cache.upstreams[0].opnsense_cred_file".to_string(),
                "registry_cache.upstreams[0].opnsense_url".to_string(),
            ]
        );
    }

    /// The same settings written in the right place: nothing to report, and
    /// the daemon actually gets them.
    #[test]
    fn correctly_placed_keys_are_clean_and_read() {
        let raw = r#"
token = "0123456789abcdef0123"
kuma_monitors_file = "/appdata/uptime/kuma-seeder-config/host-monitors.json"
homepage_services_file = "/appdata/home/homepage-config/services.yaml"

[registry_cache]
host = "10.10.10.17"

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
        let t: toml::Table = toml::from_str(raw).unwrap();
        assert!(unknown_keys(&t).is_empty(), "{:?}", unknown_keys(&t));

        let parsed: FileConfig = t.try_into().unwrap();
        assert!(parsed.kuma_monitors_file.is_some());
        assert!(parsed.homepage_services_file.is_some());
    }

    /// A plain typo is loud too — the direction that costs a warning rather
    /// than a silence.
    #[test]
    fn a_typo_is_reported() {
        let t: toml::Table = toml::from_str("kuma_monitor_file = \"/x\"\n").unwrap();
        assert_eq!(unknown_keys(&t), vec!["kuma_monitor_file".to_string()]);
    }

    use super::*;

    /// Round-trip: everything load_config understands must survive a
    /// settings save. Guards the bug where a field the settings tab does not
    /// know about is silently dropped on save — first found with the OPNsense
    /// pair, which died on the next restart without any warning. Those two
    /// are gone (form V4, 2026-09-02); the guard is not, because the class of
    /// bug belongs to the save path rather than to those fields.
    #[test]
    fn settings_render_keeps_every_config_field() {
        let config = Config {
            restore_drill_interval_s: 90 * 24 * 3600,
            notify_fallback_webhook: Some("http://10.10.5.101:8123/api/webhook/homelab".into()),
            notify_fallback_auth_bearer: None,
            // F208: with real values, so the round-trip proves they SURVIVE
            // rather than only that the struct knows their names.
            watched_backups: vec![WatchedBackup {
                name: "opnsense-config".into(),
                rclone_path: "gdrive:homelab-backups/OPNSense-backups".into(),
                max_age_hours: 26,
            }],
            device_backups: vec![homelab_core::ops::devicebackup::DeviceBackup {
                name: "opnsense".into(),
                url: "https://10.10.10.1/api/core/backup/download/this".into(),
                cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
                filename: "config.xml".into(),
                pin: Some("sha256//abc".into()),
                ca_file: None,
            }],
            token: "0123456789abcdef0123".into(),
            listen: "0.0.0.0:8443".parse().unwrap(),
            state_dir: "/var/lib/homelab".into(),
            config_path: "/etc/homelab/host.toml".into(),
            exec_enabled: true,
            notify_auth_bearer: Some("a-token-that-must-survive-a-save".into()),
            prometheus_url: Some("http://10.10.10.13:9090".into()),
            loki_url: Some("http://10.10.10.4:3100".into()),
            logs_window: "6h".into(),
            mirror_remote: Some("git@github.com:k/m.git".into()),
            safety: SafetyConfig {
                no_touch: vec![100, 101],
                gateway_vmid: 112,
                gateway_routes_dir: "/appdata/platform/traefik-config/routes".into(),
                privileged_vmids: Some(vec![105, 106, 107]),
                data_mount_roots: Some(vec!["/HDD18TB/media".into()]),
            },
            registry_cache: Some(homelab_core::ops::registry_cache::CacheCfg {
                host: "10.10.10.17".into(),
                upstreams: vec![],
                pull_timeout_secs: 180,
            }),
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
            homepage_services_file: Some("/appdata/home/homepage-config/services.yaml".into()),
            kuma_monitors_file: Some(
                "/appdata/uptime/kuma-seeder-config/host-monitors.json".into(),
            ),
            backup_concurrency: 3,
            ask_timeout_s: 120,
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
        // G16: and so must the second route, or a save re-opens the exact
        // window Y2 carved out — kyu down while the orchestrator is the one
        // restarting it.
        assert_eq!(
            parsed.notify_fallback_webhook.as_deref(),
            Some("http://10.10.5.101:8123/api/webhook/homelab")
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
        assert_eq!(
            parsed.prometheus_url.as_deref(),
            Some("http://10.10.10.13:9090")
        );
        assert_eq!(
            parsed.notify_auth_bearer.as_deref(),
            Some("a-token-that-must-survive-a-save")
        );
        // F208: the three that a settings save used to wipe.
        assert!(
            parsed.registry_cache.is_some(),
            "the pull-through cache must survive a settings save"
        );
        assert_eq!(
            parsed.watched_backups.as_ref().map(|w| w.len()),
            Some(1),
            "the router-backup watch must survive a settings save"
        );
        assert_eq!(
            parsed.device_backups.as_ref().map(|d| d.len()),
            Some(1),
            "the device backups must survive a settings save"
        );
        assert_eq!(parsed.retention.as_ref().map(|r| r.len()), Some(3));
        assert_eq!(parsed.no_touch, Some(vec![100, 101]));
        // fix-120: the host policy survives a settings save.
        assert_eq!(parsed.privileged_vmids, Some(vec![105, 106, 107]));
        assert_eq!(
            parsed.data_mount_roots,
            Some(vec!["/HDD18TB/media".to_string()])
        );
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

    /// The daemon's shared state around `config`, as `main` builds it.
    fn test_state(config: Config) -> AppState {
        let (log_tx, _) = broadcast::channel(64);
        AppState::new(config, log_tx)
    }

    /// A session over a real socket on a free local port, with `handler` in
    /// place of `handle_rpc`. Returns the address to connect to.
    async fn serve_on_loopback<H, Fut>(handler: H) -> SocketAddr
    where
        H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
    {
        // Loaded by path, not through HOMELAB_CONFIG, for the reason fix-36's
        // test gives: tests run in parallel and another one sets it.
        // One file per call: tests run in parallel, and a shared path let one
        // test read the file while another was rewriting it.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = format!(
            "/tmp/homelab-loopback-test-{}-{}.toml",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
        let config = load_config_from(path.clone());
        let _ = std::fs::remove_file(&path);
        serve_state_on_loopback(test_state(config), handler).await
    }

    /// [`serve_on_loopback`] around a state the test built itself.
    async fn serve_state_on_loopback<H, Fut>(state: AppState, handler: H) -> SocketAddr
    where
        H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
    {
        let app = Router::new()
            .route(
                "/ws",
                get(
                    move |ws: WebSocketUpgrade, State(st): State<AppState>| async move {
                        ws.on_upgrade(move |s| serve_ws(s, st, handler))
                    },
                ),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        addr
    }

    /// Stands in for a deploy whose service check reaches a question: it asks
    /// through the real `LiveAsker` and reports the answer it got. `Answer`
    /// goes to the real handler, which is what delivers it.
    async fn asking_handler(st: AppState, req: RpcRequest) -> RpcResponse {
        if matches!(req.command, Rpc::Answer { .. }) {
            return handle_rpc(&st, req).await;
        }
        let asker = LiveAsker {
            state: &st,
            timeout_s: 5,
        };
        let q = homelab_core::ask::Question {
            op: "deploy-gateway".into(),
            step: "service checks".into(),
            what: "routes went from 29 to 28".into(),
            if_allowed: "the deploy goes on".into(),
            if_stopped: "the deploy stops".into(),
        };
        let answer = homelab_core::ask::Asker::ask(&asker, &q).await;
        RpcResponse {
            id: req.id,
            ok: answer.may_continue(),
            message: format!("{:?}", answer),
            deferred: None,
        }
    }

    /// covers: fix-66
    ///
    /// An operation started over a connection asks a question, and the
    /// operator answers over the SAME connection, which is what the TUI does.
    /// Until 2026-09-27 the session read the next frame only after the
    /// running request returned, so the answer sat unread behind the deploy
    /// that was waiting for it, and every question asked of a TUI-started
    /// operation ended as Unattended whatever was pressed
    /// (host-questions-unanswerable). Real socket, real session loop.
    #[tokio::test]
    async fn fix_66_an_answer_on_the_same_connection_reaches_the_waiting_operation() {
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let addr = serve_on_loopback(asking_handler).await;
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
            .await
            .expect("connect to the loopback session");
        let (mut tx, mut rx) = ws.split();
        let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
        tx.send(frame(RpcRequest {
            id: 1,
            command: Rpc::Ping,
        }))
        .await
        .unwrap();
        let verdict = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
                match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                    ServerMsg::Ask { id, .. } => {
                        tx.send(frame(RpcRequest {
                            id: 2,
                            command: Rpc::Answer { id, allow: true },
                        }))
                        .await
                        .unwrap();
                    }
                    ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                    _ => {}
                }
            }
            "the connection closed".to_string()
        })
        .await
        .expect("the operation never finished");
        assert_eq!(
            verdict, "Allow",
            "the operator allowed over the same connection, but the operation heard: {}",
            verdict
        );
    }

    /// fix-127 (expert panel, websocket-edge-cases, 2026-09-27): the session
    /// read `while let Some(Ok(Message::Text(..)))`, so the first Ping,
    /// Pong or Binary frame ended it. A keepalive ping from a client or a
    /// proxy closed the line without a word.
    #[tokio::test]
    async fn fix_127_a_ping_or_binary_frame_does_not_end_the_session() {
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let addr = serve_on_loopback(|st, req| async move { handle_rpc(&st, req).await }).await;
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
            .await
            .expect("connect to the loopback session");
        let (mut tx, mut rx) = ws.split();
        tx.send(WsMsg::Ping(vec![1, 2, 3].into())).await.unwrap();
        tx.send(WsMsg::Binary(vec![0xff; 8].into())).await.unwrap();
        let req = RpcRequest {
            id: 7,
            command: Rpc::Ping,
        };
        tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
        let answered = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(Ok(frame)) = rx.next().await {
                if let WsMsg::Text(t) = frame {
                    if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t) {
                        return r.id == 7 && r.ok;
                    }
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        assert!(
            answered,
            "the request after a ping and a binary frame is answered"
        );
    }

    /// Asks a question, and while it waits floods the broadcast channel far
    /// past its capacity, so a slow reader lags past the question.
    async fn flooding_asker(st: AppState, req: RpcRequest) -> RpcResponse {
        if matches!(req.command, Rpc::Answer { .. }) {
            return handle_rpc(&st, req).await;
        }
        // Spawned before the question is sent, so on this single-threaded
        // test runtime it runs as soon as the asker waits: after the Ask is
        // in the channel, before the session's forwarder has taken it out.
        let flood = st.log_tx.clone();
        tokio::spawn(async move {
            for i in 0..500 {
                let _ = flood.send(ServerMsg::Log {
                    level: homelab_proto::LogLevel::Debug,
                    source: "HOST".into(),
                    msg: format!("noise {}", i),
                });
            }
        });
        asking_handler(st, req).await
    }

    /// fix-127: a client that reads slower than the host writes lags the
    /// broadcast channel, and the forwarder skipped what it missed without
    /// a word, questions included, so the operation waited for an answer to
    /// a question the operator never saw and ended Unattended. Now the
    /// client is told how much it missed and every open question is sent
    /// again.
    #[tokio::test]
    async fn fix_127_a_lagging_client_still_gets_the_open_question() {
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let addr = serve_on_loopback(flooding_asker).await;
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
            .await
            .expect("connect to the loopback session");
        let (mut tx, mut rx) = ws.split();
        let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
        tx.send(frame(RpcRequest {
            id: 1,
            command: Rpc::Ping,
        }))
        .await
        .unwrap();
        let mut told_of_the_gap = false;
        let verdict = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
                match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                    ServerMsg::Ask { id, .. } => {
                        tx.send(frame(RpcRequest {
                            id: 2,
                            command: Rpc::Answer { id, allow: true },
                        }))
                        .await
                        .unwrap();
                    }
                    ServerMsg::Log { msg, .. } if msg.contains("dropped") => told_of_the_gap = true,
                    ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                    _ => {}
                }
            }
            "the connection closed".to_string()
        })
        .await
        .expect("the operation never finished");
        assert_eq!(verdict, "Allow", "the question reached the operator");
        assert!(
            told_of_the_gap,
            "and the client was told it missed messages"
        );
    }

    /// A state whose `state_dir` is a fresh directory of its own, holding a
    /// self-update marker armed at `armed_at`.
    fn state_with_marker(tag: &str, armed_at: u64) -> (AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("homelab-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = dir.join("host.toml");
        std::fs::write(
            &toml,
            format!(
                "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
                dir.display()
            ),
        )
        .unwrap();
        let marker = dir.join("selfupdate.pending");
        std::fs::write(
            &marker,
            format!("{{\"to_version\":\"9.9.9\",\"armed_at\":{}}}\n", armed_at),
        )
        .unwrap();
        (
            test_state(load_config_from(toml.to_string_lossy().into_owned())),
            marker,
        )
    }

    /// One Ping over a real session, answered.
    async fn ping_over(addr: SocketAddr) {
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
            .await
            .expect("connect to the loopback session");
        let (mut tx, mut rx) = ws.split();
        let req = RpcRequest {
            id: 1,
            command: Rpc::Ping,
        };
        tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
                if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t) {
                    assert!(r.ok);
                    return;
                }
            }
            panic!("the connection closed before the ping was answered");
        })
        .await
        .expect("the ping was answered");
        // The acceptance runs after the answer is queued; give it its turn.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    /// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): a new
    /// daemon was accepted after five seconds alive, before anything had
    /// talked to it, so a binary that ran but could not serve a client was
    /// kept. It is accepted now by the first authenticated request it
    /// answers.
    #[tokio::test]
    async fn fix_121_a_self_update_is_accepted_by_an_answered_request() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (state, marker) = state_with_marker("fix121-new", now - 100);
        let addr =
            serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await })
                .await;
        assert!(marker.exists(), "precondition: the update is pending");
        ping_over(addr).await;
        let accepted = !marker.exists();
        let _ = std::fs::remove_dir_all(marker.parent().unwrap());
        assert!(
            accepted,
            "an answered request from the new daemon accepts the update"
        );
    }

    /// fix-121: the daemon that ARMED the marker answers the self-update
    /// request itself; that answer must not accept the binary that replaces
    /// it.
    #[tokio::test]
    async fn fix_121_the_old_daemon_never_accepts_its_successor() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (state, marker) = state_with_marker("fix121-old", now + 100);
        let addr =
            serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await })
                .await;
        ping_over(addr).await;
        let kept = marker.exists();
        let _ = std::fs::remove_dir_all(marker.parent().unwrap());
        assert!(kept, "armed after this daemon started: not its own");
    }

    /// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): a remote
    /// exec is recorded in audit.log and the journal before it runs, and was
    /// recorded verbatim, so a password typed on the command line was kept
    /// on pve indefinitely. The record still names the vmid and the command,
    /// with the values the shared masker recognises taken out.
    #[test]
    fn fix_124_an_exec_command_is_masked_before_it_is_recorded() {
        let line = exec_audit_line(
            1_800_000_000,
            108,
            "PGPASSWORD=hunter2-x9 psql -h db -c 'select 1'; curl -H 'Authorization: Bearer tk-77' x",
        );
        assert!(
            !line.contains("hunter2-x9") && !line.contains("tk-77"),
            "{}",
            line
        );
        assert!(
            line.starts_with("1800000000 exec vmid=108 ") && line.contains("psql -h db"),
            "the record still says what ran where: {}",
            line
        );
        assert!(
            line.ends_with('\n') && line.lines().count() == 1,
            "{:?}",
            line
        );
    }

    /// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27):
    /// audit.log (every exec command) and the incident bundles were readable
    /// by any local account on pve. A new audit.log is created 0600, and at
    /// start the daemon takes group and world access off what exists.
    #[test]
    fn fix_125_the_audit_log_and_existing_bundles_are_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let set = |p: &std::path::Path, m: u32| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        let dir = std::env::temp_dir().join(format!("homelab-fix125-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bundle = dir.join("incidents/1800000000-deploy-x");
        std::fs::create_dir_all(&bundle).unwrap();
        for (f, m) in [("report.json", 0o644), ("commands.sh", 0o755)] {
            std::fs::write(bundle.join(f), "x").unwrap();
            set(&bundle.join(f), m);
        }
        set(&dir.join("incidents"), 0o755);
        set(&bundle, 0o755);
        std::fs::write(dir.join("audit.log"), "old\n").unwrap();
        set(&dir.join("audit.log"), 0o644);

        tighten_private_paths(&dir.to_string_lossy());
        let fresh = dir.join("fresh-audit.log");
        append_audit(&fresh.to_string_lossy(), "1 exec vmid=108 cmd=\"ls\"\n").unwrap();

        let got = [
            mode(&dir.join("incidents")),
            mode(&bundle),
            mode(&bundle.join("report.json")),
            mode(&bundle.join("commands.sh")),
            mode(&dir.join("audit.log")),
            mode(&fresh),
        ];
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, [0o700, 0o700, 0o600, 0o700, 0o600, 0o600]);
    }

    /// One plain HTTP GET against `addr`, with an optional bearer token;
    /// returns the status code and the body.
    async fn http_get(addr: SocketAddr, path: &str, token: Option<&str>) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {}\r\n", t))
            .unwrap_or_default();
        s.write_all(
            format!(
                "GET {} HTTP/1.1\r\nHost: x\r\n{}Connection: close\r\n\r\n",
                path, auth
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        let code = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (code, body)
    }

    /// fix-126 (expert panel, unauth-version-plaintext-kyu, 2026-09-27):
    /// `/api/version` told any neighbour which release runs, which is what a
    /// probe needs to pick a known fault. It now takes the token like the
    /// line itself; `/api/health` stays open, it says only `ok`.
    #[tokio::test]
    async fn fix_126_the_version_endpoint_needs_the_token() {
        let path = format!("/tmp/homelab-fix126-{}.toml", std::process::id());
        std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
        let state = test_state(load_config_from(path));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = app_router(state.clone());
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });
        let (code, body) = http_get(addr, "/api/version", None).await;
        assert_eq!(code, 401, "no token, no version: {}", body);
        assert!(!body.contains(VERSION), "{}", body);
        assert_eq!(state.auth_failures.snapshot().count, 1, "and it is counted");
        let (code, body) = http_get(addr, "/api/version", Some("0123456789abcdef0123")).await;
        assert_eq!((code, body.trim()), (200, VERSION));
        assert_eq!(http_get(addr, "/api/health", None).await.0, 200);
    }

    /// fix-126: a notification route that sends the bearer token over plain
    /// HTTP is named at start. The token to kyu crossed VLAN 10 in clear
    /// text, where an ARP-spoofing container reads it.
    #[test]
    fn fix_126_a_bearer_sent_over_plain_http_is_named() {
        let routes = plaintext_bearer_routes(
            Some("http://10.10.10.9:8080/publish/notify.kenny"),
            Some("tok"),
            Some("http://10.10.5.101:8123/api/webhook/abc"),
            None,
        );
        assert_eq!(routes.len(), 1, "{:?}", routes);
        assert!(
            routes[0].starts_with("http://10.10.10.9:8080"),
            "{:?}",
            routes
        );
        assert!(
            !routes[0].contains("notify.kenny"),
            "path withheld: {:?}",
            routes
        );
        assert!(plaintext_bearer_routes(
            Some("https://kyu.example/publish"),
            Some("tok"),
            None,
            None
        )
        .is_empty());
        assert!(
            plaintext_bearer_routes(Some("http://127.0.0.1:8080/x"), Some("tok"), None, None)
                .is_empty(),
            "loopback never crosses a wire"
        );
    }

    /// A `MakeWriter` into a shared buffer, to read what the journal gets.
    #[derive(Clone, Default)]
    struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// fix-122 (expert panel, journal-lines-lack-op-context, 2026-09-27):
    /// three backups run at once at night, and their journal lines carried
    /// only `source=HOST` and the message, so `image not found` could not be
    /// tied to one of them; the lines also carried ANSI colour codes, which
    /// break `grep` on the journal. Every line of an operation now names the
    /// operation and its stack, steps are logged as they start and finish,
    /// and no escape code reaches the journal.
    #[test]
    fn fix_122_journal_lines_name_their_operation_and_stack_without_colour() {
        use homelab_core::runner::{Runner, StepOutcome};
        use tracing::Instrument;
        let out = Captured::default();
        let subscriber = journal_subscriber(out.clone(), "info");
        let dir = std::env::temp_dir().join(format!("homelab-fix122-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let toml = dir.join("host.toml");
        std::fs::write(
            &toml,
            format!(
                "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
                dir.display()
            ),
        )
        .unwrap();
        let state = test_state(load_config_from(toml.to_string_lossy().into_owned()));
        tracing::subscriber::with_default(subscriber, || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(
                    async {
                        run_op_locked(&state, &RealExecutor, 0, "scheduled-backup", |ctx| {
                            Box::pin(async move {
                                let mut r = Runner::new("backup-paperwork", ctx.sink, ctx.journal);
                                let _ = r
                                    .step("snapshot", || async {
                                        ctx.sink.emit(PipelineEvent::Line {
                                            level: homelab_core::sink::Level::Warn,
                                            source: "HOST".into(),
                                            msg: "image not found".into(),
                                        });
                                        Ok(StepOutcome::Changed)
                                    })
                                    .await;
                                r.finish_ok()
                            })
                        })
                        .await
                    }
                    .instrument(stack_span("paperwork")),
                );
        });
        let _ = std::fs::remove_dir_all(&dir);
        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        assert!(!text.contains('\u{1b}'), "no colour codes:\n{}", text);
        let line = text
            .lines()
            .find(|l| l.contains("image not found"))
            .unwrap_or_else(|| panic!("the line reached the journal:\n{}", text));
        assert!(
            line.contains("stack=paperwork") && line.contains("op=scheduled-backup"),
            "the line names its stack and operation: {}",
            line
        );
        assert!(
            text.lines()
                .any(|l| l.contains("snapshot") && l.contains("stack=paperwork")),
            "the step's start and finish are journal lines too:\n{}",
            text
        );
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

    /// G14: the drill rides the backup hour and only when it is due.
    #[test]
    fn g14_the_restore_drill_is_planned_only_in_the_backup_hour_and_only_when_due() {
        let never = NightlyState {
            last_host_meta: 0,
            last_zfs: 0,
            last_restore_drill: 0,
            restore_drill_interval_s: 90 * 24 * 3600,
            zfs_configured: false,
            devices_configured: false,
        };
        assert!(
            nightly_plan(4, 4, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
            "never drilled, and it is the hour: it must be planned"
        );
        assert!(
            !nightly_plan(4, 5, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
            "a restore pulls a whole snapshot back — not outside the backup hour"
        );
        let fresh = NightlyState {
            last_restore_drill: 1_000_000 - 3600,
            ..never
        };
        assert!(
            !nightly_plan(4, 4, 1_000_000, &[], &fresh).contains(&NightlyTask::RestoreDrill),
            "drilled an hour ago: not again tonight"
        );
    }

    /// F259 · a watcher pointed at a path no writer writes to.
    ///
    /// The live config had exactly this on 2026-09-03: the watcher looked at
    /// `OPNSense-backups` while the device backup wrote to `opnsense-config`,
    /// so it would have gone on reporting "holds no files at all" even after
    /// the backup started working — and whoever read that would have
    /// concluded the fix had failed.
    #[test]
    fn f259_a_watcher_that_matches_no_writer_is_named() {
        let dev = homelab_core::ops::devicebackup::DeviceBackup {
            name: "opnsense".into(),
            url: "https://10.10.10.1/api/core/backup/download/this".into(),
            cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
            filename: "config.xml".into(),
            pin: Some("sha256//abc".into()),
            ca_file: None,
        };
        let base = "rclone:gdrive:homelab-backups";

        for path in [
            // The repo root, spelled the way restic spells it.
            format!("{}/opnsense-config", base),
            // The same repo without restic's remote prefix, which is how
            // rclone itself must be given it.
            "gdrive:homelab-backups/opnsense-config".to_string(),
            // And the shape that actually works: the snapshots directory,
            // because the repo root holds `config`, written once at init, so
            // watching the root reports the creation date forever.
            "gdrive:homelab-backups/opnsense-config/snapshots".to_string(),
        ] {
            let w = WatchedBackup {
                name: "opnsense-config".into(),
                rclone_path: path.clone(),
                max_age_hours: 26,
            };
            assert!(
                orphan_watchers(&[w], std::slice::from_ref(&dev), base).is_empty(),
                "{} is the writer's own repository and must not be called an orphan — \
                 a guard that cries wolf on a correct config teaches its reader to \
                 skip the line",
                path
            );
        }

        let stray = WatchedBackup {
            name: "opnsense-config".into(),
            rclone_path: format!("{}/OPNSense-backups", base),
            max_age_hours: 26,
        };
        let out = orphan_watchers(&[stray], std::slice::from_ref(&dev), base);
        assert_eq!(out.len(), 1, "the live misconfiguration must be named");
        assert!(out[0].contains("OPNSense-backups"), "{:?}", out);

        // With nothing configured to write, every watcher is an orphan and
        // saying so is the point — that was the state for weeks.
        assert_eq!(
            orphan_watchers(&[stray_clone()], &[], base).len(),
            1,
            "no writers at all means the watcher watches nothing"
        );
    }

    fn stray_clone() -> WatchedBackup {
        WatchedBackup {
            name: "opnsense-config".into(),
            rclone_path: "rclone:gdrive:homelab-backups/OPNSense-backups".into(),
            max_age_hours: 26,
        }
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
        let plan = nightly_plan(
            4,
            4,
            now,
            &[("a".into(), true, fresh)],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
            },
        );
        assert_eq!(plan, vec![NightlyTask::HostMeta]);

        // Due stacks come first, host-meta closes the run.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[("a".into(), true, stale), ("b".into(), true, stale)],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
            },
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
        let plan = nightly_plan(
            4,
            4,
            now,
            &[("a".into(), true, fresh)],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: fresh,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
            },
        );
        assert!(plan.is_empty());

        // Wrong hour: nothing at all.
        assert!(nightly_plan(
            4,
            5,
            now,
            &[("a".into(), true, stale)],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
            }
        )
        .is_empty());

        // H8: a parked stack sits out, but the host-meta backup does not
        // depend on any stack being active.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[("a".into(), false, stale)],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
            },
        );
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
    }

    #[test]
    fn e8_zfs_only_when_configured() {
        let now = 1_800_000_000u64;
        let stale = now - 25 * 3600;
        // No jobs declared → the feature is simply off.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: stale,
                zfs_configured: false,
                devices_configured: false,
            },
        );
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
        // Declared → runs once a night, after the host-meta snapshot.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: stale,
                zfs_configured: true,
                devices_configured: false,
            },
        );
        assert_eq!(plan, vec![NightlyTask::HostMeta, NightlyTask::Zfs]);
        // Already ran this cycle → not repeated on the next 20-min tick.
        let plan = nightly_plan(
            4,
            4,
            now,
            &[],
            &NightlyState {
                last_restore_drill: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: now - 3600,
                zfs_configured: true,
                devices_configured: false,
            },
        );
        assert_eq!(plan, vec![NightlyTask::HostMeta]);
    }

    #[test]
    fn y1_a_configured_zero_never_means_no_backups() {
        // The dangerous reading: "0 = unlimited" to a person, "0 = none" to
        // the stream. A night with no backups at all, and the only trace is
        // last_backup standing still.
        assert_eq!(effective_concurrency(0), 1, "zero must not mean none");
        assert_eq!(effective_concurrency(1), 1);
        assert_eq!(effective_concurrency(3), 3, "Kenny's choice, form Y4");
        assert_eq!(effective_concurrency(13), 13);
    }

    #[test]
    fn y1_the_default_is_the_number_kenny_chose() {
        assert_eq!(
            default_backup_concurrency(),
            3,
            "form Y4: about a third of the wait, never more than three \
             services quiet at once"
        );
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
                native_only: false,
                syslog_receivers: vec![],
                natives: Vec::new(),
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
                incomplete_step: None,
                route_file: None,
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
                incomplete_step: None,
                route_file: None,
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
        // fix-53 (expert panel, timeout-leaves-container-work-running,
        // 2026-09-27): the command leads a process group of its own, and a
        // timeout kills that whole group. `kill_on_drop` alone reached only
        // the direct child (`pct`), not the `lxc-attach` and script it had
        // started, so a "timed out" step went on changing the container.
        let child = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .map_err(|e| CoreError::Other(format!("spawn {}: {}", rendered, e)))?;
        let group = child.id();
        let out = match tokio::time::timeout(
            Duration::from_secs(cmd.timeout_s),
            child.wait_with_output(),
        )
        .await
        {
            Ok(out) => out.map_err(|e| CoreError::Other(format!("wait {}: {}", rendered, e)))?,
            Err(_) => {
                if let Some(pgid) = group.and_then(|p| i32::try_from(p).ok()) {
                    // SAFETY: killpg only sends a signal; the group is the one
                    // this call created for the child it spawned.
                    unsafe {
                        libc::killpg(pgid, libc::SIGKILL);
                    }
                }
                return Err(CoreError::Timeout {
                    rendered: rendered.clone(),
                    seconds: cmd.timeout_s,
                });
            }
        };
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
            // fix-51 (expert panel, state-writes-race, 2026-09-27): a temp
            // name of its own per write. The fixed `<path>.tmp` let two
            // writers of one path (three nightly backups recording their
            // notification outcome, or writing the notify header file) remove
            // each other's temp file or fail on EEXIST.
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = format!(
                "{}.{}.{}.tmp",
                path,
                std::process::id(),
                SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            {
                // Created with its final mode: a secret written into a file
                // that is world-readable until the chmod below is readable in
                // that window. `mode()` applies at creation (minus umask),
                // and the explicit set_permissions afterwards still fixes the
                // exact bits.
                use std::os::unix::fs::OpenOptionsExt;
                let _ = std::fs::remove_file(&tmp);
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(mode)
                    .open(&tmp)
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.write_all(content.as_bytes())
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.sync_all().map_err(|e| CoreError::State(e.to_string()))?;
            }
            let placed = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
                .and_then(|()| std::fs::rename(&tmp, &path));
            if let Err(e) = placed {
                let _ = std::fs::remove_file(&tmp);
                return Err(CoreError::State(e.to_string()));
            }
            // fix-51: the rename is only durable once the directory entry
            // is; without this a power cut can bring the old file back.
            if let Some(parent) = p.parent() {
                if let Ok(d) = std::fs::File::open(parent) {
                    let _ = d.sync_all();
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Other(e.to_string()))?
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        // fix-50: absence and unreadability are different answers; the state
        // store may only treat the first as a fresh install.
        tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::NotFound(format!("{}: {}", path, e))
            } else {
                CoreError::State(format!("{}: {}", path, e))
            }
        })
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
                // fix-122: at the line's own level, so `grep WARN` on the
                // journal finds the warnings an operation printed.
                use homelab_core::sink::Level;
                match level {
                    Level::Error => tracing::error!(source = %source, "{}", msg),
                    Level::Warn => tracing::warn!(source = %source, "{}", msg),
                    Level::Debug => tracing::debug!(source = %source, "{}", msg),
                    Level::Info => tracing::info!(source = %source, "{}", msg),
                }
                ServerMsg::Log {
                    level: level.into(),
                    source,
                    msg,
                }
            }
            // fix-122: step starts and ends reach the journal too; they went
            // only to connected clients, so a night's journal had the lines
            // of a step but not which step they belonged to.
            PipelineEvent::StepStarted { op, step } => {
                let msg = format!("[sync][run ] {} :: {}", op, step);
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                }
            }
            PipelineEvent::StepFinished { op, step, changed } => {
                let msg = format!(
                    "[sync][exit] {} :: {} :: {}",
                    op,
                    step,
                    if changed { "changed" } else { "ok (no change)" }
                );
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                }
            }
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
    /// T69: questions a step is waiting on, by id. The client's answer
    /// arrives as an ordinary RPC and is delivered through one of these.
    pending_asks: Arc<std::sync::Mutex<std::collections::HashMap<u64, PendingAsk>>>,
    /// Monotonic id for those questions. Not a clock: two questions in the
    /// same second must still be distinguishable.
    next_ask_id: Arc<std::sync::atomic::AtomicU64>,
    /// fix-120: connections refused for their token since this daemon
    /// started, for `homelab doctor`.
    auth_failures: Arc<AuthFailures>,
    /// fix-121: when this daemon started. A self-update marker armed before
    /// this moment names this binary as the new one; one armed later was
    /// armed by this daemon for its successor.
    started_at: u64,
}

impl AppState {
    fn new(config: Config, log_tx: broadcast::Sender<ServerMsg>) -> Self {
        AppState {
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            settings: Arc::new(std::sync::RwLock::new(config.initial_settings.clone())),
            config,
            log_tx,
            op_lock: Arc::new(Mutex::new(())),
            pending_asks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            next_ask_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            damper: Arc::new(std::sync::Mutex::new(
                homelab_core::notify::NotifyDamper::new(20 * 3600),
            )),
            auth_failures: Arc::new(AuthFailures::default()),
        }
    }
}

/// T69: a question waiting for its answer.
struct PendingAsk {
    reply: tokio::sync::oneshot::Sender<bool>,
    /// fix-127: the question as sent, so a client that lagged past it gets
    /// it again.
    ask: ServerMsg,
}

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): the 401 branch
/// logged nothing, so a probe from a compromised container or a stolen token
/// tried from a new machine left no trace. Kept in memory: a flood of bad
/// attempts must not turn into a flood of state writes.
#[derive(Default)]
struct AuthFailures {
    count: std::sync::atomic::AtomicU64,
    last: std::sync::Mutex<Option<(String, u64)>>,
}

impl AuthFailures {
    /// Count one refusal from `peer` and return the running total.
    fn record(&self, peer: &str, now: u64) -> u64 {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Some((peer.to_string(), now));
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
    }

    /// What has been seen, in the shape doctor reads.
    fn snapshot(&self) -> homelab_core::doctor::FailedAuth {
        let last = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        homelab_core::doctor::FailedAuth {
            count: self.count.load(std::sync::atomic::Ordering::Relaxed),
            last_peer: last.as_ref().map(|(p, _)| p.clone()),
            last_at: last.map(|(_, t)| t).unwrap_or(0),
        }
    }
}

/// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): accept a
/// pending self-update once this daemon has answered an authenticated
/// request. It used to be accepted after five seconds alive, before the TLS
/// line had carried anything, so a binary that ran but could not serve a
/// client was kept and the client could not ship the next fix.
///
/// Until a request is answered the marker stays armed, so a daemon that dies
/// or crash-loops meanwhile is rolled back by the OnFailure unit as before.
/// Only a marker armed before this daemon started is its own: the daemon that
/// armed it answers the self-update request itself, and that answer must not
/// accept its successor.
fn accept_pending_update(state_dir: &str, started_at: u64) {
    let marker = format!("{}/selfupdate.pending", state_dir);
    let Ok(raw) = std::fs::read_to_string(&marker) else {
        return;
    };
    let armed_at = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("armed_at").and_then(|a| a.as_u64()))
        .unwrap_or(0);
    if armed_at >= started_at {
        return;
    }
    match std::fs::remove_file(&marker) {
        Ok(()) => info!(
            "self-update accepted — v{} answered an authenticated request",
            VERSION
        ),
        Err(e) => tracing::warn!("self-update: could not clear {} :: {}", marker, e),
    }
}

/// fix-126 (expert panel, unauth-version-plaintext-kyu, 2026-09-27):
/// notification routes that send a bearer token over plain HTTP to another
/// machine, shown as `route_for_log` shows them. The kyu publish token
/// crossed VLAN 10 in clear text, where any container that can ARP-spoof
/// reads it. Moving the route to TLS is a change on the machines, so the
/// daemon says it at start rather than refusing.
fn plaintext_bearer_routes(
    primary: Option<&str>,
    primary_bearer: Option<&str>,
    fallback: Option<&str>,
    fallback_bearer: Option<&str>,
) -> Vec<String> {
    let loopback = |url: &str| {
        let host = homelab_core::notify::route_for_log(url);
        let host = host.trim_start_matches("http://");
        host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]")
    };
    [(primary, primary_bearer), (fallback, fallback_bearer)]
        .into_iter()
        .filter_map(|(url, bearer)| Some((url?, bearer?)))
        .filter(|(url, _)| url.starts_with("http://") && !loopback(url))
        .map(|(url, _)| homelab_core::notify::route_for_log(url))
        .collect()
}

/// The daemon's routes. One function for `main` and the tests, so a test of
/// the 401 path runs the real one.
fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(version_endpoint))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state)
}

/// fix-126: the version for a caller holding the token. It was open, and it
/// told any neighbour on the VLAN which release runs.
async fn version_endpoint(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> axum::response::Response {
    if bearer_ok(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
    ) {
        VERSION.into_response()
    } else {
        log_refused(&state, peer, "/api/version");
        (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response()
    }
}

/// T69: the asker that reaches a watching operator over the live line.
///
/// Two things make it safe to use from code that also runs unattended.
/// First, it checks whether ANYONE is subscribed before it waits at all —
/// the nightly round at 04:00 has no client, so it answers immediately
/// instead of burning a timeout per question. Second, when somebody is
/// listening but nobody answers, it gives up after a bounded wait and says
/// `Unattended`, which is deliberately not the same answer as `Stop`.
struct LiveAsker<'a> {
    state: &'a AppState,
    timeout_s: u64,
}

#[async_trait::async_trait]
impl homelab_core::ask::Asker for LiveAsker<'_> {
    async fn ask(&self, q: &homelab_core::ask::Question) -> homelab_core::ask::Answer {
        use homelab_core::ask::Answer;
        if self.state.log_tx.receiver_count() == 0 {
            return Answer::Unattended("no client is connected to answer".into());
        }
        let id = self
            .state
            .next_ask_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ask = ServerMsg::Ask {
            id,
            op: q.op.clone(),
            step: q.step.clone(),
            what: q.what.clone(),
            if_allowed: q.if_allowed.clone(),
            if_stopped: q.if_stopped.clone(),
        };
        if let Ok(mut g) = self.state.pending_asks.lock() {
            g.insert(
                id,
                PendingAsk {
                    reply: tx,
                    ask: ask.clone(),
                },
            );
        }
        let _ = self.state.log_tx.send(ask);
        let answer = match tokio::time::timeout(Duration::from_secs(self.timeout_s), rx).await {
            Ok(Ok(true)) => Answer::Allow,
            Ok(Ok(false)) => Answer::Stop,
            // The sender was dropped: the client disconnected mid-question.
            Ok(Err(_)) => Answer::Unattended("the client went away before answering".into()),
            Err(_) => Answer::Unattended(format!(
                "nobody answered within {}s — the operation did not guess",
                self.timeout_s
            )),
        };
        if let Ok(mut g) = self.state.pending_asks.lock() {
            g.remove(&id);
        }
        answer
    }
}

/// The daemon's log subscriber, writing to `writer` (stderr, which systemd
/// puts in the journal).
///
/// fix-122 (expert panel, journal-lines-lack-op-context, 2026-09-27): no
/// ANSI colour. tracing-subscriber colours by default, so a journal line
/// read `\x1b[33m WARN\x1b[0m` and `grep 'WARN scheduler'` found nothing.
fn journal_subscriber<W>(writer: W, default_filter: &str) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with_ansi(false)
        .with_writer(writer)
        .finish()
}

/// fix-122: the span that names the stack an operation works on. Every
/// journal line inside it carries `stack=<name>`, which is what tells three
/// concurrent nightly backups apart.
fn stack_span(stack: &str) -> tracing::Span {
    tracing::info_span!("stack", stack = %stack)
}

/// fix-122: the span a request runs in: its id, and its stack when it has
/// one (a request about the host or the fleet has none, and says nothing).
fn rpc_span(req: &RpcRequest) -> tracing::Span {
    let span = tracing::info_span!("rpc", id = req.id, stack = tracing::field::Empty);
    if let Some(stack) = rpc_stack(&req.command) {
        span.record("stack", stack.as_str());
    }
    span
}

/// fix-122: the stack a request is about, for its span. `None` for requests
/// about the host or the whole fleet.
fn rpc_stack(command: &Rpc) -> Option<String> {
    match command {
        Rpc::DeployStack(spec) => Some(spec.manifest.stack_name.clone()),
        Rpc::DestroyStack { manifest, .. }
        | Rpc::RestoreStack { manifest, .. }
        | Rpc::UpdateStack { manifest, .. }
        | Rpc::PruneOrphans { manifest, .. } => Some(manifest.stack_name.clone()),
        Rpc::BackupStack(m) | Rpc::ApplyResources(m) => Some(m.stack_name.clone()),
        Rpc::StageNativeBinary { stack, .. }
        | Rpc::BackupNative { stack }
        | Rpc::UpdateNative { stack }
        | Rpc::ReleaseUpdateNative { stack }
        | Rpc::ForgetStack { stack }
        | Rpc::DestroyRecorded { stack, .. }
        | Rpc::SetStackEnabled { stack, .. }
        | Rpc::GetApplied { stack } => Some(stack.clone()),
        Rpc::InstallNative { manifest, .. } | Rpc::AdoptService(manifest) => {
            Some(manifest.stack_name.clone())
        }
        _ => None,
    }
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

/// Run the daemon until something ends it, and say with which exit code.
///
/// fix-52 (expert panel, background-tasks-unsupervised, 2026-09-27): main
/// used to await the server alone. The scheduler's handle was dropped, so a
/// panic in it ended every nightly backup for good while the daemon went on
/// serving and feeding the watchdog; and nothing handled SIGTERM, so
/// `systemctl stop` or a self-update restart killed a step mid-way.
///
/// - The scheduler ending in any way is a failure (exit 1): systemd's
///   `Restart=always` brings the daemon back with a live scheduler.
/// - SIGTERM waits for the operation holding `op_lock`, at most `drain`,
///   then exits 0 (adoption norm N1). The lock is fair, so no operation
///   queued after the signal starts.
async fn supervise<S>(
    serve: S,
    scheduler: tokio::task::JoinHandle<()>,
    shutdown: impl std::future::Future<Output = ()>,
    op_lock: Arc<Mutex<()>>,
    drain: Duration,
) -> i32
where
    S: std::future::Future<Output = std::io::Result<()>>,
{
    tokio::select! {
        served = serve => {
            match served {
                Ok(()) => error!("server stopped without an error — exiting so systemd restarts it"),
                Err(e) => error!("server stopped :: {}", e),
            }
            1
        }
        ended = scheduler => {
            let how = match ended {
                Err(e) if e.is_panic() => "panicked",
                Err(_) => "was cancelled",
                Ok(()) => "returned",
            };
            error!(
                "scheduler task {} — exiting so systemd restarts the daemon with a live scheduler",
                how
            );
            1
        }
        () = shutdown => {
            info!(
                "SIGTERM: waiting up to {}s for the running operation to finish",
                drain.as_secs()
            );
            match tokio::time::timeout(drain, op_lock.lock_owned()).await {
                Ok(guard) => {
                    // Held until the process exits: nothing starts after this.
                    std::mem::forget(guard);
                    info!("SIGTERM: no operation running — exiting");
                }
                Err(_) => tracing::warn!(
                    "SIGTERM: an operation is still running after {}s — exiting anyway; the next start reports it as interrupted",
                    drain.as_secs()
                ),
            }
            0
        }
    }
}

/// fix-52: resolves on SIGTERM. If the handler cannot be installed the
/// daemon keeps running as it did before, and says so.
async fn terminate_signal() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            tracing::warn!("no SIGTERM handler ({}) — a stop kills the running step", e);
            std::future::pending::<()>().await;
        }
    }
}

#[tokio::main]
async fn main() {
    // H5: the self-update gate runs `staged --selfcheck` before installing.
    // Prove we can execute at all and report our version, then exit.
    // `--version` does the same for a person. Any other argument stops the
    // daemon before it opens a port: on 2026-09-27 `homelab-host --version`
    // run by hand on pve started a second daemon that held 8443 while systemd
    // restarted the real one (corr-rogue-daemon).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {}
        Some("--selfcheck") | Some("--version") if args.len() == 1 => {
            println!("{}", VERSION);
            std::process::exit(0);
        }
        Some(_) => {
            eprintln!(
                "homelab-host: unknown argument(s) {:?}; it takes none (only --version or --selfcheck) and is started by systemd",
                args
            );
            std::process::exit(2);
        }
    }

    {
        use tracing_subscriber::util::SubscriberInitExt as _;
        journal_subscriber(std::io::stderr, "info").init();
    }

    let config = load_config();
    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState::new(config.clone(), log_tx);

    // fix-125: records written before the private modes existed.
    tighten_private_paths(&config.state_dir);

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
            let err = if interrupted.is_empty() {
                None
            } else {
                Some(format!("interrupted: {}", interrupted.join("; ")))
            };
            let payload = homelab_core::notify::op_payload(
                "host-online",
                "boot",
                interrupted.is_empty(),
                err.as_deref(),
                VERSION,
            );
            notify_raw(&boot_state, &RealExecutor, payload).await;
        });
    }

    // E4: nightly scheduler — backups for every managed stack + auto-policy
    // updates, driven from state.json manifests (no client needed). Reads the
    // live settings each tick, so G8 edits apply without a restart.
    // fix-52: the handle is kept; `supervise` ends the process if it ends.
    let scheduler = {
        let sched_state = state.clone();
        let handle = tokio::spawn(async move { scheduler_loop(sched_state).await });
        match config.initial_settings.backup_hour {
            Some(hour) => info!(
                "scheduler armed: daily backup + auto-updates at {:02}:00",
                hour
            ),
            None => info!("scheduler idle (backup_hour not set)"),
        }
        handle
    };
    let op_lock = state.op_lock.clone();

    let app = app_router(state);

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

    // H5 / fix-121: a pending self-update is accepted by the first
    // authenticated request this daemon answers (`accept_pending_update`),
    // no longer by five seconds alive. Until then the marker stays armed, so
    // a binary that binds and then dies is still rolled back by OnFailure.
    if std::path::Path::new(&format!("{}/selfupdate.pending", config.state_dir)).exists() {
        info!(
            "self-update pending — v{} is accepted by the first authenticated request it \
             answers; until then a crash-loop rolls it back",
            VERSION
        );
    }

    // fix-52: bind first, so READY=1 means the port is open. It was sent
    // before the socket existed, and a bind failure then looked like a
    // daemon that had started.
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .and_then(|l| l.into_std())
        .expect("bind listen address");
    let server = axum_server::from_tcp_rustls(listener, tls_config).expect("serve");

    // B7: tell systemd we're ready, then feed its watchdog. If this loop
    // ever stops (deadlock/hang), systemd kills and restarts the daemon.
    sd_notify("READY=1");
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            sd_notify("WATCHDOG=1");
        }
    });

    let code = supervise(
        // fix-120: with the peer address, so a refused connection says where
        // it came from.
        server.serve(app.into_make_service_with_connect_info::<SocketAddr>()),
        scheduler,
        terminate_signal(),
        op_lock,
        Duration::from_secs(60),
    )
    .await;
    std::process::exit(code);
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
    /// G14: rehearse a restore from one repository, in turn.
    ///
    /// B3 asked for a quarterly trial restore and it was never built. Kenny
    /// declined "write it down as a known limitation" at the Phase-7 gate, so
    /// it rides the round that already runs — a backup nobody has restored is
    /// a hypothesis, and one done by hand on a day somebody thought of it is
    /// a hypothesis with a date on it.
    RestoreDrill,
    /// Route A: ask a device that is on the no-touch list for its own
    /// configuration and store the answer. Rides with the host-meta slot
    /// rather than a slot of its own — it is one small GET, and a device
    /// whose config changed today is exactly a night the vault changed too.
    DeviceConfig,
}

/// H12 pattern: the whole nightly decision as a pure function, so "does the
/// host-meta backup actually run?" is a test instead of an assumption.
/// `stacks` is (name, enabled, last_backup).
/// What the night needs to know beyond the clock and the stacks: when the
/// host-wide jobs last ran, and which of them exist at all. Grouped because
/// the argument list had grown past the point where a caller could get the
/// order right by reading it.
struct NightlyState {
    last_host_meta: u64,
    last_zfs: u64,
    zfs_configured: bool,
    /// G14: when the last restore drill proved something, and how long a
    /// passed drill counts for.
    last_restore_drill: u64,
    restore_drill_interval_s: u64,
    devices_configured: bool,
}

fn nightly_plan(
    cfg_hour: u8,
    local_hour: u8,
    now: u64,
    stacks: &[(String, bool, u64)],
    st: &NightlyState,
) -> Vec<NightlyTask> {
    let mut plan = Vec::new();
    for (name, enabled, last_backup) in stacks {
        // H8: parked stacks sit out the nightly rotation entirely.
        if *enabled && backup_due(cfg_hour, local_hour, *last_backup, now) {
            plan.push(NightlyTask::Stack(name.clone()));
        }
    }
    if backup_due(cfg_hour, local_hour, st.last_host_meta, now) {
        plan.push(NightlyTask::HostMeta);
    }
    if st.zfs_configured && backup_due(cfg_hour, local_hour, st.last_zfs, now) {
        plan.push(NightlyTask::Zfs);
    }
    if st.devices_configured && backup_due(cfg_hour, local_hour, st.last_host_meta, now) {
        plan.push(NightlyTask::DeviceConfig);
    }
    // G14: at most one per round, and only in the backup hour — a restore
    // pulls a whole snapshot back over the same link the backups just used.
    if local_hour == cfg_hour
        && homelab_core::ops::restoredrill::due(
            st.last_restore_drill,
            now,
            st.restore_drill_interval_s,
        )
    {
        plan.push(NightlyTask::RestoreDrill);
    }
    plan
}

/// G14: restore one repository into a scratch directory and say what came
/// back. The judgement lives in core (`restoredrill::verdict`); this is the
/// shell that fetches the numbers and always cleans up after itself.
async fn run_restore_drill(
    exec: &RealExecutor,
    cfg: &homelab_core::ops::backup::BackupCfg,
    repo: &str,
    target: &str,
) -> homelab_core::ops::restoredrill::Outcome {
    use homelab_core::ops::restoredrill::{verdict, Outcome};
    let _ = exec.run(&Cmd::new("rm", &["-rf", target], 120)).await;
    let restored = homelab_core::ops::backup::restore_into(exec, cfg, repo, target).await;
    let outcome = match restored {
        Err(e) => Outcome::Failed(format!("the restore itself failed: {}", e)),
        Ok(()) => {
            let count = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", &format!("find {} -type f | wc -l", target)],
                    120,
                ))
                .await
                .map(|o| o.stdout.trim().parse::<usize>().unwrap_or(0))
                .unwrap_or(0);
            let largest = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "find {} -type f -printf '%s\\n' 2>/dev/null | sort -n | tail -1",
                            target
                        ),
                    ],
                    120,
                ))
                .await
                .map(|o| o.stdout.trim().parse::<u64>().unwrap_or(0))
                .unwrap_or(0);
            // fix-62: a native unit's backup is one tar; a torn one has
            // content and passed the size rule, so each must also list.
            let unreadable: Vec<String> = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "find {} -type f -name '*.tar' | while read -r f; do \
                             tar -tf \"$f\" >/dev/null 2>&1 || echo \"$f\"; done",
                            target
                        ),
                    ],
                    600,
                ))
                .await
                .map(|o| {
                    o.stdout
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            homelab_core::ops::restoredrill::with_archives(verdict(count, largest), &unreadable)
        }
    };
    // Always: a drill that leaves a full restore behind fills the disk the
    // backups need.
    let _ = exec.run(&Cmd::new("rm", &["-rf", target], 300)).await;
    outcome
}

/// H12: bearer check, extracted for testing.
/// F259: a watcher pointed at a path no writer writes to watches nothing.
///
/// `watched_backups` checks that files keep arriving somewhere;
/// `device_backups` is what puts them there, at
/// `<restic_base>/<name>-config`. On 2026-09-03 the two disagreed — the
/// watcher looked at `gdrive:homelab-backups/OPNSense-backups` while the
/// writer targeted `.../opnsense-config` — so the watcher would have gone on
/// reporting "holds no files at all" even after the backup started working,
/// and whoever read it would have concluded the fix had failed.
///
/// Returns the watchers whose path matches no configured writer. Deliberately
/// a warning rather than a refusal: a watcher may legitimately point at
/// something written by a machine this suite does not manage, and refusing to
/// start over that would be worse than saying so.
fn orphan_watchers(
    watched: &[WatchedBackup],
    devices: &[homelab_core::ops::devicebackup::DeviceBackup],
    restic_base: &str,
) -> Vec<String> {
    // Two shapes have to line up before this can compare anything, and both
    // were got wrong on the first live configuration:
    //
    //  * `restic_base` carries an `rclone:` prefix (`rclone:gdrive:...`)
    //    because that is how restic names a remote; an rclone path does not.
    //  * a watcher usefully points at `<repo>/snapshots` rather than at the
    //    repo root, because the root holds `config`, written once at init and
    //    never again — so watching the root reports the creation date forever
    //    and goes stale while the backups are running fine.
    //
    // So: normalise the prefix, and accept the repo path or anything under
    // it. A guard that cries wolf on a correct configuration is worse than no
    // guard, because it teaches its reader to skip the line.
    let norm = |p: &str| {
        p.trim_start_matches("rclone:")
            .trim_end_matches('/')
            .to_string()
    };
    let written: Vec<String> = devices
        .iter()
        .map(|d| norm(&format!("{}/{}-config", restic_base, d.name)))
        .collect();
    watched
        .iter()
        .filter(|w| {
            let path = norm(&w.rclone_path);
            !written
                .iter()
                .any(|repo| path == *repo || path.starts_with(&format!("{}/", repo)))
        })
        .map(|w| format!("{} → {}", w.name, w.rclone_path))
        .collect()
}

/// fix-120 (api-token-is-root, 2026-09-27): compared as SHA-256 digests with
/// every byte folded in, so neither the length nor the first differing byte
/// shows in the time the answer takes. It was a plain `==` on a formatted
/// string, which stops at the first difference.
fn bearer_ok(header: Option<&str>, token: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Some(given) = header else {
        return false;
    };
    let want = Sha256::digest(format!("Bearer {}", token).as_bytes());
    let got = Sha256::digest(given.as_bytes());
    want.iter()
        .zip(got.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// fix-120: say that a connection was refused for its token, and from where.
fn log_refused(state: &AppState, peer: SocketAddr, path: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let n = state.auth_failures.record(&peer.to_string(), now);
    tracing::warn!(
        "401 on {} from {}: missing or wrong bearer token ({} refused since this daemon started)",
        path,
        peer,
        n
    );
}

/// E4: check every 20 minutes; when the local hour matches `hour` and a
/// stack's last backup is >20h old, run backup (E1) then auto-updates (D9)
/// for that stack. Uses the same op machinery as RPCs (op-lock, incidents,
/// notifications), so a client-triggered deploy never overlaps.
/// Y1: the nightly backups, several at a time.
///
/// Kenny asked why so much of a run is spent waiting, and measuring answered
/// it: on 2026-09-02 a full round took about 38 minutes for thirteen stacks,
/// of which only ~6 minutes was actually writing data. The other half hour
/// was 36 small questions to Google Drive — does this repository exist, which
/// snapshots are there, forget the old ones — each one waiting on a
/// round-trip, not on bandwidth. That kind of waiting overlaps almost
/// perfectly, which is why this helps far more than a bandwidth argument
/// would suggest.
///
/// The global lock is taken ONCE for the whole round rather than dropped:
/// backups still cannot interleave with a deploy or a destroy, exactly as
/// before. What changed is only that they can interleave with EACH OTHER.
/// That is the conservative half of the win, and it is the half that is
/// obviously safe.
///
/// `limit` bounds how many run at once. Not a constant: a backup pauses its
/// containers to take a clean snapshot, so the number is also "how much of
/// the house may be briefly still at 04:00" — which is Kenny's call, not the
/// author's (he chose three).
use homelab_core::ops::backup::NightBackup;

async fn run_backup_batch(
    state: &AppState,
    exec: &RealExecutor,
    jobs: Vec<BackupJob>,
    limit: usize,
) -> std::collections::HashMap<String, NightBackup> {
    use futures_util::stream::StreamExt;
    if jobs.is_empty() {
        return std::collections::HashMap::new();
    }
    let limit = effective_concurrency(limit);
    info!(
        "scheduler: backing up {} stack(s), {} at a time",
        jobs.len(),
        limit
    );
    let _guard = state.op_lock.lock().await;
    // M-T75: measured, not predicted — the line the morning reads.
    let phase_started = std::time::Instant::now();
    let stacks_in_phase = jobs.len();
    let results: Vec<(String, NightBackup)> = futures_util::stream::iter(jobs)
        .map(|job| {
            // fix-122: the three backups interleave; their lines say whose.
            use tracing::Instrument as _;
            let span = stack_span(&job.stack);
            async move {
                let name = job.stack.clone();
                let outcome = match job.what {
                    BackupWhat::Compose(manifest) => {
                        let cfg = job.cfg.clone();
                        let r = run_op_locked(state, exec, 0, "scheduled-backup", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await
                            })
                        })
                        .await;
                        NightBackup::of(r.ok, r.deferred.as_deref())
                    }
                    // T5: several services share one container, so all of them are
                    // backed up and one failure fails the night for the stack —
                    // they share a container and a fate. Sequential WITHIN a
                    // stack: they are on the same container, so overlapping their
                    // pauses would stop that container twice over.
                    BackupWhat::Native(services) => {
                        let mut worst = NightBackup::Done;
                        for native in services {
                            let cfg = job.cfg.clone();
                            let r =
                                run_op_locked(state, exec, 0, "scheduled-backup-native", |ctx| {
                                    Box::pin(async move {
                                        homelab_core::ops::native::backup_native(ctx, &native, &cfg)
                                            .await
                                    })
                                })
                                .await;
                            worst = worst.worse_of(NightBackup::of(r.ok, r.deferred.as_deref()));
                        }
                        worst
                    }
                };
                if let NightBackup::Deferred(why) = &outcome {
                    info!("scheduler: backup for {} stood aside — {}", name, why);
                }
                (name, outcome)
            }
            .instrument(span)
        })
        .buffer_unordered(limit)
        .collect()
        .await;
    info!(
        "{}",
        homelab_core::ops::backup::phase_duration_line(
            phase_started.elapsed().as_secs(),
            stacks_in_phase,
            limit
        )
    );
    results.into_iter().collect()
}

/// One stack's share of the nightly backup phase.
struct BackupJob {
    stack: String,
    what: BackupWhat,
    cfg: homelab_core::ops::backup::BackupCfg,
}

enum BackupWhat {
    Compose(Box<homelab_proto::StackManifest>),
    Native(Vec<homelab_core::native::NativeServiceManifest>),
}

async fn scheduler_loop(state: AppState) {
    // fix-122: the nightly updates run inside a span naming their stack.
    use tracing::Instrument as _;
    let exec = RealExecutor;
    loop {
        tokio::time::sleep(Duration::from_secs(20 * 60)).await;
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
        let (hour, tiers) = {
            let s = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner);
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
        // G14: taken before the loop below consumes `snapshot.stacks`.
        let drill_repos: Vec<String> = homelab_core::ops::restoredrill::all_drill_repos(
            &snapshot
                .stacks
                .iter()
                .map(|(name, st)| {
                    (
                        st.manifest
                            .as_ref()
                            .map(|m| m.storage.clone())
                            .unwrap_or_default(),
                        name.clone(),
                        st.natives
                            .iter()
                            .map(|n| n.unit.clone())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
        );
        // fix-62: whose turn it is, read before the loop below consumes the
        // snapshot's stacks.
        let drill_pick = homelab_core::ops::restoredrill::pick(&snapshot, &drill_repos);
        let plan = nightly_plan(
            hour,
            local_hour,
            now,
            &stack_inputs,
            &NightlyState {
                last_host_meta: snapshot.last_host_meta,
                last_zfs: snapshot.last_zfs,
                last_restore_drill: snapshot.last_restore_drill,
                restore_drill_interval_s: state.config.restore_drill_interval_s,
                zfs_configured: !state.config.zfs_jobs.is_empty(),
                devices_configured: !state.config.device_backups.is_empty(),
            },
        );
        // Y1: every due backup runs first, several at a time, under one
        // hold of the global lock. Then the loop below does the updates one
        // by one — an update replaces running containers, which is a very
        // different risk from reading their data, so it stays serial.
        //
        // Backups before updates also orders the night sensibly on its own:
        // everything is safe on disk before anything is replaced.
        let backup_jobs: Vec<BackupJob> = snapshot
            .stacks
            .iter()
            .filter(|(name, _)| plan.contains(&NightlyTask::Stack((*name).clone())))
            .filter_map(|(name, st)| {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let what = if st.is_native() {
                    BackupWhat::Native(st.natives.clone())
                } else {
                    BackupWhat::Compose(Box::new(st.manifest.clone()?))
                };
                Some(BackupJob {
                    stack: name.clone(),
                    what,
                    cfg,
                })
            })
            .collect();
        let backup_done =
            run_backup_batch(&state, &exec, backup_jobs, state.config.backup_concurrency).await;

        let updates_parked = snapshot.updates_parked.clone();
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
                // Y1: the backup already ran in the batch above.
                let backup = backup_done
                    .get(&name)
                    .cloned()
                    .unwrap_or(NightBackup::Failed);
                let mut update_ok = true;
                let applied = Some(st.applied_at);
                // fix-59: a stack whose updates a failed night parked is
                // still backed up above, and not updated here.
                let natives: &[homelab_core::native::NativeServiceManifest] =
                    if updates_parked.contains_key(&name) {
                        info!(
                            "scheduler: automatic updates of {} are parked — skipped; \
                             `homelab enable {}` resumes them",
                            name, name
                        );
                        &[]
                    } else if !backup.allows_update() {
                        info!("{}", backup.update_skip_line(&name));
                        &[]
                    } else {
                        &st.natives
                    };
                // B1: the orchestrator's own release update, for the
                // services whose policy hands it to the orchestrator.
                for native in natives.iter().filter(|n| n.nightly_updates().release) {
                    let native = native.clone();
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-release-update", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::release_update(ctx, &native).await
                        })
                    })
                    .instrument(stack_span(&name))
                    .await;
                    update_ok &= r.ok;
                }
                for native in natives.iter().filter(|n| n.nightly_updates().own_cmd) {
                    let n2 = native.clone();
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-update-native", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::update_native(ctx, &n2, applied).await
                        })
                    })
                    .instrument(stack_span(&name))
                    .await;
                    update_ok &= r.ok;
                }
                if backup.records_a_timestamp() {
                    record_state(&store, "last_backup", |s| {
                        if let Some(rec) = s.stacks.get_mut(&name) {
                            rec.last_backup = now;
                        }
                    })
                    .await;
                }
                // fix-59: only a failed update parks, and only the updates; a
                // failed or deferred backup is simply tried again tomorrow.
                park_after_night(&state, &exec, &store, &name, update_ok, now).await;
                continue;
            }
            let Some(manifest) = st.manifest else {
                tracing::warn!("scheduler: stack {} has no stored manifest — skipped", name);
                continue;
            };
            info!("scheduler: nightly run for {}", name);
            // Y1: the backup already ran in the batch above.
            let backup = backup_done
                .get(&name)
                .cloned()
                .unwrap_or(NightBackup::Failed);
            if backup.records_a_timestamp() {
                // Record last_backup so tomorrow's check is accurate.
                record_state(&store, "last_backup", |s| {
                    if let Some(rec) = s.stacks.get_mut(&name) {
                        rec.last_backup = now;
                    }
                })
                .await;
            }
            // fix-59: parked updates are skipped; the backup above still ran.
            if updates_parked.contains_key(&name) {
                info!(
                    "scheduler: automatic updates of {} are parked — skipped; \
                     `homelab enable {}` resumes them",
                    name, name
                );
                continue;
            }
            if !backup.allows_update() {
                info!("{}", backup.update_skip_line(&name));
                continue;
            }
            let m2 = manifest.clone();
            let update_report = run_mutating_op(&state, &exec, 0, "scheduled-update", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::update::update(ctx, &m2, None, true).await },
                )
            })
            .instrument(stack_span(&name))
            .await;
            // H8: a failed nightly update parks the stack's updates — one
            // loud message, then silence instead of a fresh failure every
            // night. State-only: onboot and the running containers are
            // untouched, and since fix-59 the nightly backup goes on.
            park_after_night(&state, &exec, &store, &name, update_report.ok, now).await;
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
                record_state(&store, "last_host_meta", |s| s.last_host_meta = now).await;
            } else {
                tracing::error!(
                    "scheduler: host-meta backup FAILED — the vault/state/TLS snapshot is the recovery path for a lost host disk; investigate now"
                );
            }
        }

        // G14: one restore, rehearsed for real, in turn.
        //
        // Restores into a temporary directory and judges what came back by
        // its LARGEST file rather than its first. That is not fussiness: on
        // 2026-09-02 a hand-run drill declared a restore identical to live by
        // comparing two md5 sums that both belonged to a zero-byte file, and
        // a drill that can be satisfied by empty files rehearses nothing.
        if plan.contains(&NightlyTask::RestoreDrill) {
            if let Some(repo) = drill_pick.clone() {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let target = format!("{}/restore-drill", state.config.state_dir);
                let outcome = run_restore_drill(&exec, &cfg, &repo, &target).await;
                match &outcome {
                    homelab_core::ops::restoredrill::Outcome::Passed {
                        files,
                        largest_bytes,
                    } => info!(
                        "restore drill: {} came back with {} file(s), largest {} bytes",
                        repo, files, largest_bytes
                    ),
                    homelab_core::ops::restoredrill::Outcome::Failed(why) => {
                        tracing::error!("restore drill: {} proved nothing :: {}", repo, why)
                    }
                }
                record_state(&store, "restore drill", |sn| {
                    homelab_core::ops::restoredrill::record(sn, &drill_repos, &repo, &outcome, now)
                })
                .await;
            }
        }

        // Route A: the devices this suite may not touch, asked for their own
        // configuration. Best-effort per device — one router refusing does
        // not fail the night for the others, and the failure is a finding
        // rather than a silence.
        if plan.contains(&NightlyTask::DeviceConfig) {
            for dev in state.config.device_backups.clone() {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let name = dev.name.clone();
                let report = run_mutating_op(&state, &exec, 0, "device-backup", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
                    })
                })
                .await;
                if !report.ok {
                    tracing::error!(
                        "scheduler: device backup for {} FAILED — its configuration is not \
                         being kept",
                        name
                    );
                }
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
                record_state(&store, "last_zfs", |s| s.last_zfs = now).await;
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
        // G15 of the Phase-7 gate: this used to run only when the night had
        // work to do. Backwards — the findings this check exists for are
        // precisely the ones that produce no failure of their own, so a night
        // where nothing was due is a night where nothing was watched either.
        // A hostname that drifted, a route that leads nowhere and a template
        // that no longer exists do not wait for a backup to be due.
        {
            // The stack-file half is deliberately empty here and it is not an
            // oversight: those files live in the CLIENT's repository, not on
            // the host, so "does this file target a vmid somebody else owns"
            // is a question `homelab check` asks from the workstation. The
            // host answers everything it can actually see.
            let live = gather_live_facts(&exec, &state, &[]).await;
            if let Ok(snapshot) = store.load().await {
                let findings = homelab_core::ops::fleetcheck::evaluate(
                    &snapshot,
                    &live,
                    now,
                    homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
                    homelab_core::ops::fleetcheck::GrowthLimits::default(),
                );
                // Z3, now a tested function in core rather than a filter
                // buried in this loop (G15).
                let problems = homelab_core::ops::fleetcheck::alarming(&findings);
                // fix-65: the same alarming set is sent once, then weekly
                // while it stands; a new or changed set goes out that night.
                let fingerprint = homelab_core::ops::fleetcheck::report_fingerprint(&findings);
                let send = !problems.is_empty()
                    && homelab_core::ops::fleetcheck::nightly_report_due(
                        &fingerprint,
                        &snapshot.last_fleet_report_fp,
                        snapshot.last_fleet_report_at,
                        now,
                    );
                if send || (problems.is_empty() && !snapshot.last_fleet_report_fp.is_empty()) {
                    // Remember what went out; forget it once nothing is
                    // alarming, so a problem that comes back is sent again.
                    let fp = fingerprint.clone();
                    record_state(&store, "fleet report fingerprint", |sn| {
                        if send {
                            sn.last_fleet_report_fp = fp;
                            sn.last_fleet_report_at = now;
                        } else {
                            sn.last_fleet_report_fp.clear();
                        }
                    })
                    .await;
                }
                if problems.is_empty() {
                    info!(
                        "fleet check: repo and reality agree{}",
                        if findings.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", render_findings(&findings))
                        }
                    );
                } else if !send {
                    tracing::warn!(
                        "fleet check: {} finding(s), the same alarming set as reported on {} — \
                         not sent again until it changes or a week has passed\n{}",
                        findings.len(),
                        homelab_core::state::ymd(snapshot.last_fleet_report_at),
                        render_findings(&findings)
                    );
                } else {
                    tracing::warn!(
                        "fleet check: {} finding(s)\n{}",
                        findings.len(),
                        render_findings(&findings)
                    );
                    // The finding text is already in the log above; the
                    // webhook exists so it leaves the machine.
                    // F86: through op_payload like every other event, so the
                    // report of the day carries `source` and `label` too. It
                    // used to hand-build its own JSON without either, which
                    // made it the one event a filter on `source` would drop.
                    notify_raw(
                        &state,
                        &exec,
                        homelab_core::notify::op_payload(
                            "fleet-check",
                            "nightly",
                            false,
                            Some(&render_findings(&findings)),
                            VERSION,
                        ),
                    )
                    .await;
                }
            }
        }
    }
}

/// The largest single message the link carries, in bytes. Shared with the
/// client so a payload that cannot arrive is refused before it is built.
pub const MAX_WS_FRAME: usize = 256 * 1024 * 1024;

// Raised from 64 MiB on 2026-09-09 (F303). The old figure was "five times the
// current binary" — reasoning about the HOST binary, which was the only large
// payload the link had at the time. A stack deploy carries every native
// service's program in one message, so the ceiling has to scale with the
// STACK, not with one program: CT 109 alone ships kyu, kyu-runner and
// http-switchboard, 71 MiB of binaries and 94.7 MiB once base64-encoded.
//
// Headroom, not a solution. The honest fix is to send those programs one at a
// time instead of in one message — recorded as T85. This number only buys the
// room to get there without a wall in the middle.

async fn ws_upgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authed = bearer_ok(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
    );
    if !authed {
        log_refused(&state, peer, "/api/ws");
        return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
    }
    // H5 self-update ships the whole host binary in one message, and the
    // default frame ceiling is 16 MiB. The binary crossed it between v3.19.0
    // (16 729 464 bytes of base64) and v3.20.0 (16 861 920) — 132 KB over —
    // and the failure said "Connection reset by peer", which points at the
    // network rather than at a limit nobody had ever named. The rollout had
    // to be done by hand to get a host that could accept the next one.
    //
    // 64 MiB is not a considered capacity figure, it is distance: five times
    // the current binary, so the ceiling is not reachable by growth alone.
    // The client refuses to send more than this and says so in words.
    ws.max_frame_size(MAX_WS_FRAME)
        .max_message_size(MAX_WS_FRAME)
        .on_upgrade(move |socket| ws_session(socket, state))
        .into_response()
}

/// The journal line for a request frame this end could not parse.
///
/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): never
/// the body. A deploy frame carries every `.env` value of its stack and a
/// staging frame tens of MB of base64; both went into one journal line when
/// a version skew made a frame unreadable. serde's own message is left out
/// too, because it quotes the offending value ("invalid type: string
/// \"...\""), which can be a secret. The method, the size and where parsing
/// stopped are enough to tell which client sent what.
fn unparseable_frame_line(e: &serde_json::Error, text: &str) -> String {
    let method = text
        .find("\"cmd\"")
        .map(|at| {
            text[at + 5..]
                .trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '"')
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .take(40)
                .collect::<String>()
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "?".into());
    format!(
        "unparseable request dropped :: cmd={} · {} bytes · {:?} error at line {} column {}",
        method,
        text.len(),
        e.classify(),
        e.line(),
        e.column()
    )
}

async fn ws_session(socket: WebSocket, state: AppState) {
    serve_ws(socket, state, |st, req| async move {
        handle_rpc(&st, req).await
    })
    .await
}

/// One client's session: the Hello, the forwarder that carries broadcasts and
/// answers out, and the loop that reads requests. The request handler is a
/// parameter so a test can drive the real session over a real socket with a
/// handler that asks a question (fix-66).
async fn serve_ws<H, Fut>(socket: WebSocket, state: AppState, handler: H)
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let (mut tx, mut rx) = socket.split();
    let hello = ServerMsg::Hello {
        version: VERSION.into(),
        proto: homelab_proto::PROTO_VERSION,
    };
    let _ = tx
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await;

    let mut log_rx = state.log_tx.subscribe();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let asks = state.pending_asks.clone();
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                received = log_rx.recv() => {
                    let msgs = match received {
                        Ok(msg) => vec![msg],
                        // fix-127 (websocket-edge-cases, 2026-09-27): a client
                        // slower than the host lags the channel. The skipped
                        // messages were dropped in silence, questions
                        // included, so an operation waited on an answer to a
                        // question nobody saw. Say how much was missed, and
                        // send every open question again.
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("a client lagged: {} message(s) to it dropped", n);
                            lag_catch_up(n, &asks)
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    };
                    let mut gone = false;
                    for msg in msgs {
                        if tx.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() {
                            gone = true;
                            break;
                        }
                    }
                    if gone { break; }
                }
                Some(msg) = out_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() { break; }
                }
                else => break,
            }
        }
    });

    // fix-66 (host-questions-unanswerable, 2026-09-27): requests are READ
    // continuously and RUN by a worker, one at a time and in arrival order.
    // Before this the loop ran each request inline, so the next frame was not
    // read until the current request returned. The TUI sends its answer to a
    // question over the same connection as the deploy that asked it; that
    // answer sat unread behind the deploy waiting for it, and every question
    // ended as Unattended whatever the operator pressed.
    //
    // Everything except an answer keeps its order: the TUI tells replies
    // apart by the order it sent the requests in, not by id.
    let (work_tx, mut work_rx) = tokio::sync::mpsc::unbounded_channel::<RpcRequest>();
    let worker = {
        let out_tx = out_tx.clone();
        let state = state.clone();
        let handler = handler.clone();
        tokio::spawn(async move {
            use tracing::Instrument as _;
            while let Some(req) = work_rx.recv().await {
                let span = rpc_span(&req);
                let resp = handler(state.clone(), req).instrument(span).await;
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                // fix-121: an answered request is what accepts an update.
                accept_pending_update(&state.config.state_dir, state.started_at);
            }
        })
    };

    while let Some(frame) = rx.next().await {
        // fix-127 (websocket-edge-cases, 2026-09-27): only Close and a read
        // error end the session. The loop matched Text alone, so the first
        // Ping (a client or proxy keepalive), Pong or Binary frame ended it
        // without a word. Pings are answered by the WebSocket layer itself.
        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Binary(b)) => {
                tracing::warn!(
                    "a binary frame of {} bytes was ignored — requests are JSON text",
                    b.len()
                );
                continue;
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                info!("session ended on a read error :: {}", e);
                break;
            }
        };
        let req = match serde_json::from_str::<RpcRequest>(&text) {
            Ok(r) => r,
            Err(e) => {
                // Silence here is why `homelab checks answer` looked like a
                // hang rather than a bug: the request was dropped and the
                // client waited for a reply that was never coming. A frame
                // this end cannot understand is a fault on this end.
                tracing::error!("{}", unparseable_frame_line(&e, &text));
                continue;
            }
        };
        if runs_beside_the_queue(&req.command) {
            let (out_tx, state, handler) = (out_tx.clone(), state.clone(), handler.clone());
            tokio::spawn(async move {
                use tracing::Instrument as _;
                let span = rpc_span(&req);
                let resp = handler(state.clone(), req).instrument(span).await;
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                accept_pending_update(&state.config.state_dir, state.started_at);
            });
        } else if work_tx.send(req).is_err() {
            break;
        }
    }
    // The client went away. What it already asked for still runs to the end,
    // as it did when the loop ran requests inline: a deploy is not abandoned
    // halfway because a laptop lid closed.
    drop(work_tx);
    let _ = worker.await;
    forward.abort();
}

/// fix-127: what a client that lagged past `n` messages is sent instead: a
/// warning that says so, then every question still waiting for an answer,
/// oldest first.
fn lag_catch_up(
    n: u64,
    asks: &std::sync::Mutex<std::collections::HashMap<u64, PendingAsk>>,
) -> Vec<ServerMsg> {
    let mut out = vec![ServerMsg::Log {
        level: homelab_proto::LogLevel::Warn,
        source: "HOST".into(),
        msg: format!(
            "{} message(s) to this client were dropped because it read too slowly; \
             open questions are sent again",
            n
        ),
    }];
    let guard = asks.lock().unwrap_or_else(PoisonError::into_inner);
    let mut open: Vec<(&u64, &PendingAsk)> = guard.iter().collect();
    open.sort_by_key(|(id, _)| **id);
    out.extend(open.into_iter().map(|(_, p)| p.ask.clone()));
    out
}

/// fix-66: requests that must not wait behind the one in flight. An answer
/// only hands a value to an operation that is parked waiting for it; queued
/// behind that same operation it can never arrive.
fn runs_beside_the_queue(command: &Rpc) -> bool {
    // fix-68: `today` reads for about a minute (doctor plus the fleet check);
    // in the queue it would hold up every deploy and refresh the TUI sends
    // after opening. It changes nothing, and the TUI recognises its reply by
    // shape rather than by order.
    matches!(command, Rpc::Answer { .. } | Rpc::Today { .. })
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

/// H8: record what one stack's night parked, and say so once. The decision
/// is `ops::enable::after_night`; this is the load, save and notice around it.
async fn park_after_night(
    state: &AppState,
    exec: &RealExecutor,
    store: &homelab_core::state::StateStore<'_>,
    name: &str,
    update_ok: bool,
    now: u64,
) {
    let parked = record_state(store, "auto-park", |s| {
        homelab_core::ops::enable::after_night(s, name, update_ok, now)
    })
    .await
    .unwrap_or(false);
    if parked {
        tracing::warn!(
            "scheduler: nightly update for {} FAILED — automatic updates parked, backups continue (H8, fix-59); investigate, then resume with `homelab enable {}`",
            name, name
        );
    }
    if parked {
        notify_auto_disabled(
            state,
            exec,
            name,
            homelab_core::ops::enable::AUTO_PARK_NOTICE,
        )
        .await;
    }
}

/// A stack has just been parked by H8, which is the moment it stops being
/// protected: since fix-59 no automatic update (its nightly backup goes on;
/// onboot is left alone by the automatic park, gap-22).
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
        .unwrap_or_else(PoisonError::into_inner)
        .should_send(&op, false, Some(why), now)
    {
        return;
    }
    let payload = homelab_core::notify::op_payload(&op, stack, false, Some(why), VERSION);
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
        .unwrap_or_else(PoisonError::into_inner)
        .should_send(&report.op, report.ok, error.as_deref(), now)
    {
        return;
    }
    let payload =
        homelab_core::notify::op_payload(&report.op, label, report.ok, error.as_deref(), VERSION);
    notify_raw(state, exec, payload).await;
}

/// fix-36: remote exec checks the no-touch list the daemon actually runs
/// with. It used `SafetyConfig::default()`, so a vmid host.toml added to the
/// list (F8 lets config widen it) was still reachable through `homelab exec`.
/// Append one line to audit.log.
///
/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27): a new
/// file is created 0600. It was created with the default mode, 0644, and it
/// records every exec command.
fn append_audit(path: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?
        .write_all(line.as_bytes())
}

/// fix-125: at start, take group and world access off the daemon's private
/// records that already exist: audit.log, journal.jsonl and the incident
/// bundles (0700 directories, 0600 files, the replay scripts 0700). Before
/// fix-125 all of them were created readable by every account on pve, and a
/// new mode for new files leaves the old ones as they were. Best effort: a
/// path that cannot be changed is said once and skipped.
fn tighten_private_paths(state_dir: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let set = |p: &std::path::Path, mode: u32| {
        let Ok(meta) = std::fs::symlink_metadata(p) else {
            return;
        };
        if meta.file_type().is_symlink() || meta.permissions().mode() & 0o777 == mode {
            return;
        }
        if let Err(e) = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)) {
            tracing::warn!("could not make {} private :: {}", p.display(), e);
        }
    };
    let base = std::path::Path::new(state_dir);
    set(&base.join("audit.log"), 0o600);
    set(&base.join("journal.jsonl"), 0o600);
    let incidents = base.join("incidents");
    set(&incidents, 0o700);
    for bundle in std::fs::read_dir(&incidents)
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = bundle.path();
        if !path.is_dir() {
            set(&path, 0o600);
            continue;
        }
        set(&path, 0o700);
        for file in std::fs::read_dir(&path).into_iter().flatten().flatten() {
            let f = file.path();
            let is_script = f.extension().is_some_and(|e| e == "sh");
            set(&f, if is_script { 0o700 } else { 0o600 });
        }
    }
}

/// The audit.log line for one remote exec, which the journal repeats.
///
/// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): the command
/// passes the shared secret masker first. It was recorded verbatim, so a
/// password typed on an exec command line stayed on pve for good. The
/// masker knows shapes (`NAME=value` for secret-looking names, URL
/// passwords, `Bearer <token>`), not every secret: a bare password argument
/// still lands, which the user guide says.
fn exec_audit_line(ts: u64, vmid: u16, command: &str) -> String {
    format!(
        "{} exec vmid={} cmd={:?}\n",
        ts,
        vmid,
        homelab_core::executor::mask_secrets(command)
    )
}

fn exec_allowed(config: &Config, vmid: u16) -> Result<(), homelab_core::error::CoreError> {
    homelab_core::safety::exec_guard(config.exec_enabled, &config.safety, vmid)
}

/// Lower-level webhook POST used by notify() and the boot notification.
///
/// G16: this used to be `let _ = exec.run(...)` — a fire-and-forget curl with
/// the body discarded, so the one path by which Kenny learns that anything is
/// wrong could itself be broken with nothing anywhere saying so. It now reads
/// the status, falls back to the second route when the first fails, and
/// records the outcome in state so an unreachable notification path becomes a
/// finding instead of a silence.
async fn notify_raw(state: &AppState, exec: &RealExecutor, payload: String) {
    let primary = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .notify_webhook
        .clone();
    let fallback = state.config.notify_fallback_webhook.clone();
    let urls = homelab_core::notify::route(primary.as_deref(), fallback.as_deref());
    if urls.is_empty() {
        return;
    }
    let mut last = String::new();
    let mut delivered = false;
    for (i, url) in urls.iter().enumerate() {
        // Each route carries its own credential: kyu takes a bearer token,
        // Home Assistant's webhook takes none.
        let bearer = if i == 0 {
            state.config.notify_auth_bearer.clone()
        } else {
            state.config.notify_fallback_auth_bearer.clone()
        };
        // fix-35: the token goes to curl through a 0600 header file, never
        // through argv (see homelab_core::notify::curl_args).
        let header_file = match bearer {
            Some(t) => {
                let path = homelab_core::notify::header_file_path(&state.config.state_dir, i);
                match exec
                    .write_file(&path, &homelab_core::notify::header_file_content(&t), 0o600)
                    .await
                {
                    Ok(()) => Some(path),
                    Err(e) => {
                        last = format!("cannot write the header file {}: {}", path, e);
                        continue;
                    }
                }
            }
            None => None,
        };
        let owned = homelab_core::notify::curl_args(&payload, url, header_file.as_deref());
        let args: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let out = exec.run(&Cmd::new("curl", &args, 10)).await;
        let (ran, code) = match &out {
            Ok(o) => (true, o.stdout.clone()),
            Err(_) => (false, String::new()),
        };
        match homelab_core::notify::verdict(ran, &code) {
            homelab_core::notify::Delivery::Delivered => {
                if i > 0 {
                    tracing::warn!(
                        "notification took the fallback route: the primary said {}",
                        last
                    );
                }
                delivered = true;
                break;
            }
            homelab_core::notify::Delivery::Failed(why) => {
                last = why;
                // fix-123: the route's host only; a webhook path is its id.
                tracing::warn!(
                    "notification route {} failed: {}",
                    homelab_core::notify::route_for_log(url),
                    last
                );
            }
        }
    }
    record_notify_outcome(state, exec, delivered, &last).await;
}

/// fix-51 (expert panel, state-writes-race, 2026-09-27): every short
/// read-modify-write of state.json outside an operation goes through the
/// store's lock, and a failure is logged instead of dropped. These used to be
/// `load` then `let _ = store.save(..)`: concurrent callers saved over each
/// other and nobody heard about a save that failed.
async fn record_state<R>(
    store: &homelab_core::state::StateStore<'_>,
    what: &str,
    change: impl FnOnce(&mut homelab_core::state::HostState) -> R,
) -> Option<R> {
    match store.update(change).await {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!("state: could not record {} :: {}", what, e);
            None
        }
    }
}

/// Keep the last word on whether notifications are arriving, so a broken
/// notification path is visible somewhere other than in a notification.
///
/// That circularity is real and worth stating: if every route is down, this
/// record is what `homelab check` and the TUI read, because the report saying
/// so cannot reach him by the path that is broken.
async fn record_notify_outcome(state: &AppState, exec: &RealExecutor, delivered: bool, why: &str) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    record_state(&store, "notification outcome", |st| {
        if delivered {
            st.last_notify_ok = now;
            st.last_notify_error = None;
        } else {
            st.last_notify_failed = now;
            st.last_notify_error = Some(why.to_string());
        }
    })
    .await;
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
    run_op_locked(state, exec, req_id, label, op).await
}

/// The body of a mutating operation WITHOUT taking the global lock.
///
/// Y1: the nightly backup phase holds that lock once for the whole round and
/// runs several backups inside it, so it needs the work without the locking.
/// Every RPC still goes through `run_mutating_op`, which is this plus the
/// lock — there is one implementation, not two that can drift.
async fn run_op_locked<F>(
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
    // fix-122 (journal-lines-lack-op-context, 2026-09-27): every journal
    // line of the operation, its steps and its failure carries `op=<label>`;
    // the caller's span adds the stack.
    use tracing::Instrument as _;
    run_op_body(state, exec, req_id, label, op)
        .instrument(tracing::info_span!("op", op = %label))
        .await
}

async fn run_op_body<F>(
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
    let asker = LiveAsker {
        state,
        timeout_s: state.config.ask_timeout_s,
    };
    let ctx = OpCtx {
        exec,
        sink: &sink,
        journal: &journal,
        safety: state.config.safety.clone(),
        state_dir: state.config.state_dir.clone(),
        now_unix: now,
        metrics_targets_dir: state.config.metrics_targets_dir.clone(),
        grafana_dashboards_dir: state.config.grafana_dashboards_dir.clone(),
        homepage_services_file: state.config.homepage_services_file.clone(),
        kuma_monitors_file: state.config.kuma_monitors_file.clone(),
        // C1/C2: the same Loki the coverage check already asks about, so
        // there is not a second address to keep in step with the first.
        loki_url: state.config.loki_url.clone(),
        backup: state.config.backup.clone(),
        registry_cache: state.config.registry_cache.clone(),
        asker: &asker,
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
            deferred: None,
        }
    } else if let Some(why) = report.deferred.clone() {
        // Stood aside on purpose. No incident bundle, no `error!`, no
        // failure count: nothing broke and nothing changed. `ok` stays false
        // because nothing ran either, so no caller records work that did not
        // happen (F280).
        info!("{} stood aside: {}", label, why);
        RpcResponse {
            id: req_id,
            ok: false,
            message: format!("{} deferred — {}", label, why),
            deferred: Some(why),
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
            deferred: None,
        }
    }
}

/// C7: look up an adopted native stack's manifest in state. Error strings
/// carry the remedy, per standing rule 11.
async fn native_from_state(
    state_dir: &str,
    stack: &str,
) -> Result<(Vec<homelab_core::native::NativeServiceManifest>, u64), String> {
    let store = homelab_core::state::StateStore::new(&RealExecutor, state_dir);
    let snapshot = store
        .load()
        .await
        .map_err(|e| format!("state unreadable: {}", e))?;
    match snapshot.stacks.get(stack) {
        // The timestamp rides along so a skip can say how old the copy it
        // read is — the difference between a decision and a stale field.
        Some(st) if st.is_native() => Ok((st.natives.clone(), st.applied_at)),
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
///
/// G6 (T79): the gathering itself now lives in `homelab_core::ops::facts`,
/// behind an executor, with tests; this is the host's thin adapter that
/// hands it the configuration and logs what it measured.
/// fix-68: the three readings `homelab today` merges — doctor, the fleet
/// check with its manual checks, and the incident bundles — gathered the
/// same way their own verbs gather them.
async fn gather_today(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
) -> homelab_core::ops::today::Today {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut probes = gather_probes(
        exec,
        &state.config.state_dir,
        state.config.mirror_remote.as_deref(),
        now,
    )
    .await;
    probes.failed_auth = Some(state.auth_failures.snapshot());
    let checks = homelab_core::doctor::diagnose(&probes);
    let live = gather_live_facts(exec, state, stack_files).await;
    let incidents: Vec<String> = std::fs::read_dir(format!("{}/incidents", state.config.state_dir))
        .map(|rd| {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();
    match homelab_core::state::StateStore::new(exec, &state.config.state_dir)
        .load()
        .await
    {
        Ok(snapshot) => {
            let findings = homelab_core::ops::fleetcheck::evaluate(
                &snapshot,
                &live,
                now,
                homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
                homelab_core::ops::fleetcheck::GrowthLimits::default(),
            );
            homelab_core::ops::today::assemble(&checks, &findings, &incidents, &snapshot, now)
        }
        Err(e) => {
            let mut t =
                homelab_core::ops::today::assemble(&checks, &[], &[], &Default::default(), now);
            t.unread.push(format!(
                "state unreadable, so the fleet check and the incidents were not read: {}",
                e
            ));
            t
        }
    }
}

async fn gather_live_facts(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
) -> homelab_core::ops::fleetcheck::LiveFacts {
    use homelab_core::ops::facts::{FactsInputs, WatchedBackupSpec};
    let inp = FactsInputs {
        watched_backups: state
            .config
            .watched_backups
            .iter()
            .map(|w| WatchedBackupSpec {
                name: w.name.clone(),
                rclone_path: w.rclone_path.clone(),
                max_age_hours: w.max_age_hours,
            })
            .collect(),
        kuma_monitors_file: state.config.kuma_monitors_file.clone(),
        state_dir: state.config.state_dir.clone(),
        gateway_vmid: state.config.safety.gateway_vmid,
        gateway_routes_dir: state.config.safety.gateway_routes_dir.clone(),
        no_touch: state.config.safety.no_touch.to_vec(),
        prometheus_url: state.config.prometheus_url.clone(),
        loki_url: state.config.loki_url.clone(),
        logs_window: sane_window(&state.config.logs_window),
        grafana_dashboards_dir: state.config.grafana_dashboards_dir.clone(),
        now_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    let (facts, notes) = homelab_core::ops::facts::gather_live_facts(exec, &inp, stack_files).await;
    for n in notes {
        info!("{}", n);
    }
    facts
}

/// A backup is a backup, whoever asked for it. The scheduler recorded
/// `last_backup` and the on-demand paths did not, so a stack backed up by
/// hand still read as never backed up — which is exactly what the fleet check
/// reported about kyu minutes after I had backed it up myself.
async fn record_backup_time(state: &AppState, stack: &str) {
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    record_state(&store, "last_backup", |s| {
        if let Some(rec) = s.stacks.get_mut(stack) {
            rec.last_backup = now;
        }
    })
    .await;
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
                Severity::Noted => "noted",
            },
            f.subject,
            f.what,
            f.remedy
        ));
    }
    s
}

/// T85: where a stack's binaries wait between `StageNativeBinary` and the
/// `DeployStack` that installs them. Under the state directory (root-only,
/// in no backup — a staged binary is re-fetchable and short-lived).
fn staged_binaries_dir(state_dir: &str, stack: &str) -> String {
    format!("{}/staged/{}", state_dir, stack)
}

async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor;
    match req.command {
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
            deferred: None,
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
                deferred: None,
            }
        }
        // T85: a native binary arrives on its own, before the deploy that
        // installs it. Kept under the state directory, root-only, until the
        // deploy consumes it; a name that is not a plain stack or unit name
        // is refused before it can become a path.
        Rpc::StageNativeBinary {
            stack,
            unit,
            binary_b64,
        } => {
            let plain = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            };
            if !plain(&stack) || !plain(&unit) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "refusing to stage '{}' for '{}': stack and unit names are lowercase \
                         [a-z0-9-] and nothing else",
                        unit, stack
                    ),
                    deferred: None,
                };
            }
            let dir = staged_binaries_dir(&state.config.state_dir, &stack);
            let path = format!("{}/{}.b64", dir, unit);
            // fix-51: through the executor's atomic write (own temp name,
            // 0600 from creation, off the async worker) rather than a plain
            // `std::fs::write` of tens of MB that a deploy reading the file at
            // the same moment could see half-written.
            let written = exec.write_file(&path, &binary_b64, 0o600).await;
            match written {
                Ok(()) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "staged {} for {} ({} KiB of base64) — the next deploy of this stack \
                         installs it",
                        unit,
                        stack,
                        binary_b64.len() / 1024
                    ),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("could not stage {} for {}: {}", unit, stack, e),
                    deferred: None,
                },
            }
        }
        Rpc::DeployStack(mut spec) => {
            // T85: the binaries this deploy installs were staged one per
            // message; fill them in here, and clear the staging area after
            // the deploy whatever its outcome — a retry stages again.
            let dir = staged_binaries_dir(&state.config.state_dir, &spec.manifest.stack_name);
            let merged = homelab_core::ops::native::merge_staged_binaries(
                &spec.manifest.natives,
                &mut spec.native_binaries,
                |unit| std::fs::read_to_string(format!("{}/{}.b64", dir, unit)).ok(),
            );
            if !merged.is_empty() {
                info!(
                    "deploy {}: {} staged binar{} taken up ({})",
                    spec.manifest.stack_name,
                    merged.len(),
                    if merged.len() == 1 { "y" } else { "ies" },
                    merged.join(", ")
                );
            }
            let resp = run_mutating_op(state, &exec, req.id, "deploy", |ctx| {
                Box::pin(async move { deploy(ctx, &spec).await })
            })
            .await;
            let _ = std::fs::remove_dir_all(&dir);
            resp
        }
        Rpc::DestroyStack {
            manifest,
            confirm,
            skip_backup,
        } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy(ctx, &manifest, &confirm, skip_backup).await
                })
            })
            .await
        }
        Rpc::BackupStack(manifest) => {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: state
                    .settings
                    .read()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retention
                    .clone(),
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
        Rpc::RestoreStack {
            manifest,
            snapshot,
            confirm,
            skip_safety_copy,
        } => {
            // fix-64: no typed name, no restore — whoever sent the request.
            if let Err(e) = homelab_core::ops::backup::restore_confirmed(
                &manifest.stack_name,
                confirm.as_deref(),
            ) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e.to_string(),
                    deferred: None,
                };
            }
            // The configured target and timeout, not the compiled defaults:
            // this is the path where a hardcoded 1800 s used to kill a large
            // restore over Google Drive at thirty minutes (F38).
            let cfg = state.config.backup.clone();
            run_mutating_op(state, &exec, req.id, "restore", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::backup::restore_with(
                        ctx,
                        &manifest,
                        &cfg,
                        &snapshot,
                        !skip_safety_copy,
                    )
                    .await
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
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
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
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                record_state(&store, "last_host_meta", |s| s.last_host_meta = now).await;
            }
            resp
        }
        Rpc::AdoptService(m) => {
            run_mutating_op(state, &exec, req.id, "adopt", |ctx| {
                Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
            })
            .await
        }
        Rpc::InstallNative {
            manifest,
            binary_b64,
            unit_file,
        } => {
            run_mutating_op(state, &exec, req.id, "install-native", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::native::install_native(
                        ctx,
                        &manifest,
                        &binary_b64,
                        &unit_file,
                    )
                    .await
                })
            })
            .await
        }
        Rpc::BackupNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, _)) => {
                    let tiers = state
                        .settings
                        .read()
                        .unwrap_or_else(PoisonError::into_inner)
                        .retention
                        .clone();
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
                        deferred: None,
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
                    deferred: None,
                },
            }
        }
        Rpc::UpdateNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, applied_at)) => {
                    let stored_at = Some(applied_at);
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                        deferred: None,
                    };
                    for m in services {
                        let r = run_mutating_op(state, &exec, req.id, "update-native", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::update_native(ctx, &m, stored_at).await
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
                    deferred: None,
                },
            }
        }
        Rpc::ReleaseUpdateNative { stack } => {
            match native_from_state(&state.config.state_dir, &stack).await {
                Ok((services, _)) => {
                    let mut resp = RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("no services on stack '{}'", stack),
                        deferred: None,
                    };
                    for m in services {
                        let r =
                            run_mutating_op(state, &exec, req.id, "release-update-native", |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::release_update(ctx, &m).await
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
                    deferred: None,
                },
            }
        }
        Rpc::PruneOrphans {
            manifest,
            spec,
            confirm,
        } => {
            if confirm != manifest.stack_name {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "confirmation '{}' does not match stack '{}' — nothing removed",
                        confirm, manifest.stack_name
                    ),
                    deferred: None,
                };
            }
            // A1/A2: the same gate every mutating operation passes. Removing
            // files reaches into a container, so the no-touch list and the
            // hostname check apply exactly as they do to a deploy.
            if let Err(e) =
                homelab_core::safety::check_deploy_target(&exec, &state.config.safety, &manifest)
                    .await
            {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                    deferred: None,
                };
            }
            // The same exclusions the deploy applies: the gateway holds other
            // stacks' generated dashboards and routes.
            let keep = homelab_core::ops::deploy::generated_dirs(
                &state.config.safety,
                state.config.grafana_dashboards_dir.as_deref(),
                manifest.vmid,
            );
            let orphans =
                homelab_core::ops::deploy::orphan_files_keeping(&exec, &manifest, &spec, &keep)
                    .await;
            if orphans.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "nothing to remove — everything under /opt/{} is in the repository",
                        manifest.stack_name
                    ),
                    deferred: None,
                };
            }
            let mut removed = 0usize;
            for o in &orphans {
                let path = format!("/opt/{}/{}", manifest.stack_name, o);
                // -f, never -r: this removes FILES the repository dropped.
                // A directory would take whatever else is under it, which is
                // exactly the surprise this whole feature exists to avoid.
                if exec
                    .run(&homelab_core::executor::Cmd::new(
                        "pct",
                        &["exec", &manifest.vmid.to_string(), "--", "rm", "-f", &path],
                        60,
                    ))
                    .await
                    .is_ok()
                {
                    removed += 1;
                    tracing::info!("[prune] removed {}", path);
                }
            }
            RpcResponse {
                id: req.id,
                ok: removed == orphans.len(),
                message: format!("removed {} of {} orphan file(s)", removed, orphans.len()),
                deferred: None,
            }
        }
        // step-22: forget runs the same unregister steps destroy runs
        // (route, scrape target, dashboard, manual checks, front page, host
        // monitors) and records what the stack left behind (ask-9) — under
        // the op-lock like every other operation that writes state.
        Rpc::ForgetStack { stack } => {
            run_mutating_op(state, &exec, req.id, "forget", |ctx| {
                Box::pin(async move { homelab_core::ops::destroy::forget(ctx, &stack).await })
            })
            .await
        }
        // ask-8: `homelab apply` destroys a stack whose directory is gone,
        // from the manifest recorded in state. Every gate of a destroy holds.
        Rpc::DestroyRecorded {
            stack,
            confirm,
            skip_backup,
        } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy_recorded(ctx, &stack, &confirm, skip_backup)
                        .await
                })
            })
            .await
        }
        // ask-9: what a retired entry kept, listed or wiped.
        Rpc::WipeRetired { name, confirm } => match confirm {
            None => {
                let snapshot = match homelab_core::state::StateStore::new(
                    &RealExecutor,
                    &state.config.state_dir,
                )
                .load()
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        return RpcResponse {
                            id: req.id,
                            ok: false,
                            message: format!("state unreadable: {}", e),
                            deferred: None,
                        }
                    }
                };
                match homelab_core::ops::retired::wipe_plan(
                    &snapshot,
                    &name,
                    &state.config.state_dir,
                ) {
                    Ok(plan) => RpcResponse {
                        id: req.id,
                        ok: true,
                        message: plan.render(&name),
                        deferred: None,
                    },
                    Err(why) => RpcResponse {
                        id: req.id,
                        ok: false,
                        message: why,
                        deferred: None,
                    },
                }
            }
            Some(confirm) => {
                run_mutating_op(state, &exec, req.id, "wipe", |ctx| {
                    Box::pin(
                        async move { homelab_core::ops::retired::wipe(ctx, &name, &confirm).await },
                    )
                })
                .await
            }
        },
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
                    deferred: None,
                };
            }
            run_mutating_op(state, &exec, req.id, "apply-guards", |ctx| {
                Box::pin(async move {
                    let mut runner =
                        homelab_core::runner::Runner::new("apply-guards", ctx.sink, ctx.journal);
                    match runner
                        .step("guards", || async {
                            // gap-33: only a managed stack, after the A2
                            // hostname guard, docker guards only where it
                            // runs docker.
                            let store = homelab_core::state::StateStore::new(ctx.exec, &ctx.state_dir);
                            let snapshot = store.load().await.unwrap_or_default();
                            let managed: Vec<homelab_core::ops::guards::ManagedTarget> = snapshot
                                .stacks
                                .iter()
                                .map(|(name, st)| homelab_core::ops::guards::ManagedTarget {
                                    name: name.clone(),
                                    vmid: st.vmid,
                                    // A stack with no stored manifest was adopted
                                    // (`homelab adopt` stores none): a native
                                    // service, so no docker guards (CT 118).
                                    docker: st.manifest.as_ref().map(|m| !m.native_only).unwrap_or(false),
                                })
                                .collect();
                            homelab_core::ops::guards::apply_for_managed(
                                ctx.exec,
                                ctx.sink,
                                &ctx.safety,
                                &managed,
                                vmid,
                                ctx.registry_cache.as_ref(),
                            )
                            .await?;
                            Ok(homelab_core::runner::StepOutcome::Changed)
                        })
                        .await
                    {
                        Ok(_) => {
                            runner.log(
                                homelab_core::sink::Level::Info,
                                format!(
                                    "[guards] {} — runaway guards applied (the docker ones only where the stack runs docker)",
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
                            deferred: None,
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
                ok: homelab_core::ops::fleetcheck::check_passes(&findings),
                message: render_findings(&findings),
                deferred: None,
            }
        }
        // fix-68: always ok with a JSON body. A part that could not be read
        // travels inside it as `unread`, so the TUI can tell this reply from
        // any other by its shape and never mistakes a failure of it for the
        // end of an operation it has open.
        Rpc::Today { stack_files } => RpcResponse {
            id: req.id,
            ok: true,
            message: serde_json::to_string(&gather_today(&exec, state, &stack_files).await)
                .unwrap_or_default(),
            deferred: None,
        },
        // T69: the operator answered a suspended step. Delivering it is all
        // that happens here — the step itself is parked on a channel inside
        // the operation, not on this task.
        Rpc::Answer { id, allow } => {
            let delivered = state
                .pending_asks
                .lock()
                .ok()
                .and_then(|mut g| g.remove(&id))
                .map(|p| p.reply.send(allow).is_ok())
                .unwrap_or(false);
            RpcResponse {
                id: req.id,
                ok: delivered,
                message: if delivered {
                    format!("answer delivered to question {}", id)
                } else {
                    // Not an error worth an incident: a question times out on
                    // its own, so an answer arriving late is ordinary.
                    format!(
                        "question {} is no longer waiting — it timed out or was answered",
                        id
                    )
                },
                deferred: None,
            }
        }
        Rpc::ZfsReplicate => {
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let jobs = state.config.zfs_jobs.clone();
            let resp = run_mutating_op(state, &exec, req.id, "zfs-replicate", |ctx| {
                Box::pin(async move { homelab_core::ops::zfs::replicate(ctx, &jobs, &tiers).await })
            })
            .await;
            if resp.ok {
                let store =
                    homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                record_state(&store, "last_zfs", |s| s.last_zfs = now).await;
            }
            resp
        }
        Rpc::BackupDevices => {
            let devices = state.config.device_backups.clone();
            if devices.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "no device_backups configured in host.toml".into(),
                    deferred: None,
                };
            }
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let mut lines = Vec::new();
            let mut ok = true;
            for dev in devices {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let name = dev.name.clone();
                let report = run_mutating_op(state, &exec, req.id, "device-backup", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
                    })
                })
                .await;
                ok &= report.ok;
                lines.push(format!(
                    "{}: {}",
                    name,
                    if report.ok { "ok" } else { &report.message }
                ));
            }
            RpcResponse {
                id: req.id,
                ok,
                message: lines.join("\n"),
                deferred: None,
            }
        }
        Rpc::ListManualChecks => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let st = store.load().await.unwrap_or_default();
            let rows = homelab_core::ops::manualchecks::listing(&st);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            RpcResponse {
                id: req.id,
                ok: true,
                message: homelab_core::ops::manualchecks::render_listing(&rows, now),
                deferred: None,
            }
        }
        Rpc::AnswerManualCheck {
            check_id: id,
            ok,
            note,
            accept_days,
        } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // fix-65: a deliberate nok, accepted until a date.
            let until = accept_days.map(|d| now + u64::from(d) * 86_400);
            let found = record_state(&store, "manual check answer", |st| match until {
                Some(u) => homelab_core::ops::manualchecks::accept(st, &id, u, &note, now),
                None => homelab_core::ops::manualchecks::answer(st, &id, ok, &note, now),
            })
            .await
            .unwrap_or(false);
            let message = if found {
                match until {
                    Some(u) => format!(
                        "{} recorded as NOT ok, accepted until {}",
                        id,
                        homelab_core::state::ymd(u)
                    ),
                    None => format!("{} recorded as {}", id, if ok { "ok" } else { "NOT ok" }),
                }
            } else {
                format!(
                    "no manual check has id {} — run `homelab checks` for the list",
                    id
                )
            };
            RpcResponse {
                id: req.id,
                ok: found,
                message,
                deferred: None,
            }
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
                deferred: None,
            }
        }
        Rpc::BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
            base_template,
        } => {
            run_mutating_op(state, &exec, req.id, "template-build", |ctx| {
                Box::pin(async move {
                    let defaults = homelab_core::ops::template::TemplateCfg::default();
                    let cfg = homelab_core::ops::template::TemplateCfg {
                        temp_vmid,
                        version,
                        unprivileged,
                        base_template: base_template.unwrap_or(defaults.base_template),
                        ..defaults
                    };
                    homelab_core::ops::template::build_template(ctx, &cfg).await
                })
            })
            .await
        }
        Rpc::ExecIn { vmid, command } => {
            if let Err(e) = exec_allowed(&state.config, vmid) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                    deferred: None,
                };
            }
            // A6: audit every invocation before running it.
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let audit = exec_audit_line(ts, vmid, &command);
            let audit_path = format!("{}/audit.log", state.config.state_dir);
            if let Err(e) = append_audit(&audit_path, &audit) {
                tracing::warn!("audit.log: could not record the exec :: {}", e);
            }
            info!("A6 {}", audit.trim_end());
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
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                    deferred: None,
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
                    deferred: None,
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
                deferred: None,
            }
        }
        Rpc::GetConfig => {
            let view = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let _ = state.log_tx.send(ServerMsg::Config(Box::new(view)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "config".into(),
                deferred: None,
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
                        deferred: None,
                    };
                }
            }
            if view.retention.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "retention needs at least one tier".into(),
                    deferred: None,
                };
            }
            match persist_settings(&state.config, &view) {
                Ok(()) => {
                    *state
                        .settings
                        .write()
                        .unwrap_or_else(PoisonError::into_inner) = *view;
                    info!("settings updated via G8");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: "settings saved and applied".into(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("persist settings: {}", e),
                    deferred: None,
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
                        deferred: None,
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
                    deferred: None,
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
            let mut probes = gather_probes(
                &exec,
                &state.config.state_dir,
                state.config.mirror_remote.as_deref(),
                now,
            )
            .await;
            probes.failed_auth = Some(state.auth_failures.snapshot());
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
                deferred: None,
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
                deferred: None,
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
                deferred: None,
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
                // gap-27: sealed = every secret file on the container has a
                // vault copy. It was hard-coded true, so the check never fired.
                let env_sealed = !present
                    || homelab_core::ops::facts::unsealed_secret_files(exec, state_dir, name, st)
                        .await
                        .is_empty();
                managed_stacks.push(StackProbe {
                    name: name.clone(),
                    backup_age_h: (st.last_backup > 0)
                        .then(|| now_unix.saturating_sub(st.last_backup) / 3600),
                    container_present: present,
                    env_sealed,
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
    // The daemon's own units, held against the copies this binary carries.
    let mut units = Vec::new();
    for u in homelab_core::hostunits::UNITS {
        units.push((u.path, exec.read_file(u.path).await.ok()));
    }
    let host_units_drift = Some(homelab_core::hostunits::drift(&units));
    Probes {
        host_units_drift,
        host_disk_free_pct: disk,
        state_parses,
        managed_stacks,
        offsite_configured,
        offsite_token_valid,
        mirror_behind,
        interrupted_ops: interrupted,
        // fix-120: filled in by the caller, which holds the counter.
        failed_auth: None,
    }
}
