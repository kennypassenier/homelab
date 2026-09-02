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
    let path = std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "/etc/homelab/host.toml".into());
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
    Config {
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
    /// T69: questions a step is waiting on, by id. The client's answer
    /// arrives as an ordinary RPC and is delivered through one of these.
    pending_asks:
        Arc<std::sync::Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<bool>>>>,
    /// Monotonic id for those questions. Not a clock: two questions in the
    /// same second must still be distinguishable.
    next_ask_id: Arc<std::sync::atomic::AtomicU64>,
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
        if let Ok(mut g) = self.state.pending_asks.lock() {
            g.insert(id, tx);
        }
        let _ = self.state.log_tx.send(ServerMsg::Ask {
            id,
            op: q.op.clone(),
            step: q.step.clone(),
            what: q.what.clone(),
            if_allowed: q.if_allowed.clone(),
            if_stopped: q.if_stopped.clone(),
        });
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
        pending_asks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        next_ask_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
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
            verdict(count, largest)
        }
    };
    // Always: a drill that leaves a full restore behind fills the disk the
    // backups need.
    let _ = exec.run(&Cmd::new("rm", &["-rf", target], 300)).await;
    outcome
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
async fn run_backup_batch(
    state: &AppState,
    exec: &RealExecutor,
    jobs: Vec<BackupJob>,
    limit: usize,
) -> std::collections::HashMap<String, bool> {
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
    let results: Vec<(String, bool)> = futures_util::stream::iter(jobs)
        .map(|job| async move {
            let name = job.stack.clone();
            let ok = match job.what {
                BackupWhat::Compose(manifest) => {
                    let cfg = job.cfg.clone();
                    run_op_locked(state, exec, 0, "scheduled-backup", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await
                        })
                    })
                    .await
                    .ok
                }
                // T5: several services share one container, so all of them are
                // backed up and one failure fails the night for the stack —
                // they share a container and a fate. Sequential WITHIN a
                // stack: they are on the same container, so overlapping their
                // pauses would stop that container twice over.
                BackupWhat::Native(services) => {
                    let mut ok = true;
                    for native in services {
                        let cfg = job.cfg.clone();
                        let r = run_op_locked(state, exec, 0, "scheduled-backup-native", |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::backup_native(ctx, &native, &cfg).await
                            })
                        })
                        .await;
                        ok &= r.ok;
                    }
                    ok
                }
            };
            (name, ok)
        })
        .buffer_unordered(limit)
        .collect()
        .await;
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
        // G14: taken before the loop below consumes `snapshot.stacks`.
        let drill_repos: Vec<String> = snapshot
            .stacks
            .values()
            .flat_map(|st| st.apps.iter().cloned())
            .collect();
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
                let backup_ok = backup_done.get(&name).copied().unwrap_or(false);
                let mut update_ok = true;
                for native in st.natives.clone() {
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
            // Y1: the backup already ran in the batch above.
            let backup_ok = backup_done.get(&name).copied().unwrap_or(false);
            if backup_ok {
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
            if !backup_ok || !update_report.ok {
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

        // G14: one restore, rehearsed for real, in turn.
        //
        // Restores into a temporary directory and judges what came back by
        // its LARGEST file rather than its first. That is not fussiness: on
        // 2026-09-02 a hand-run drill declared a restore identical to live by
        // comparing two md5 sums that both belonged to a zero-byte file, and
        // a drill that can be satisfied by empty files rehearses nothing.
        if plan.contains(&NightlyTask::RestoreDrill) {
            if let Some((repo, next)) = homelab_core::ops::restoredrill::next_repo(
                &drill_repos,
                snapshot.restore_drill_index,
            ) {
                let cfg = homelab_core::ops::backup::BackupCfg {
                    tiers: tiers.clone(),
                    ..state.config.backup.clone()
                };
                let target = format!("{}/restore-drill", state.config.state_dir);
                let outcome = run_restore_drill(&exec, &cfg, &repo, &target).await;
                if let Ok(mut sn) = store.load().await {
                    sn.restore_drill_index = next;
                    sn.last_restore_drill_repo = repo.clone();
                    match &outcome {
                        homelab_core::ops::restoredrill::Outcome::Passed {
                            files,
                            largest_bytes,
                        } => {
                            sn.last_restore_drill = now;
                            sn.last_restore_drill_error = None;
                            info!(
                                "restore drill: {} came back with {} file(s), largest {} bytes",
                                repo, files, largest_bytes
                            );
                        }
                        homelab_core::ops::restoredrill::Outcome::Failed(why) => {
                            sn.last_restore_drill_error = Some(why.clone());
                            tracing::error!("restore drill: {} proved nothing :: {}", repo, why);
                        }
                    }
                    let _ = store.save(sn).await;
                }
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
                if problems.is_empty() {
                    info!(
                        "fleet check: repo and reality agree{}",
                        if findings.is_empty() {
                            String::new()
                        } else {
                            format!("\n{}", render_findings(&findings))
                        }
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
pub const MAX_WS_FRAME: usize = 64 * 1024 * 1024;

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
        let req = match serde_json::from_str::<RpcRequest>(&text) {
            Ok(r) => r,
            Err(e) => {
                // Silence here is why `homelab checks answer` looked like a
                // hang rather than a bug: the request was dropped and the
                // client waited for a reply that was never coming. A frame
                // this end cannot understand is a fault on this end.
                tracing::error!("unparseable request dropped :: {} :: {}", e, text);
                continue;
            }
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
        .unwrap()
        .should_send(&report.op, report.ok, error.as_deref(), now)
    {
        return;
    }
    let payload =
        homelab_core::notify::op_payload(&report.op, label, report.ok, error.as_deref(), VERSION);
    notify_raw(state, exec, payload).await;
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
    let primary = state.settings.read().unwrap().notify_webhook.clone();
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
        let auth = bearer.map(|t| format!("authorization: Bearer {}", t));
        let mut args: Vec<&str> = vec![
            "-m",
            "5",
            "-s",
            "-o",
            "/dev/null",
            // The status is the whole point: -o /dev/null throws the body
            // away, and without this the exit code alone cannot tell a 200
            // from a 404 on a topic that no longer exists.
            "-w",
            "%{http_code}",
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
        args.push(url);
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
                tracing::warn!("notification route {} failed: {}", url, last);
            }
        }
    }
    record_notify_outcome(state, exec, delivered, &last).await;
}

/// Keep the last word on whether notifications are arriving, so a broken
/// notification path is visible somewhere other than in a notification.
///
/// That circularity is real and worth stating: if every route is down, this
/// record is what `homelab check` and the TUI read, because the report saying
/// so cannot reach him by the path that is broken.
async fn record_notify_outcome(state: &AppState, exec: &RealExecutor, delivered: bool, why: &str) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let Ok(mut st) = store.load().await else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if delivered {
        st.last_notify_ok = now;
        st.last_notify_error = None;
    } else {
        st.last_notify_failed = now;
        st.last_notify_error = Some(why.to_string());
    }
    let _ = store.save(st).await;
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
/// F184: total RAM, memory promised to guests, and swap in use — the three
/// numbers that decide whether "give this container more memory" is advice
/// or nonsense. None when any of them cannot be read, which is deliberately
/// not the same as a healthy host.
async fn read_host_memory(exec: &RealExecutor) -> Option<(u32, u32, u32, u32)> {
    let out = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                // free gives total and swap; pct/qm give what is promised.
                "free -m | awk '/^Mem:/{print $2} /^Swap:/{print $3\" \"$2}'; \
                 pct list 2>/dev/null | awk 'NR>1{print $1}' | \
                   xargs -r -n1 pct config 2>/dev/null | awk '/^memory:/{s+=$2} END{print s+0}'; \
                 qm list 2>/dev/null | awk 'NR>1{s+=$4} END{print s+0}'",
            ],
            60,
        ))
        .await
        .ok()?;
    let n: Vec<&str> = out.stdout.split_whitespace().collect();
    // total, swap_used, swap_total, lxc_committed, vm_committed
    if n.len() < 5 {
        return None;
    }
    let p = |i: usize| n.get(i)?.parse::<u32>().ok();
    Some((p(0)?, p(3)? + p(4)?, p(1)?, p(2)?))
}

async fn gather_live_facts(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
) -> homelab_core::ops::fleetcheck::LiveFacts {
    use homelab_core::executor::{Cmd, Executor};
    // O1: what the router (and anything else outside this suite) uploaded
    // last night. Read through the rclone remote that already exists for the
    // restic repositories — no new credential, no new timer.
    let mut watched = Vec::new();
    for w in &state.config.watched_backups {
        let out = exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!(
                        "rclone lsjson --files-only '{}' 2>&1 | \
                         sed -n 's/.*\"ModTime\":\"\\([^\"]*\\)\".*/\\1/p' | sort | tail -1",
                        w.rclone_path
                    ),
                ],
                180,
            ))
            .await;
        let mut fact = homelab_core::ops::fleetcheck::WatchedBackupFact {
            name: w.name.clone(),
            max_age_s: w.max_age_hours * 3600,
            ..Default::default()
        };
        match out {
            Err(e) => fact.error = Some(e.to_string()),
            Ok(o) if !o.success() => fact.error = Some(o.stderr.trim().to_string()),
            Ok(o) => {
                let newest = o.stdout.trim().to_string();
                if !newest.is_empty() {
                    // rclone prints RFC3339; the host has `date` and this
                    // avoids a chrono dependency in a place that has none.
                    if let Ok(d) = exec
                        .run(&Cmd::new("date", &["-d", &newest, "+%s"], 30))
                        .await
                    {
                        if let Ok(t) = d.stdout.trim().parse::<u64>() {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|x| x.as_secs())
                                .unwrap_or(0);
                            fact.newest_age_s = Some(now.saturating_sub(t));
                        }
                    }
                }
            }
        }
        watched.push(fact);
    }
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // T49: the seeder's verdict, read from the file it writes beside the
    // generated monitor list. Same directory, so there is no second setting
    // to keep in step with the first.
    let seed = match state.config.kuma_monitors_file.as_deref() {
        None => homelab_core::ops::fleetcheck::SeedFact {
            judged: true,
            age_s: Some(0),
            ..Default::default()
        },
        Some(monitors) => {
            let dir = monitors.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            let path = format!("{}/last-seed.json", dir);
            match exec.read_file(&path).await {
                Err(e) => homelab_core::ops::fleetcheck::SeedFact {
                    error: Some(format!("{}: {}", path, e)),
                    ..Default::default()
                },
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Err(e) => homelab_core::ops::fleetcheck::SeedFact {
                        error: Some(format!("{} is not JSON: {}", path, e)),
                        ..Default::default()
                    },
                    Ok(v) => homelab_core::ops::fleetcheck::SeedFact {
                        stale: v["stale"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        age_s: v["at"].as_u64().map(|at| now_unix.saturating_sub(at)),
                        judged: v["judged"].as_bool().unwrap_or(false),
                        error: None,
                    },
                },
            }
        }
    };

    let mut facts = homelab_core::ops::fleetcheck::LiveFacts {
        seed,
        stack_files: stack_files.to_vec(),
        watched_backups: watched,
        // F184: the host's own numbers, so a per-container remedy cannot
        // advise memory the machine does not have.
        host_memory: read_host_memory(exec).await,
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
    let window = sane_window(&state.config.logs_window);
    // Which dashboards Grafana actually holds, asked once rather than per
    // stack. Grafana is the reader; the deploy is the writer, and the writer
    // has been reporting success into a dead directory (F149). Only asked
    // when a dashboards directory is configured — an unasked question must
    // never become a finding.
    let provisioned: Option<Vec<String>> = match state.config.grafana_dashboards_dir.as_deref() {
        Some(dir) => grafana_generated_uids(exec, state.config.safety.gateway_vmid, dir).await,
        None => None,
    };
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
                    // `container_name=~".+"` is not decoration. F79 was not
                    // silence: lines kept arriving for months while promtail
                    // read `attrs.name`, a field docker does not write, so
                    // every line landed without the label the dashboards
                    // query by. Counting lines would have passed throughout.
                    // Counting LABELLED lines is the question that was
                    // actually being got wrong.
                    let q = format!(
                        "{}/loki/api/v1/query?query=sum(count_over_time(%7Bstack%3D%22{}%22%2Ccontainer_name%3D~%22.%2B%22%7D%5B{}%5D))",
                        base.trim_end_matches('/'),
                        name,
                        window
                    );
                    c.logs_recent = Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"value\""))
                            .unwrap_or(false),
                    );
                }
                if let Some(uids) = provisioned.as_ref() {
                    let uid = format!("homelab-{}", name);
                    c.dashboard_provisioned = Some(uids.iter().any(|u| u == &uid));
                }
                facts.coverage.push(c);
            }
        }
    }
    facts
}

/// The uids of the generated dashboards Grafana is actually serving.
///
/// Asked of Grafana over its own API, with the credentials read from the
/// app's `.env` at the moment of asking — the same reasoning as Jellyfin's
/// key (F131): a credential handed to a check goes stale without telling
/// anybody, and the service itself is the only source that cannot.
///
/// The `.env` path is DERIVED from the configured dashboards directory
/// rather than typed, because a second typed path is exactly what caused the
/// fault this question exists to catch. `/opt/gateway/grafana/dashboards-generated`
/// → `/opt/gateway/grafana/.env`.
///
/// `None` means the question could not be asked at all (no parent directory,
/// the container unreachable, Grafana refusing) — never an empty answer, so a
/// gateway that is down does not turn every stack into a finding.
async fn grafana_generated_uids(
    exec: &dyn homelab_core::executor::Executor,
    gateway_vmid: u16,
    dashboards_dir: &str,
) -> Option<Vec<String>> {
    let app_dir = std::path::Path::new(dashboards_dir).parent()?.to_str()?;
    let script = format!(
        "U=$(grep -h GRAFANA_GF_ADMIN_USER {0}/.env | cut -d= -f2);          P=$(grep -h GRAFANA_GF_ADMIN_PASSWORD {0}/.env | cut -d= -f2);          curl -s -m 15 -u \"$U:$P\" 'http://127.0.0.1:3000/api/search?tag=generated&limit=500'",
        app_dir
    );
    let out = exec
        .run(&Cmd::new(
            "pct",
            &["exec", &gateway_vmid.to_string(), "--", "sh", "-c", &script],
            60,
        ))
        .await
        .ok()?;
    if !out.stdout.contains("\"uid\"") {
        return None;
    }
    Some(
        out.stdout
            .split("\"uid\":\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(str::to_string)
            .collect(),
    )
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
                Severity::Noted => "noted",
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
                };
            }
            let orphans = homelab_core::ops::deploy::orphan_files(&exec, &manifest, &spec).await;
            if orphans.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "nothing to remove — everything under /opt/{} is in the repository",
                        manifest.stack_name
                    ),
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
                            homelab_core::ops::guards::apply(ctx.exec, ctx.sink, vmid, true, ctx.registry_cache.as_ref()).await?;
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
        // T69: the operator answered a suspended step. Delivering it is all
        // that happens here — the step itself is parked on a channel inside
        // the operation, not on this task.
        Rpc::Answer { id, allow } => {
            let delivered = state
                .pending_asks
                .lock()
                .ok()
                .and_then(|mut g| g.remove(&id))
                .map(|tx| tx.send(allow).is_ok())
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
            }
        }
        Rpc::AnswerManualCheck {
            check_id: id,
            ok,
            note,
        } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let mut st = store.load().await.unwrap_or_default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let found = homelab_core::ops::manualchecks::answer(&mut st, &id, ok, &note, now);
            let message = if found {
                let _ = store.save(st).await;
                format!("{} recorded as {}", id, if ok { "ok" } else { "NOT ok" })
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
