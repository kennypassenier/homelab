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

mod secrets;
mod tls;
mod ui_relay;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// fix-141: the tree this binary was built from (see build.rs).
const BUILD: &str = env!("HOMELAB_BUILD");

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

/// arch-tokens: one `[[tokens]]` entry in host.toml.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct TokenEntry {
    /// Who holds it, e.g. "admin" or "wsl"; named in every audit line.
    name: String,
    scope: homelab_proto::Scope,
    /// Lowercase hex SHA-256 of the token itself.
    sha256: String,
}

/// Who a session belongs to, decided once when it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    name: String,
    scope: homelab_proto::Scope,
}

/// arch-tokens: a token list the host can trust, or the reason it cannot.
fn validate_tokens(tokens: &[TokenEntry]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for t in tokens {
        if t.name.trim().is_empty() {
            return Err("a [[tokens]] entry has an empty name".into());
        }
        if t.name == "legacy" {
            return Err(
                "\"legacy\" is the name of the single `token` key; pick another name".into(),
            );
        }
        if !seen.insert(t.name.as_str()) {
            return Err(format!("two [[tokens]] entries are named {:?}", t.name));
        }
        let hex = t.sha256.len() == 64
            && t.sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !hex {
            return Err(format!(
                "[[tokens]] {:?}: sha256 must be 64 lowercase hex characters (sha256sum of the token)",
                t.name
            ));
        }
    }
    Ok(())
}

/// host.toml as written: the one serde representation of the file.
///
/// config-four-representations (expert panel, 2026-09-27): the same struct is
/// read, checked for keys it does not read (`unknown_keys`), and written
/// back by a settings save (`render_settings_toml`). There used to be a
/// second, hand-kept struct for the write and hand-kept key lists for the
/// check, held in step by tests that scraped this source file. Every field
/// is optional so that what is written back is only what the file said;
/// defaults are applied when `Config` is resolved.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
struct FileConfig {
    token: Option<String>,
    /// arch-tokens (homelab-admin, 2026-09-28): one entry per machine, each
    /// with a scope. Only the SHA-256 of a token is stored here; the token
    /// itself lives with the client. `token` above keeps working as scope
    /// `all` under the name "legacy" until it is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<Vec<TokenEntry>>,
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
    /// fix-62 (restore-drill-covers-almost-nothing, 2026-10-01): where the
    /// nightly restore drill restores a repository to — a data pool, not the
    /// root disk, same reasoning as `native_backup_staging_dir`.
    restore_drill_scratch_dir: Option<String>,
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
    /// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
    /// LAN-only self-signed certificate (PEM) an `https://` route is pinned
    /// to — the host's own copy, for curl's `--cacert`. Required once any
    /// notify route is `https://` (`startup_problems`); absent otherwise.
    notify_tls_cert: Option<String>,
    /// The SHA-256 of that certificate's DER form, lowercase hex. Checked
    /// against the file on disk at every start: a mismatch means the file
    /// changed since it was pinned (the hub's certificate was regenerated,
    /// or the file is the wrong one), and is refused the same way a
    /// changed host certificate refuses the client (`reconcile_pin`).
    notify_tls_fingerprint: Option<String>,
    /// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01):
    /// the read-only Cloudflare API token the host's nightly edge check
    /// uses, mirroring the workstation's `~/.config/cloudflare/kp-soft.token`
    /// (`client/src/edge.rs`). A secret: changed over ssh or the dashboard's
    /// secret field, never shown back. Absent = the nightly comparison
    /// reports "not configured", not broken.
    cloudflare_token: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    /// fix-93 (2026-09-27): the container Loki runs in. Loki's LAN port
    /// takes pushes only since then, so the coverage check asks from inside
    /// this container. Absent = asked at `loki_url` from the host.
    loki_vmid: Option<u16>,
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
    /// rule-20 (disk-audit, 2026-10-01): `[default_log_rotation]` — the
    /// fleet default a data mount gets when it declares no `rotate:` of its
    /// own and does not opt out. Absent = no fleet default (today's
    /// behaviour: only an explicit `rotate:` rotates anything).
    default_log_rotation: Option<homelab_core::ops::guards::FleetLogRotationDefault>,
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
    /// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): where a
    /// `backup_pause: chassis` native backup stages its local copy before
    /// restic uploads it. Absent/empty = no staging.
    native_backup_staging_dir: Option<String>,
    /// fix-113 ADDENDUM: the staging cap, in MiB, before its 20% margin.
    native_backup_staging_cap_mib: Option<u64>,
    /// T1: directory the orchestrator writes per-stack Prometheus discovery
    /// files into. Absent = off, and the scrape list stays hand-maintained.
    metrics_targets_dir: Option<String>,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// dashboard's own address (CT 120), from which the firewall step of a
    /// deploy derives an inbound rule per stack for every tile that opens on
    /// that stack's own container. Absent or empty = feature off, and no
    /// rule is derived.
    tile_watch_source: Option<String>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    ask_timeout_s: Option<u64>,
    /// Decision "23 constants" (Kenny, 2026-09-30): how long a container's
    /// updates may stand before `homelab check` says so. Default 7 days.
    patch_threshold_s: Option<u64>,
    /// How stale a stack's backup may be before it counts as a finding.
    /// Default 48 hours.
    backup_max_age_s: Option<u64>,
    /// How old the daemon's own state backup (vault, state.json, TLS,
    /// host.toml) may be before it counts as a finding. Default 48 hours.
    host_meta_max_age_s: Option<u64>,
    /// How often the nightly report repeats an unchanged set of findings.
    /// Default 7 days.
    nightly_report_repeat_s: Option<u64>,
    /// fix-131: incident bundles older than this many days are pruned.
    /// Default 90.
    incident_bundle_max_age_days: Option<u64>,
    /// fix-131: at most this many incident bundles are kept. Default 200.
    incident_bundle_max_count: Option<usize>,
    /// gap-26: `journal.jsonl` is cut back to half of this many bytes once
    /// it grows past it (`compact_journal_file`). Default 4 MiB
    /// (`incidents::JOURNAL_MAX_BYTES`) — was a fixed constant with no
    /// host.toml key and no dashboard row, unlike the incident-bundle
    /// limits right above it.
    journal_max_bytes: Option<u64>,
    /// fix-122 (AR15's JSONL ring, Kenny's go 2026-10-01): size cap in bytes
    /// of `logs/host.jsonl`, the daemon's own trace ring — cut back to half
    /// of it, the same way `journal.jsonl` is, once it grows past it.
    /// Default [`homelab_core::logring::LOG_RING_MAX_BYTES`].
    log_ring_max_bytes: Option<u64>,
    /// fix-122: a `tracing`/`EnvFilter` directive (e.g. "info", "debug",
    /// "homelab_host=debug,info"), applied to both the journald sink and the
    /// JSONL ring. Default "info". Editable from the dashboard and applied
    /// live — no restart — unlike `log_ring_max_bytes` above it.
    log_level: Option<String>,
    /// How long one GitHub answer about a pinned upstream stands. Default
    /// 20 hours.
    upstream_max_age_s: Option<u64>,
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
    backup_concurrency: Option<usize>,
    /// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): the ZFS
    /// dataset that holds the second repository set, e.g. `HDD4TB/restic`.
    /// Absent = no second copy (the nightly `restic check` of the Google
    /// Drive copy runs either way).
    second_copy_dataset: Option<String>,
    /// fix-96: how often one repository's check also reads a slice of its
    /// data. Default 30 days (Kenny: monthly).
    integrity_data_read_interval_s: Option<u64>,
    /// feat-platform-2 (homelab-admin, 2026-09-28): seconds between two
    /// readings of every container's real status. Default 60; at least 10.
    status_interval_s: Option<u64>,
    /// feat-platform-3: how many of the newest operation lines the host
    /// keeps in memory for a client that connects mid-operation. Default 2000.
    recent_lines: Option<usize>,
    /// arch-history: days of history.jsonl kept (default 90) and its size
    /// ceiling in MiB (default 16); past the ceiling the oldest half goes.
    history_days: Option<u64>,
    history_max_mib: Option<usize>,
    /// Decision notify-detail (2026-09-30): the dashboard's public address,
    /// for the link a push carries (`click_url`). Unset: no link.
    dashboard_url: Option<String>,
    /// replace-kuma (2026-09-30): the dashboard's health address, asked every
    /// minute; five minutes without an answer is an urgent notice, as the
    /// dashboard does for the host. Unset: not watched.
    watch_url: Option<String>,
    /// Owner decision "default plus per tile" (2026-09-30): how often the
    /// host asks `watch_url`, and the fleet default the `Tiles` RPC hands
    /// the dashboard for its own minute watch. Default 60.
    watch_interval_s: Option<u64>,
    /// How long `watch_url` (or, by fleet default, a tile) may fail before
    /// it counts as down. Default 300.
    watch_down_after_s: Option<u64>,
    /// rule-20 (disk-audit, 2026-10-01): `[capacity_thresholds]` — warn and
    /// critical percentages for pve's root fs, the local-lvm thin pool, every
    /// ZFS pool, pve's journald and Prometheus' TSDB. Absent = the audit's
    /// own defaults (`HostCapacityThresholds::default()`).
    capacity_thresholds: Option<homelab_core::ops::fleetcheck::HostCapacityThresholds>,
    /// rule-20: the configured `--storage.tsdb.retention.size` of whatever
    /// runs Prometheus, in MiB — Kenny's own number, matching what the
    /// stack's compose file declares; core has no business knowing it.
    /// Absent (with `prometheus_url`) = the question is not asked, same as
    /// every other Prometheus-backed reading in this daemon.
    tsdb_retention_size_mib: Option<u64>,
}

#[derive(Clone)]
struct Config {
    token: String,
    /// arch-tokens: scoped tokens from `[[tokens]]`, validated at start.
    tokens: Vec<TokenEntry>,
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
    /// fix-62: where the nightly restore drill restores a repository to — a
    /// data pool, not the root disk. "none" is not a valid value (unlike
    /// staging, the drill always needs somewhere to restore to); an absent
    /// `host.toml` key falls back to `DEFAULT_DRILL_SCRATCH_DIR`.
    restore_drill_scratch_dir: String,
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
    /// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
    /// LAN-only self-signed certificate (PEM) an `https://` route is pinned
    /// to — the host's own copy, for curl's `--cacert`. Required once any
    /// notify route is `https://` (`startup_problems`); absent otherwise.
    notify_tls_cert: Option<String>,
    /// The SHA-256 of that certificate's DER form, lowercase hex. Checked
    /// against the file on disk at every start: a mismatch means the file
    /// changed since it was pinned (the hub's certificate was regenerated,
    /// or the file is the wrong one), and is refused the same way a
    /// changed host certificate refuses the client (`reconcile_pin`).
    notify_tls_fingerprint: Option<String>,
    /// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01):
    /// the read-only Cloudflare API token the host's nightly edge check
    /// uses, mirroring the workstation's `~/.config/cloudflare/kp-soft.token`
    /// (`client/src/edge.rs`). A secret: changed over ssh or the dashboard's
    /// secret field, never shown back. Absent = the nightly comparison
    /// reports "not configured", not broken.
    cloudflare_token: Option<String>,
    /// Where the coverage check asks whether a stack is measured and whether
    /// its logs arrive. Unset means the question is not asked at all, which
    /// is deliberate: an unasked question must never become a finding.
    prometheus_url: Option<String>,
    loki_url: Option<String>,
    /// fix-93: where the log question is asked; see the file field.
    loki_vmid: Option<u16>,
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
    /// rule-20: the fleet default log rotation; see `FileConfig`.
    default_log_rotation: Option<homelab_core::ops::guards::FleetLogRotationDefault>,
    /// T1: where per-stack Prometheus discovery files are written.
    metrics_targets_dir: Option<String>,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// dashboard's own address (CT 120), from which the firewall step of a
    /// deploy derives an inbound rule per stack for every tile that opens on
    /// that stack's own container. Absent or empty = feature off, and no
    /// rule is derived.
    tile_watch_source: Option<String>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    ask_timeout_s: u64,
    /// Decision "23 constants": [`homelab_core::ops::fleetcheck::PATCH_THRESHOLD_S`] by default.
    patch_threshold_s: u64,
    /// [`homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S`] by default.
    backup_max_age_s: u64,
    /// [`homelab_core::ops::fleetcheck::HOST_META_MAX_AGE_S`] by default.
    host_meta_max_age_s: u64,
    /// [`homelab_core::ops::fleetcheck::NIGHTLY_REPORT_REPEAT_S`] by default.
    nightly_report_repeat_s: u64,
    /// [`homelab_core::incidents::BUNDLE_MAX_AGE_DAYS`] by default.
    incident_bundle_max_age_days: u64,
    /// [`homelab_core::incidents::BUNDLE_MAX_COUNT`] by default.
    incident_bundle_max_count: usize,
    /// gap-26: [`homelab_core::incidents::JOURNAL_MAX_BYTES`] by default.
    journal_max_bytes: u64,
    /// fix-122: [`homelab_core::logring::LOG_RING_MAX_BYTES`] by default.
    /// Restart-applied, like `journal_max_bytes` beside it: the ring
    /// writer's cap is read once, at `init_production_logging`.
    log_ring_max_bytes: u64,
    /// fix-122: the daemon's startup filter — `RUST_LOG` wins over this when
    /// set, exactly as it always has. Live-reloadable afterwards through
    /// `settings.log_level` and `AppState::log_filter`; this field is read
    /// only once, to seed the subscriber.
    log_level: String,
    /// [`homelab_core::ops::pins::UPSTREAM_MAX_AGE_S`] by default.
    upstream_max_age_s: u64,
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
    /// fix-96: the second repository set's dataset; None = no second copy.
    second_copy_dataset: Option<String>,
    /// fix-96: how often a repository's check reads data.
    integrity_data_read_interval_s: u64,
    /// feat-platform-2: seconds between two status readings.
    status_interval_s: u64,
    /// feat-platform-3: how many of the newest lines `CurrentOp` returns.
    recent_lines: usize,
    /// arch-history: how long and how large history.jsonl may grow.
    history_max_age_s: u64,
    history_max_bytes: usize,
    /// Decision notify-detail: the dashboard's public address.
    dashboard_url: String,
    /// replace-kuma: the dashboard's health address, when watched.
    watch_url: Option<String>,
    /// Owner decision "default plus per tile": how often `watch_url` is
    /// asked, and the fleet default handed to the dashboard's own watch.
    watch_interval_s: u64,
    /// How long `watch_url` (or, by fleet default, a tile) may fail before
    /// it counts as down.
    watch_down_after_s: u64,
    /// rule-20: warn/critical thresholds for pve's own capacity readings.
    capacity_thresholds: homelab_core::ops::fleetcheck::HostCapacityThresholds,
    /// rule-20: Prometheus' own TSDB cap, in MiB; None = not asked.
    tsdb_retention_size_mib: Option<u64>,
    /// Initial mutable settings (live copy lives in AppState.settings).
    initial_settings: homelab_proto::HostConfigView,
    /// host.toml as it was read; a settings save writes this back with only
    /// the settings changed.
    file: FileConfig,
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
/// The keys it reads are found by reading the file into `FileConfig` and
/// writing that back: whatever the round trip lost, no field reads
/// (config-four-representations, 2026-09-27; this replaced hand-kept key
/// lists that had drifted, see the retention `keep` test). Returned as
/// dotted paths, e.g. `registry_cache.upstreams[0].opnsense_url`. A file
/// that does not deserialize returns nothing here; loading it fails loudly.
fn unknown_keys(raw: &toml::Table) -> Vec<String> {
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
    let as_read = file.clone();

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
    if file.status_interval_s.is_some_and(|s| s < 10) {
        eprintln!("FATAL: {}: status_interval_s must be at least 10 (each reading runs one probe per container)", path);
        std::process::exit(1);
    }
    // fix-122: the runtime debug toggle, checked the same way at start as it
    // is at `SetHostConfig` (`startup_problems`) — a directive string
    // `EnvFilter` cannot parse would otherwise be written to host.toml and
    // only fail the NEXT restart, silently, with the daemon logging nothing
    // at all.
    let log_level = file
        .log_level
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "info".to_string());
    if tracing_subscriber::EnvFilter::try_new(&log_level).is_err() {
        eprintln!(
            "FATAL: {}: log_level {:?} is not a tracing/EnvFilter directive (e.g. \"info\", \"debug\")",
            path, log_level
        );
        std::process::exit(1);
    }
    let tokens = file.tokens.clone().unwrap_or_default();
    if let Err(e) = validate_tokens(&tokens) {
        eprintln!("FATAL: {}: {}", path, e);
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
        // rule-20: absent from host.toml = no fleet default, same as before.
        default_log_rotation: file.default_log_rotation,
        // rule-20: absent from host.toml = the audit's own defaults.
        capacity_thresholds: file.capacity_thresholds.unwrap_or_default(),
        tsdb_retention_size_mib: file.tsdb_retention_size_mib,
        backup: {
            let d = homelab_core::ops::backup::BackupCfg::default();
            homelab_core::ops::backup::BackupCfg {
                restic_base: file.restic_base.unwrap_or(d.restic_base),
                password_file: file.restic_password_file.unwrap_or(d.password_file),
                snapshot_timeout_s: file
                    .restic_snapshot_timeout_s
                    .unwrap_or(d.snapshot_timeout_s),
                restore_timeout_s: file.restic_restore_timeout_s.unwrap_or(d.restore_timeout_s),
                // Unset: the default staging dir. "none" or empty: no staging.
                staging_dir: match file.native_backup_staging_dir {
                    None => d.staging_dir,
                    Some(s) if s.trim().is_empty() || s.trim() == "none" => None,
                    Some(s) => Some(s),
                },
                staging_cap_mib: file
                    .native_backup_staging_cap_mib
                    .unwrap_or(d.staging_cap_mib),
                tiers: d.tiers,
            }
        },
        metrics_targets_dir: file.metrics_targets_dir,
        tile_watch_source: file.tile_watch_source.filter(|s| !s.trim().is_empty()),
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
             network segment can read it; an https:// route closes this",
            route
        );
    }
    // fix-126: an https:// route is already refused at start without a pin
    // (`startup_problems`); this is the one check that needs the file on
    // disk, so it runs here instead — a mismatch means the hub's
    // certificate changed (or notify_tls_cert points at the wrong file) and
    // is as loud as a client refusing a changed host certificate.
    if let (Some(path), Some(want)) = (&cfg.notify_tls_cert, &cfg.notify_tls_fingerprint) {
        match std::fs::read_to_string(path).map(|pem| homelab_core::notify::cert_fingerprint(&pem))
        {
            Ok(Ok(got)) if &got != want => {
                tracing::error!(
                    "notify_tls_cert {} does not match notify_tls_fingerprint ({}; the file is \
                     {}) — notifications over https will be refused until this is fixed: either \
                     the hub's certificate changed (read its fingerprint again and update \
                     notify_tls_fingerprint) or notify_tls_cert is the wrong file",
                    path,
                    want,
                    got
                );
            }
            Ok(Err(e)) => {
                tracing::error!("notify_tls_cert {} is not a valid certificate: {}", path, e);
            }
            Err(e) => {
                tracing::error!("notify_tls_cert {} could not be read: {}", path, e);
            }
            Ok(Ok(_)) => {}
        }
    }
    cfg
}

/// G8: render host.toml for a settings save: the file as it was read, with
/// only the settings the TUI may change replaced.
///
/// config-four-representations (expert panel, 2026-09-27): this serialises
/// `FileConfig` itself. It used to fill a separate hand-kept struct from the
/// resolved `Config`, so every new field had to be added twice, and a field
/// forgotten there was wiped from host.toml by the next save (F208). It also
/// wrote resolved values the file never said: the compiled no-touch list
/// merged in, the default drill interval, a token from the environment.
/// Comments in host.toml are not kept, as before.
/// fix-134 (config-four-representations, expert panel, 2026-09-27):
/// `render_settings_toml` used to round-trip the file through `FileConfig`
/// (parse into the struct, re-serialize the whole struct), which is how
/// every comment and the human's own key order were lost on the first
/// settings-tab save. Editing in place with `toml_edit` touches only the
/// three keys a settings save actually changes — everything else in the
/// file, comments included, passes through byte-for-byte.
fn render_settings_toml(
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
    write_config_file(&config.config_path, &raw)
}

/// feat-settings-1: the start-up validation's refusals for a file, without
/// the process exit, so a settings change is held to the same checks
/// before it is written. `load_config_from` exits on the same list.
fn startup_problems(file: &FileConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = &file.token {
        if t.len() < 16 {
            out.push("token must be at least 16 characters".into());
        }
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
    if let Some(l) = &file.listen {
        if l.parse::<SocketAddr>().is_err() {
            out.push(format!("listen {:?} must be host:port", l));
        }
    }
    if file.backup_hour.is_some_and(|h| h > 23) {
        out.push("backup_hour must be 0-23".into());
    }
    if file.retention.as_ref().is_some_and(|r| r.is_empty()) {
        out.push("retention needs at least one tier".into());
    }
    if let Some(level) = &file.log_level {
        if !level.trim().is_empty() && tracing_subscriber::EnvFilter::try_new(level).is_err() {
            out.push(format!(
                "log_level {:?} is not a tracing/EnvFilter directive (e.g. \"info\", \"debug\")",
                level
            ));
        }
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

/// JSON as a TOML value; null has no TOML form.
fn json_to_toml(v: &serde_json::Value) -> Result<toml::Value, String> {
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

/// fix-110: the non-secret keys host.toml sets right now, as JSON — for the
/// fleet check's comparison with `config/host.toml` (`evaluate_host_config_drift`).
/// Unlike `host_config_view` this never fails: an unreadable or unparsable
/// file is simply "sets nothing", which is also true of a host that has
/// never had one.
fn live_host_config_table(raw: &str) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(table) = toml::from_str::<toml::Table>(raw) else {
        return out;
    };
    for (key, v) in &table {
        if homelab_core::hostconfig::is_secret(key) {
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
fn host_config_view(path: &str, raw: &str) -> Result<homelab_proto::HostConfigFile, String> {
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
    Ok(homelab_proto::HostConfigFile {
        path: path.to_string(),
        sha256: homelab_core::manifest::sha256_hex(raw.as_bytes()),
        values,
        secrets_set,
        unknown: unknown_keys(&table),
    })
}

/// fix-false-alarm (2026-09-30): the URL to reach once before a
/// `SetHostConfig` save is accepted, when `changes` sets `watch_url`.
/// `None` = nothing to check: the key is absent from this save, or set to
/// empty — turning the watch off is always allowed, since it can never
/// itself cause a false alarm.
fn watch_url_to_check(
    changes: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Option<String> {
    let s = changes.get("watch_url")?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// The reason to refuse the save because the host's own request to `url`
/// did not succeed (`reached` is what running the same probe
/// `watch_dashboard` uses — `curl -sf -m 10` — reported), or None when the
/// save may proceed. Pure: the caller does the actual request.
fn watch_url_refusal(url: &str, reached: bool) -> Option<String> {
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
fn apply_host_config_changes(
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
fn apply_host_config_whole(
    raw: &str,
    declared: &str,
    expect_sha256: Option<&str>,
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
fn write_config_file(path: &str, raw: &str) -> Result<(), String> {
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

/// fix-120: a fresh per-machine bearer, 32 random bytes as lowercase hex.
/// `/dev/urandom` rather than a new crate dependency: this is the one place
/// the host needs cryptographic randomness.
fn random_token() -> Result<String, String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| format!("/dev/urandom: {}", e))?;
    let mut buf = [0u8; 32];
    f.read_exact(&mut buf)
        .map_err(|e| format!("/dev/urandom: {}", e))?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

/// fix-120: the `[[tokens]]` list out of raw host.toml text, read-modify-
/// write helpers share this so neither duplicates the parse.
fn read_tokens(raw: &str) -> Result<(toml::Table, Vec<TokenEntry>), String> {
    let table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("host.toml does not parse as TOML: {}", e))?
    };
    let file: FileConfig = table
        .clone()
        .try_into()
        .map_err(|e| format!("host.toml is not a valid host config: {}", e))?;
    Ok((table, file.tokens.unwrap_or_default()))
}

/// fix-120: `tokens` written back into `table` and the whole file
/// re-rendered, after `validate_tokens` has already passed.
fn write_tokens(mut table: toml::Table, tokens: &[TokenEntry]) -> Result<String, String> {
    if tokens.is_empty() {
        table.remove("tokens");
    } else {
        table.insert(
            "tokens".into(),
            toml::Value::try_from(tokens).map_err(|e| e.to_string())?,
        );
    }
    toml::to_string_pretty(&table).map_err(|e| e.to_string())
}

/// fix-120 (per-machine tokens, owner decision 2026-10-01): the current
/// `[[tokens]]` list plus a new entry named `name` at `scope`, whose SHA-256
/// is of `plain_token` — pure, so the random token itself is handed in and
/// tested separately. Refuses an empty or duplicate name, `"legacy"`, and
/// anything `validate_tokens` would refuse at start.
fn issue_token_in(
    raw: &str,
    name: &str,
    scope: homelab_proto::Scope,
    plain_token: &str,
) -> Result<(String, Vec<TokenEntry>), String> {
    let (table, mut tokens) = read_tokens(raw)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("a token needs a name".into());
    }
    use sha2::{Digest, Sha256};
    let sha256: String = Sha256::digest(plain_token.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    tokens.push(TokenEntry {
        name: name.to_string(),
        scope,
        sha256,
    });
    validate_tokens(&tokens)?;
    let text = write_tokens(table, &tokens)?;
    Ok((text, tokens))
}

/// fix-120: the current `[[tokens]]` list with `name` removed, or the
/// reason it is refused: `"legacy"` is the single `token` key, cleared over
/// ssh instead (OPERATIONS_RUNBOOK's migration note), and a name that is
/// not there is refused rather than silently a no-op.
fn revoke_token_in(raw: &str, name: &str) -> Result<(String, Vec<TokenEntry>), String> {
    if name == "legacy" {
        return Err(
            "\"legacy\" is the single `token` key, not a [[tokens]] entry — clear `token` over \
             ssh once every machine has its own named token"
                .into(),
        );
    }
    let (table, mut tokens) = read_tokens(raw)?;
    let before = tokens.len();
    tokens.retain(|t| t.name != name);
    if tokens.len() == before {
        return Err(format!("no token named {:?}", name));
    }
    let text = write_tokens(table, &tokens)?;
    Ok((text, tokens))
}

#[cfg(test)]
mod tests {
    /// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
    /// Hello every client prints names the build, not only the version.
    #[test]
    fn fix_141_the_hello_names_the_build() {
        match super::hello() {
            homelab_proto::ServerMsg::Hello { version, build, .. } => {
                assert_eq!(version, env!("CARGO_PKG_VERSION"));
                assert_eq!(build.as_deref(), option_env!("HOMELAB_BUILD"));
                assert!(build.is_some_and(|b| !b.is_empty()));
            }
            other => panic!("not a Hello: {other:?}"),
        }
    }

    /// covers: fix-94, fix-156
    ///
    /// The home address is read again after every gateway deploy that
    /// finished; another stack's deploy and a failed gateway deploy leave it
    /// alone. (Every night and at the host's start it is read regardless.)
    #[test]
    fn fix_94_the_home_address_follows_gateway_deploys() {
        use super::home_address_after_deploy;
        assert!(home_address_after_deploy(104, true, 104));
        assert!(!home_address_after_deploy(104, false, 104));
        assert!(!home_address_after_deploy(115, true, 104));
    }

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

    /// fix-52 residual (2026-10-01): the 10 s ping only ever proved that its
    /// own loop was still scheduled, not that the scheduler was making any
    /// progress — a nightly round wedged inside one `await` (not a panic,
    /// which the test above already covers) kept being fed forever. Now the
    /// ping is withheld once the scheduler's heartbeat is older than
    /// `SCHEDULER_WATCHDOG_STALE_S`, so systemd's `WatchdogSec` eventually
    /// restarts a daemon whose scheduler stopped making progress.
    #[test]
    fn fix_52_residual_a_stale_scheduler_heartbeat_withholds_the_watchdog_ping() {
        let stale_after = super::SCHEDULER_WATCHDOG_STALE_S;
        // A heartbeat from a few seconds ago: still alive.
        assert!(super::scheduler_is_alive(1_000_000, 999_990, stale_after));
        // Exactly at the edge still counts.
        assert!(super::scheduler_is_alive(
            1_000_000,
            1_000_000 - stale_after,
            stale_after
        ));
        // One second past the edge: the scheduler has made no progress in
        // longer than the tolerance, so the ping must stop.
        assert!(!super::scheduler_is_alive(
            1_000_000,
            1_000_000 - stale_after - 1,
            stale_after
        ));
        // A heartbeat from "the future" (clock skew on a fresh AppState,
        // or `now` ticking backwards) never reads as stale.
        assert!(super::scheduler_is_alive(1_000_000, 1_000_100, stale_after));
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
        // The positive twin: still a real diagnostic line, not an empty one
        // that would also pass "no secret in it".
        assert!(!line.is_empty(), "the frame still gets a log line");
    }

    /// Load a config from `raw` through a file of its own. Tests run as
    /// parallel threads of one process, so neither a shared path nor
    /// `HOMELAB_CONFIG` may be used (rust-code-hygiene, 2026-09-27: one test
    /// set that variable and wrote a fixed /tmp path while others ran).
    fn config_from_text(raw: &str) -> Config {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "homelab-host-test-{}-{}.toml",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, raw).unwrap();
        let cfg = load_config_from(path.display().to_string());
        let _ = std::fs::remove_file(&path);
        cfg
    }

    /// F8: host.toml may only ADD to the no-touch list. Assigning used to be
    /// possible, which meant one line in a hand-edited file could drop VM 100
    /// and VM 101 out of protection without a word.
    #[test]
    fn the_config_can_widen_the_no_touch_list_but_never_shrink_it() {
        let raw = "token = \"0123456789abcdef0123\"\nno_touch = [200, 201]\n";
        let cfg = config_from_text(raw);

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
        let cfg = config_from_text(raw);
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

    /// fix-120: `random_token` makes a 64-hex-char token and never repeats
    /// (a weak source would have shown up as a collision across a thousand
    /// draws).
    #[test]
    fn fix_120_random_token_is_64_hex_and_does_not_repeat() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let t = random_token().unwrap();
            assert_eq!(t.len(), 64, "{}", t);
            assert!(t.bytes().all(|b| b.is_ascii_hexdigit()), "{}", t);
            assert!(seen.insert(t), "random_token repeated");
        }
    }

    /// fix-120: issuing a token adds exactly one `[[tokens]]` entry, whose
    /// SHA-256 is of the plaintext handed in — never the plaintext itself.
    #[test]
    fn fix_120_issue_token_adds_one_entry_hashed_not_plain() {
        let (text, tokens) =
            issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a-fresh-token").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].name, "wsl");
        assert_eq!(tokens[0].scope, homelab_proto::Scope::Operate);
        use sha2::{Digest, Sha256};
        let want: String = Sha256::digest(b"a-fresh-token")
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert_eq!(tokens[0].sha256, want);
        assert!(!text.contains("a-fresh-token"), "{}", text);
        assert!(text.contains(&want), "{}", text);
        // Issuing a second, differently-named token keeps the first.
        let (_, tokens2) =
            issue_token_in(&text, "ct120", homelab_proto::Scope::All, "another-token").unwrap();
        assert_eq!(tokens2.len(), 2);
    }

    /// fix-120: a duplicate name, an empty name and the reserved name
    /// "legacy" are refused rather than silently applied.
    #[test]
    fn fix_120_issue_token_refuses_bad_names() {
        assert!(issue_token_in("", "", homelab_proto::Scope::Read, "x").is_err());
        assert!(issue_token_in("", "legacy", homelab_proto::Scope::Read, "x").is_err());
        let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Read, "x").unwrap();
        assert!(issue_token_in(&text, "wsl", homelab_proto::Scope::All, "y").is_err());
    }

    /// fix-120: revoking removes only the named entry, and leaves the rest
    /// untouched — the whole point of per-machine tokens is that revoking
    /// one never affects another.
    #[test]
    fn fix_120_revoke_token_removes_only_that_one() {
        let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a").unwrap();
        let (text, _) = issue_token_in(&text, "ct120", homelab_proto::Scope::All, "b").unwrap();
        let (text, tokens) = revoke_token_in(&text, "wsl").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].name, "ct120");
        assert!(!text.contains("\"wsl\""), "{}", text);
        // The positive twin: the other token's entry is still there.
        assert!(text.contains("\"ct120\""), "{}", text);
    }

    /// fix-120: revoking a name that is not there, or "legacy" (the single
    /// `token` key, not a `[[tokens]]` entry), is refused.
    #[test]
    fn fix_120_revoke_token_refuses_unknown_and_legacy() {
        assert!(revoke_token_in("", "legacy").is_err());
        assert!(revoke_token_in("", "nobody").is_err());
    }

    /// fix-126: an `https://` notify route with no pin configured is
    /// refused at start, before any notification is ever attempted —
    /// `http://` stays accepted (migration is stepwise).
    #[test]
    fn fix_126_an_https_notify_route_with_no_pin_is_refused_at_start() {
        let raw = "token = \"0123456789abcdef0123\"\nnotify_webhook = \"https://kyu.lan/x\"\n";
        let file: FileConfig = toml::from_str(raw).unwrap();
        let problems = startup_problems(&file);
        assert!(
            problems.iter().any(|p| p.contains("notify_tls_cert")),
            "{:?}",
            problems
        );

        let raw_http = "token = \"0123456789abcdef0123\"\nnotify_webhook = \"http://kyu.lan/x\"\n";
        let file_http: FileConfig = toml::from_str(raw_http).unwrap();
        assert!(
            startup_problems(&file_http)
                .iter()
                .all(|p| !p.contains("notify_tls_cert")),
            "plain http must not need a pin"
        );
    }

    /// fix-126: `pinned_cacert` hands back the path only when the file on
    /// disk hashes to the configured fingerprint — a stale pin (the hub's
    /// certificate was regenerated since) is refused rather than silently
    /// trusting whatever is on disk now, and a plain `http://` url needs no
    /// pin at all.
    #[test]
    fn fix_126_pinned_cacert_checks_the_file_against_the_fingerprint() {
        use sha2::{Digest, Sha256};
        let path = std::env::temp_dir().join(format!(
            "homelab-host-test-cert-{}-{}.pem",
            std::process::id(),
            line!()
        ));
        let pem = "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
        std::fs::write(&path, pem).unwrap();
        let good: String = Sha256::digest(b"hello")
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();

        let raw = format!(
            "token = \"0123456789abcdef0123\"\nnotify_tls_cert = \"{}\"\nnotify_tls_fingerprint = \"{}\"\n",
            path.display(),
            good
        );
        let cfg = config_from_text(&raw);
        assert_eq!(
            pinned_cacert(&cfg, "https://kyu.lan/x").unwrap(),
            Some(path.display().to_string())
        );
        // http:// needs no pin at all, even with one configured.
        assert_eq!(pinned_cacert(&cfg, "http://kyu.lan/x").unwrap(), None);

        let raw_wrong = format!(
            "token = \"0123456789abcdef0123\"\nnotify_tls_cert = \"{}\"\nnotify_tls_fingerprint = \"deadbeef\"\n",
            path.display()
        );
        let cfg_wrong = config_from_text(&raw_wrong);
        assert!(pinned_cacert(&cfg_wrong, "https://kyu.lan/x").is_err());
        let _ = std::fs::remove_file(&path);
    }

    /// fix-143: no `cloudflare_token` is reported as "not configured", not
    /// as a failure — the nightly round must never turn an unasked question
    /// into a finding, same rule `prometheus_url`/`loki_url` already
    /// follow.
    #[tokio::test]
    async fn fix_143_the_nightly_edge_check_reports_not_configured_without_a_token() {
        let dir = std::env::temp_dir().join(format!("homelab-fix143-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        );
        let cfg = config_from_text(&raw);
        let state_dir = cfg.state_dir.clone();
        let app = AppState::new(cfg, tokio::sync::broadcast::channel(16).0);
        let exec = RealExecutor;
        run_nightly_edge_check(&app, &exec, 1_800_000_000).await;
        let store = homelab_core::state::StateStore::new(&exec, &state_dir);
        let s = store.load().await.unwrap();
        assert_eq!(s.last_edge_check, 1_800_000_000);
        assert_eq!(s.last_edge_findings, 0);
        assert_eq!(
            s.last_edge_error.as_deref(),
            Some("not configured (no cloudflare_token in host.toml)")
        );
    }

    /// fix-120: a connection refused for its token used to leave no trace at
    /// all, so a probe from a compromised container or a stolen token tried
    /// from a new machine was invisible. Every refusal is counted with the
    /// address it came from, for `homelab doctor`.
    #[tokio::test]
    async fn fix_120_a_refused_connection_is_counted_with_its_peer() {
        let path = format!("/tmp/homelab-fix120-router-{}.toml", std::process::id());
        std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
        let state = test_state(load_config_from(path.clone()));
        let _ = std::fs::remove_file(&path);
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
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"
tile_watch_source = "10.10.10.20"

[registry_cache]
host = "10.10.10.17"

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
        let t: toml::Table = toml::from_str(raw).unwrap();
        assert!(unknown_keys(&t).is_empty(), "{:?}", unknown_keys(&t));

        let parsed: FileConfig = t.try_into().unwrap();
        assert!(parsed.metrics_targets_dir.is_some());
        assert!(parsed.tile_watch_source.is_some());
    }

    /// A plain typo is loud too — the direction that costs a warning rather
    /// than a silence.
    #[test]
    fn a_typo_is_reported() {
        let t: toml::Table = toml::from_str("metrics_target_dir = \"/x\"\n").unwrap();
        assert_eq!(unknown_keys(&t), vec!["metrics_target_dir".to_string()]);
    }

    /// config-four-representations (expert panel, 2026-09-27): the known
    /// keys were hand-kept lists beside the struct, and they had already
    /// drifted: `KNOWN_RETENTION` named `keep`, which `RetentionTier` has
    /// never read, so a `keep = 3` was ignored without a word. The keys are
    /// now whatever the one struct reads.
    #[test]
    fn a_key_the_struct_does_not_read_is_reported_even_where_a_list_named_it() {
        let raw = "[[retention]]\nevery_days = 1\nspan_days = 7\nkeep = 3\n";
        let t: toml::Table = toml::from_str(raw).unwrap();
        assert_eq!(unknown_keys(&t), vec!["retention[0].keep".to_string()]);
    }

    /// config-four-representations (expert panel, 2026-09-27): a settings
    /// save writes back the file it read, changing only what the settings
    /// tab changed. It used to render a fourth, hand-kept struct from the
    /// resolved config, which wrote the merged no-touch list, the compiled
    /// drill interval and the env token into a file that never said them.
    /// Nested tables write their defaults out (`max_age_hours`,
    /// `pull_timeout_secs`), so the fixture states them.
    ///
    /// covers: F208
    #[test]
    fn a_settings_save_writes_back_the_file_it_read() {
        let raw = r#"
token = "0123456789abcdef0123"
listen = "0.0.0.0:8443"
backup_hour = 4
notify_webhook = "http://ha/webhook/x"
notify_auth_bearer = "a-token-that-must-survive-a-save"
notify_fallback_webhook = "http://10.10.5.101:8123/api/webhook/homelab"
exec_enabled = true
no_touch = [200]
gateway_vmid = 112
restic_base = "rclone:hdd:homelab-backups"
restic_restore_timeout_s = 9999
logs_window = "6h"
ask_timeout_s = 300
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[retention]]
every_days = 1
span_days = 7

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[[watched_backups]]
name = "opnsense-config"
rclone_path = "gdrive:homelab-backups/OPNSense-backups"
max_age_hours = 26

[[device_backups]]
name = "opnsense"
url = "https://10.10.10.1/api/core/backup/download/this"
cred_file = "/var/lib/homelab/secrets/opnsense-backup.conf"
filename = "config.xml"
pin = "sha256//abc"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
        let config = config_from_text(raw);

        let mut settings = config.initial_settings.clone();
        settings.backup_hour = Some(5);
        let rendered = render_settings_toml(&config, &settings).expect("render");

        let mut want: toml::Table = toml::from_str(raw).unwrap();
        want.insert("backup_hour".into(), toml::Value::Integer(5));
        let got: toml::Table = toml::from_str(&rendered).unwrap();
        assert_eq!(got, want, "rendered:\n{rendered}");
    }

    /// fix-134 (config-four-representations): the round trip through
    /// `FileConfig` that used to back a settings save cannot carry a
    /// comment — a struct has no field for one. `config_from_text` deletes
    /// its temp file the moment `load_config_from` returns, so it cannot
    /// prove anything about a later read of the same path; this test keeps
    /// the file on disk across the save, the way a real settings save
    /// finds it.
    ///
    /// covers: fix-134
    #[test]
    fn fix_134_a_settings_save_keeps_the_file_s_own_comments() {
        let raw = "\
# the token nobody is allowed to rotate without telling Kenny first\n\
token = \"0123456789abcdef0123\"\n\
# backed up at 4am because that is when the drive is quietest\n\
backup_hour = 4\n\
\n\
[[retention]]\n\
every_days = 1\n\
span_days = 7\n";

        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "homelab-host-test-fix134-{}-{}.toml",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&path, raw).unwrap();
        let config = load_config_from(path.display().to_string());

        let mut settings = config.initial_settings.clone();
        settings.backup_hour = Some(5);
        let rendered = render_settings_toml(&config, &settings).expect("render");
        let _ = std::fs::remove_file(&path);

        assert!(
            rendered
                .contains("# the token nobody is allowed to rotate without telling Kenny first"),
            "a comment on an untouched key must survive a save:\n{rendered}"
        );
        assert!(
            rendered.contains("# backed up at 4am because that is when the drive is quietest"),
            "a comment on the CHANGED key must survive too — only the value moves:\n{rendered}"
        );
        // And the value changed as asked.
        let got: toml::Table = toml::from_str(&rendered).unwrap();
        assert_eq!(got["backup_hour"].as_integer(), Some(5));
    }

    /// The dashboard's key table and the file the host reads cannot drift:
    /// every field of `FileConfig` has a row (a new field without one fails
    /// here), and every row names a field.
    #[test]
    fn feat_settings_1_every_host_toml_key_is_described_once() {
        let json = serde_json::to_value(FileConfig::default()).unwrap();
        let mut fields: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        // Skipped when absent, so it is not in the default's JSON.
        fields.push("tokens".into());
        fields.sort();
        let mut rows: Vec<String> = homelab_core::hostconfig::KEYS
            .iter()
            .map(|k| k.key.to_string())
            .collect();
        rows.sort();
        assert_eq!(fields, rows);
    }

    /// GetHostConfig: every key the file sets, a secret only as "set", a
    /// scoped token without its hash, the keys the host does not read, and
    /// the file's hash for the write that follows.
    #[test]
    fn feat_settings_1_the_view_hides_secrets_and_hashes() {
        let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nnotify_auth_bearer = \"s3cret-bearer\"\n\
                   loki_url = \"http://10.10.10.13:3100\"\nbogus_key = 1\n\
                   [[tokens]]\nname = \"admin\"\nscope = \"all\"\nsha256 = \"{h}\"\n"
            .replace("{h}", &"a".repeat(64));
        let v = host_config_view("/etc/homelab/host.toml", &raw).unwrap();
        let json = serde_json::to_string(&v).unwrap();
        assert!(!json.contains("0123456789abcdef0123"), "{json}");
        assert!(!json.contains("s3cret-bearer"), "{json}");
        assert!(!json.contains(&"a".repeat(64)), "{json}");
        assert_eq!(v.values["backup_hour"], 4);
        assert_eq!(v.values["tokens"][0]["name"], "admin");
        assert_eq!(v.secrets_set, vec!["notify_auth_bearer", "token"]);
        assert_eq!(v.unknown, vec!["bogus_key"]);
        assert_eq!(v.sha256, homelab_core::manifest::sha256_hex(raw.as_bytes()));
    }

    /// fix-false-alarm (2026-09-30): what to check before a `watch_url`
    /// save is accepted — absent or empty needs no request (turning the
    /// watch off is always allowed), a non-empty value does.
    #[test]
    fn fix_false_alarm_watch_url_to_check() {
        use std::collections::BTreeMap;
        assert_eq!(super::watch_url_to_check(&BTreeMap::new()), None);
        assert_eq!(
            super::watch_url_to_check(&BTreeMap::from([(
                "watch_url".to_string(),
                serde_json::json!("")
            )])),
            None
        );
        assert_eq!(
            super::watch_url_to_check(&BTreeMap::from([(
                "watch_url".to_string(),
                serde_json::json!("  ")
            )])),
            None
        );
        assert_eq!(
            super::watch_url_to_check(&BTreeMap::from([(
                "backup_hour".to_string(),
                serde_json::json!(4)
            )])),
            None,
            "a save of some other key must not run the probe"
        );
        assert_eq!(
            super::watch_url_to_check(&BTreeMap::from([(
                "watch_url".to_string(),
                serde_json::json!("http://10.10.10.20:8080/health")
            )])),
            Some("http://10.10.10.20:8080/health".to_string())
        );
    }

    /// fix-false-alarm: the save is refused with a reason a person can act
    /// on when the host's own request failed, and let through otherwise.
    #[test]
    fn fix_false_alarm_watch_url_refusal() {
        assert_eq!(
            super::watch_url_refusal("http://10.10.10.20:8080/health", true),
            None
        );
        let why = super::watch_url_refusal("http://10.10.10.20:8080/health", false)
            .expect("a failed request refuses the save");
        assert!(why.contains("10.10.10.20:8080/health"), "{why}");
        assert!(why.contains("firewall"), "{why}");
    }

    /// SetHostConfig: a change lands with every other key kept; a file that
    /// moved meanwhile, a locked or ssh-only key, a secret, a stray sub-key
    /// and a value the start-up validation refuses are each refused before
    /// anything is written.
    #[test]
    fn feat_settings_1_a_change_is_checked_like_a_start() {
        use std::collections::BTreeMap;
        let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nexec_enabled = false\n";
        let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
        let one = |k: &str, v: serde_json::Value| BTreeMap::from([(k.to_string(), v)]);

        let (text, saved) = apply_host_config_changes(
            raw,
            &BTreeMap::from([
                ("backup_hour".to_string(), serde_json::json!(5)),
                ("status_interval_s".to_string(), serde_json::json!(30)),
            ]),
            &sha,
        )
        .unwrap();
        let t: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(t["backup_hour"].as_integer(), Some(5));
        assert_eq!(t["status_interval_s"].as_integer(), Some(30));
        assert_eq!(t["token"].as_str(), Some("0123456789abcdef0123"));
        assert_eq!(saved.live, vec!["backup_hour"]);
        assert_eq!(saved.restart, vec!["status_interval_s"]);
        assert_eq!(
            saved.sha256,
            homelab_core::manifest::sha256_hex(text.as_bytes())
        );

        // null removes the key: the host takes its default.
        let (text, _) =
            apply_host_config_changes(raw, &one("backup_hour", serde_json::Value::Null), &sha)
                .unwrap();
        assert!(!text.contains("backup_hour"), "{text}");
        // The positive twin: the rest of the file is still there, not an
        // empty file that would also pass "no backup_hour".
        assert!(text.contains("token"), "{text}");

        let refused = |changes: BTreeMap<String, serde_json::Value>, expect: &str| {
            apply_host_config_changes(raw, &changes, expect).unwrap_err()
        };
        assert!(
            refused(one("backup_hour", serde_json::json!(5)), "stale").contains("changed since")
        );
        for key in [
            "listen",
            "state_dir",
            "tokens",
            "token",
            "exec_enabled",
            "no_touch",
        ] {
            let e = refused(one(key, serde_json::json!("x")), &sha);
            assert!(e.contains("ssh"), "{key}: {e}");
        }
        assert!(refused(one("backup_hour", serde_json::json!(24)), &sha).contains("0 to 23"));
        assert!(refused(one("status_interval_s", serde_json::json!(5)), &sha).contains("10"));
        let e = refused(
            one(
                "registry_cache",
                serde_json::json!({"host": "10.10.10.17", "upstreams": [], "hots": 1}),
            ),
            &sha,
        );
        assert!(e.contains("registry_cache.hots"), "{e}");
        let e = refused(
            one(
                "zfs_jobs",
                serde_json::json!([{"source": "HDD2TB", "target": "HDD2TB"}]),
            ),
            &sha,
        );
        assert!(e.contains("zfs_jobs"), "{e}");
        assert!(refused(one("nonsense", serde_json::json!(1)), &sha).contains("not a setting"));
    }

    /// fix-122 (AR15's runtime debug toggle, Kenny's go 2026-10-01): a
    /// directive `EnvFilter` cannot parse is refused the same way an
    /// out-of-range `backup_hour` is — at the save, not discovered on the
    /// next restart when the daemon refuses to start at all.
    #[test]
    fn fix_122_an_unparsable_log_level_is_refused() {
        let raw = "token = \"0123456789abcdef0123\"\n";
        let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
        let changes = std::collections::BTreeMap::from([(
            "log_level".to_string(),
            serde_json::json!("not a directive!!"),
        )]);
        let e = apply_host_config_changes(raw, &changes, &sha).unwrap_err();
        assert!(e.contains("log_level"), "{e}");
        assert!(e.contains("EnvFilter"), "{e}");
    }

    /// fix-122: a valid directive saves and is live (G8), the same way
    /// `backup_hour` is — `saved.live` names it, not `saved.restart`.
    #[test]
    fn fix_122_a_valid_log_level_saves_as_a_live_key() {
        let raw = "token = \"0123456789abcdef0123\"\n";
        let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
        let changes = std::collections::BTreeMap::from([(
            "log_level".to_string(),
            serde_json::json!("debug"),
        )]);
        let (text, saved) = apply_host_config_changes(raw, &changes, &sha).unwrap();
        let t: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(t["log_level"].as_str(), Some("debug"));
        assert_eq!(saved.live, vec!["log_level"]);
    }

    use super::*;

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
        // Present = Proxmox holds a configuration for it (what `pct status`
        // answered), read off pmxcfs.
        exec.seed_file("/etc/pve/lxc/108.conf", "hostname: 108-app-synctest\n");
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
        let probes = gather_probes(
            &exec,
            "/var/lib/homelab",
            None,
            None,
            homelab_core::ops::restoredrill::DEFAULT_DRILL_SCRATCH_DIR,
            now,
            &|_| {},
        )
        .await;
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

    /// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the new
    /// doctor probes read the machine: file modes, privileged containers
    /// (never a no-touch guest's configuration), the host-meta and drill
    /// records in state, the password file and Drive's space.
    #[tokio::test]
    async fn fix_130_the_new_doctor_probes_read_the_machine() {
        use homelab_core::executor::{CmdOutput, MockExecutor};
        let now = 1_800_000_000u64;
        let exec = MockExecutor::new();
        exec.seed_file(
            "/var/lib/homelab/state.json",
            &format!(
                r#"{{"schema_version":1,"stacks":{{}},"last_host_meta":{},
                "last_restore_drill":{},"restore_drills":{{"kyu-config":{{"last_attempt":{},"last_pass":0,"last_error":"the restore itself failed"}}}}}}"#,
                now - 30 * 3600,
                now - 10 * 86_400,
                now - 86_400
            ),
        );
        exec.respond_always(
            "stat -c",
            CmdOutput::ok("644 /etc/homelab/host.toml\n600 /var/lib/homelab/tls-key.pem\n700 /var/lib/homelab/incidents\n"),
        );
        exec.respond_always("unprivileged: 1", CmdOutput::ok("105\n106\n108\n"));
        exec.respond_always("test -s", CmdOutput::ok(""));
        exec.respond_always("listremotes", CmdOutput::ok("gdrive:\n"));
        exec.respond_always(
            "about gdrive:",
            CmdOutput::ok(r#"{"total":107374182400,"used":90000000000,"trashed":2147483648,"free":17374182400}"#),
        );
        exec.respond_always(
            "ls -1A '/opt/traefik-config/routes'",
            CmdOutput::ok("104-app-gateway.yml\nmanual-leftover.yml\n"),
        );
        let pc = ProbeContext {
            listen: "0.0.0.0:8443".into(),
            exec_enabled: false,
            config_path: "/etc/homelab/host.toml".into(),
            password_file: "/var/lib/homelab/secrets/restic.pass".into(),
            privileged_vmids: vec![105, 106],
            no_touch: vec![100, 101, 102, 103],
            drill_interval_s: 90 * 86_400,
            state_dir: "/var/lib/homelab".into(),
            gateway_vmid: 104,
            gateway_routes_dir: "/opt/traefik-config/routes".into(),
        };
        // As `gather_probes` leaves it when the gdrive remote exists.
        let mut probes = homelab_core::doctor::Probes {
            offsite_configured: true,
            ..Default::default()
        };
        gather_security_probes(&exec, &pc, now, &mut probes).await;
        assert_eq!(
            probes.loose_files,
            Some(vec!["/etc/homelab/host.toml (644)".to_string()])
        );
        let pv = probes.privileged.expect("privileged probed");
        assert_eq!(
            (pv.vmids, pv.outside_policy),
            (vec![105, 106, 108], vec![108])
        );
        let listing = exec.calls_containing("unprivileged: 1");
        assert!(
            listing.iter().all(|c| c.contains("100|101|102|103")),
            "no-touch configurations are skipped, not read: {:?}",
            listing
        );
        assert_eq!(probes.host_meta.and_then(|h| h.age_h), Some(30));
        let drill = probes.restore_drill.expect("drill probed");
        assert_eq!(drill.age_h, Some(240));
        assert!(
            drill.failing[0].starts_with("kyu-config"),
            "{:?}",
            drill.failing
        );
        assert_eq!(probes.password_file_ok, Some(true));
        let drive = probes.drive.expect("drive probed");
        assert_eq!((drive.total, drive.trashed), (107374182400, 2147483648));
        assert_eq!(
            probes.exposure.map(|e| e.listen),
            Some("0.0.0.0:8443".into())
        );
        // fix-130 (second half): no stack is recorded in this state, so
        // every file the gateway lists is unowned.
        assert_eq!(
            probes.unowned_route_files,
            Some(vec![
                "104-app-gateway.yml".to_string(),
                "manual-leftover.yml".to_string()
            ])
        );
    }

    /// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
    /// `homelab incidents` listed names only; reading a bundle took a root
    /// shell on pve. `incidents show <name>` brings the error, the versions
    /// and the end of the transcript to the workstation, and a name that is
    /// not a plain bundle name is refused before it becomes a path.
    #[tokio::test]
    async fn fix_131_incident_show_reads_one_bundle_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("homelab-fix131-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bundle = dir.join("incidents/1800000000-deploy-media");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("report.json"),
            r#"{"op":"deploy-media","steps":[],"ok":false,"error":{"what":"pull images failed","why":"image not found","remedy":"check the tag"}}"#,
        )
        .unwrap();
        std::fs::write(bundle.join("versions.txt"), "host=3.60.0\nproto=1\n").unwrap();
        // Written the way `write_bundle` writes them.
        let line = |m: &str| {
            let ev = PipelineEvent::Line {
                level: homelab_core::sink::Level::Info,
                source: "HOST".into(),
                msg: m.into(),
            };
            format!("{}\n", serde_json::to_string(&ev).unwrap())
        };
        std::fs::write(
            bundle.join("events.jsonl"),
            format!(
                "{}{}",
                line("[run ] docker compose pull"),
                line("image not found")
            ),
        )
        .unwrap();
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
        let show = |name: &str| RpcRequest {
            id: 3,
            command: Rpc::IncidentShow { name: name.into() },
        };
        let got = handle_rpc(&state, show("1800000000-deploy-media")).await;
        let refused = handle_rpc(&state, show("../../etc")).await;
        let missing = handle_rpc(&state, show("1800000001-deploy-x")).await;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(got.ok, "{}", got.message);
        for want in [
            "pull images failed",
            "image not found",
            "check the tag",
            "host=3.60.0",
            "docker compose pull",
        ] {
            assert!(
                got.message.contains(want),
                "{} missing from:\n{}",
                want,
                got.message
            );
        }
        assert!(
            !refused.ok && refused.message.contains("not a bundle name"),
            "{}",
            refused.message
        );
        assert!(!missing.ok, "{}", missing.message);
    }

    /// fix-131: the pruner removes what `bundles_to_prune` names, on disk.
    #[test]
    fn fix_131_old_bundles_are_removed_from_disk() {
        let dir = std::env::temp_dir().join(format!("homelab-fix131p-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let now = 1_800_000_000u64;
        let old = dir.join(format!("incidents/{}-deploy-media", now - 100 * 86_400));
        let young = dir.join(format!("incidents/{}-deploy-kyu", now - 86_400));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&young).unwrap();
        std::fs::write(old.join("report.json"), "{}").unwrap();
        let removed = prune_incidents(
            &dir.to_string_lossy(),
            now,
            homelab_core::incidents::BUNDLE_MAX_AGE_DAYS,
            homelab_core::incidents::BUNDLE_MAX_COUNT,
        );
        let (old_gone, young_kept) = (!old.exists(), young.exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(removed, 1);
        assert!(old_gone && young_kept);
    }

    /// rule-20: `cleanup_push_staging` removes a `push-staging-*` file it
    /// finds on disk (the age judgement itself is
    /// `util::stale_push_staging_tests`, pure) and leaves anything else in
    /// the state dir alone.
    #[test]
    fn rule_20_orphaned_push_staging_files_are_removed_from_disk() {
        let dir = std::env::temp_dir().join(format!("homelab-rule20-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let staging = dir.join("push-staging-118-abcd1234");
        let other = dir.join("state.json");
        std::fs::write(&staging, "stale compose content").unwrap();
        std::fs::write(&other, "{}").unwrap();
        // max_age_s is baked into the call via `now`, well past any file's
        // mtime — the threshold itself is covered in core's pure tests.
        let removed = cleanup_push_staging(&dir.to_string_lossy(), u64::MAX / 2);
        let (staging_gone, other_kept) = (!staging.exists(), other.exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(removed, 1);
        assert!(staging_gone && other_kept);
    }

    /// gap-26: `journal_max_bytes` used to be the fixed constant
    /// `incidents::JOURNAL_MAX_BYTES`, with no host.toml key and no
    /// dashboard row — unlike the incident-bundle limits right beside it.
    /// Absent, the default still applies; set, host.toml wins.
    #[test]
    fn gap_26_journal_max_bytes_defaults_and_follows_host_toml() {
        let cfg = config_from_text("token = \"0123456789abcdef0123\"\n");
        assert_eq!(
            cfg.journal_max_bytes,
            homelab_core::incidents::JOURNAL_MAX_BYTES as u64
        );
        let cfg = config_from_text("token = \"0123456789abcdef0123\"\njournal_max_bytes = 65536\n");
        assert_eq!(cfg.journal_max_bytes, 65536);
    }

    /// gap-26: `compact_journal_file` honours the configured limit rather
    /// than the hardcoded default — a small `max_bytes` cuts a journal the
    /// default would have left alone.
    #[test]
    fn gap_26_compact_journal_file_uses_the_configured_limit() {
        let dir = std::env::temp_dir().join(format!("homelab-gap26-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let line = "{\"op\":\"deploy\",\"at\":1,\"running\":false}\n";
        let content = line.repeat(2000); // well under JOURNAL_MAX_BYTES, well over 64 KiB
        assert!(content.len() < homelab_core::incidents::JOURNAL_MAX_BYTES);
        let path = dir.join("journal.jsonl");
        std::fs::write(&path, &content).unwrap();
        compact_journal_file(&dir.to_string_lossy(), 65536);
        let cut = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            cut.len() < content.len(),
            "a 64 KiB configured limit must cut a journal the 4 MiB default would not: \
             {} -> {}",
            content.len(),
            cut.len()
        );
    }

    /// A session over a real socket on a free local port, with `handler` in
    /// place of `handle_rpc`. Returns the address to connect to.
    async fn serve_on_loopback<H, Fut>(handler: H) -> SocketAddr
    where
        H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
    {
        let config = config_from_text("token = \"0123456789abcdef0123\"\n");
        serve_state_on_loopback(test_state(config), handler).await
    }

    /// [`serve_on_loopback`] around a state the test built itself.
    async fn serve_state_on_loopback<H, Fut>(state: AppState, handler: H) -> SocketAddr
    where
        H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
    {
        let app =
            Router::new()
                .route(
                    "/ws",
                    get(
                        move |ws: WebSocketUpgrade,
                              State(st): State<AppState>,
                              headers: HeaderMap| async move {
                            // The tests that predate arch-tokens send no header
                            // and ran as the one token there was: scope all.
                            let who = identify(
                                headers.get("authorization").and_then(|v| v.to_str().ok()),
                                &st.config.token,
                                &st.config.tokens,
                            )
                            .unwrap_or(Identity {
                                name: "legacy".into(),
                                scope: homelab_proto::Scope::All,
                            });
                            ws.on_upgrade(move |s| serve_ws(s, st, who, handler))
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
                            command: Rpc::Answer {
                                boot: None,
                                id,
                                allow: true,
                            },
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

    /// feat-platform-2: an app's status from a reading, and what stands in
    /// for it before the first reading.
    #[test]
    fn feat_platform_2_app_status_comes_from_the_reading() {
        use homelab_core::ops::livestatus::{AppStatus, LiveStatus};
        assert_eq!(
            live_app(None, 106, "jellyfin"),
            (true, 0),
            "no reading: the old answer"
        );
        let mut r = LiveStatus::default();
        r.apps.entry(106).or_default().insert(
            "jellyfin".into(),
            AppStatus {
                running: false,
                containers: 1,
                restarts: 4,
            },
        );
        assert_eq!(live_app(Some(&r), 106, "jellyfin"), (false, 4));
        assert_eq!(
            live_app(Some(&r), 106, "bazarr"),
            (false, 0),
            "probed and absent: not running"
        );
        assert_eq!(
            live_app(Some(&r), 107, "kuma"),
            (true, 0),
            "guest not probed: unknown"
        );
    }

    /// feat-platform-3: every line an operation prints carries its request
    /// and the time; step starts and ends carry a structured mark; the ring
    /// keeps only the newest lines.
    #[test]
    fn feat_platform_3_lines_carry_request_time_and_step_and_the_ring_is_capped() {
        use homelab_core::sink::{Level, PipelineEvent};
        let (tx, mut rx) = broadcast::channel(16);
        let recent = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        let sink = BroadcastSink {
            log_tx: tx,
            req: Some(7),
            recent: recent.clone(),
            recent_cap: 2,
            timings: std::sync::Mutex::new(Vec::new()),
            subject: std::sync::Mutex::new(None),
            by: None,
        };
        sink.emit(PipelineEvent::StepStarted {
            op: "deploy".into(),
            step: "pull".into(),
        });
        sink.emit(PipelineEvent::Line {
            level: Level::Info,
            source: "HOST".into(),
            msg: "pulling".into(),
        });
        sink.emit(PipelineEvent::StepFinished {
            op: "deploy".into(),
            step: "pull".into(),
            changed: true,
        });
        let first = rx.try_recv().unwrap();
        match first {
            ServerMsg::Log {
                req,
                ts,
                step: Some(mark),
                ..
            } => {
                assert_eq!(req, Some(7));
                assert!(ts.unwrap_or(0) > 1_700_000_000);
                assert_eq!((mark.step.as_str(), mark.finished), ("pull", false));
            }
            other => panic!("{:?}", other),
        }
        match rx.try_recv().unwrap() {
            ServerMsg::Log { req, step, msg, .. } => {
                assert_eq!((req, step, msg.as_str()), (Some(7), None, "pulling"));
            }
            other => panic!("{:?}", other),
        }
        let ring = recent.lock().unwrap();
        assert_eq!(ring.len(), 2, "capped at recent_cap");
        assert!(
            matches!(&ring[1], ServerMsg::Log { step: Some(m), .. } if m.finished && m.changed)
        );
    }

    /// feat-platform-3: CurrentOp answers the holder and the kept lines.
    #[tokio::test]
    async fn feat_platform_3_current_op_answers_the_holder_and_the_newest_lines() {
        let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
        *state.busy.lock().unwrap() = Some(homelab_core::oplock::Holder {
            what: "deploy media".into(),
            started_unix: 1_800_000_000,
            done: 0,
            total: 0,
            stack: Some("media".into()),
        });
        state.recent.lock().unwrap().push_back(ServerMsg::Log {
            level: homelab_proto::LogLevel::Info,
            source: "HOST".into(),
            msg: "step 3".into(),
            req: Some(9),
            ts: Some(1_800_000_010),
            step: None,
            by: None,
        });
        let resp = handle_rpc(
            &state,
            RpcRequest {
                id: 1,
                command: Rpc::CurrentOp,
            },
        )
        .await;
        assert!(resp.ok);
        let view: homelab_proto::CurrentOpView = serde_json::from_str(&resp.message).unwrap();
        assert_eq!(view.holder.as_deref(), Some("deploy media"));
        assert_eq!(view.lines.len(), 1);
    }

    /// feat-platform-1: asked for JSON, the incident list and the manual
    /// checks answer JSON; asked for text, the text stays as it was.
    #[tokio::test]
    async fn feat_platform_1_reports_answer_json_when_asked() {
        let dir = std::env::temp_dir().join(format!("homelab-json-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("incidents/1800000000-deploy-media")).unwrap();
        let state = test_state(config_from_text(&format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        )));
        let r = handle_rpc(
            &state,
            RpcRequest {
                id: 1,
                command: Rpc::Incidents { json: true },
            },
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
        assert_eq!(v["incidents"][0], "1800000000-deploy-media");
        let r = handle_rpc(
            &state,
            RpcRequest {
                id: 2,
                command: Rpc::Incidents { json: false },
            },
        )
        .await;
        assert!(r.message.starts_with("incidents:"), "{}", r.message);
        let r = handle_rpc(
            &state,
            RpcRequest {
                id: 3,
                command: Rpc::ListManualChecks { json: true },
            },
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
        assert!(v["checks"].is_array() && v["now"].is_u64(), "{}", r.message);
    }

    /// arch-history: the sink times each step and remembers the subject.
    #[test]
    fn arch_history_the_sink_times_steps_and_names_the_subject() {
        use homelab_core::sink::PipelineEvent;
        let (tx, _rx) = broadcast::channel(16);
        let sink = BroadcastSink {
            log_tx: tx,
            req: None,
            recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            recent_cap: 10,
            timings: std::sync::Mutex::new(Vec::new()),
            subject: std::sync::Mutex::new(None),
            by: None,
        };
        sink.emit(PipelineEvent::StepStarted {
            op: "deploy media".into(),
            step: "pull".into(),
        });
        sink.emit(PipelineEvent::StepFinished {
            op: "deploy media".into(),
            step: "pull".into(),
            changed: true,
        });
        sink.emit(PipelineEvent::StepStarted {
            op: "deploy media".into(),
            step: "up".into(),
        });
        let t = sink.timings.lock().unwrap().clone();
        assert_eq!(t.len(), 2);
        assert!(t[0].end >= t[0].start && t[0].end > 0 && t[0].changed);
        assert_eq!(t[1].end, 0, "a step still running has no end");
        assert_eq!(
            sink.subject.lock().unwrap().as_deref(),
            Some("deploy media")
        );
    }

    /// arch-history: History answers what history.jsonl holds since a moment.
    #[tokio::test]
    async fn arch_history_the_history_command_reads_since_a_moment() {
        let dir = std::env::temp_dir().join(format!("homelab-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(config_from_text(&format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        )));
        for (start, name) in [(100u64, "old"), (200, "backup")] {
            record_history(
                &state,
                &homelab_core::history::HistoryEntry::Phase {
                    start,
                    end: start + 5,
                    name: name.into(),
                    count: 3,
                },
            );
        }
        let r = handle_rpc(
            &state,
            RpcRequest {
                id: 1,
                command: Rpc::History {
                    since: 150,
                    limit: 10,
                },
            },
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
        assert_eq!(v["entries"].as_array().unwrap().len(), 1, "{}", r.message);
        assert_eq!(v["entries"][0]["name"], "backup");
    }

    /// Decision notify-routing (2026-09-30): every event becomes a notice
    /// the dashboard reads after its cursor; only an urgent one tries the
    /// push, and what became of it is recorded; the sequence survives a
    /// restart.
    #[tokio::test]
    async fn notify_routing_every_event_is_a_notice_and_only_urgent_pushes() {
        let dir = std::env::temp_dir().join(format!("homelab-notices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        );
        let state = test_state(config_from_text(&cfg));
        let report = |op: &str, ok: bool| homelab_core::runner::OperationReport {
            op: op.into(),
            steps: Vec::new(),
            ok,
            error: (!ok).then(|| homelab_core::error::OperatorError {
                what: "restic failed".into(),
                why: "repo locked".into(),
                remedy: "unlock it".into(),
            }),
            deferred: None,
        };
        notify(
            &state,
            &RealExecutor,
            "deploy",
            &report("deploy-media", true),
            90,
            Some(4),
            None,
        )
        .await;
        notify(
            &state,
            &RealExecutor,
            "scheduled-backup",
            &report("backup-home", false),
            95,
            None,
            Some("100-backup-home".into()),
        )
        .await;
        let r = handle_rpc(
            &state,
            RpcRequest {
                id: 1,
                command: Rpc::Notices {
                    after: 0,
                    limit: 10,
                },
            },
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
        let n = v["notices"].as_array().unwrap();
        assert_eq!(n.len(), 2, "{}", r.message);
        assert_eq!(n[0]["push"], "centre only");
        assert_eq!(n[0]["req"], 4);
        assert_eq!(n[0]["page"], "/stacks/media");
        assert_eq!(n[1]["urgent"], true);
        assert!(
            n[1]["push"]
                .as_str()
                .unwrap()
                .starts_with("failed: no notification route"),
            "{}",
            n[1]
        );
        assert_eq!(n[1]["since"], 95);
        assert_eq!(n[1]["incident"], "100-backup-home");
        assert!(n[1]["remedy"]
            .as_str()
            .unwrap()
            .contains("`homelab backup home`"));
        let first = n[0]["seq"].as_u64().unwrap();
        assert_eq!(v["last_seq"], n[1]["seq"]);
        let after = handle_rpc(
            &state,
            RpcRequest {
                id: 2,
                command: Rpc::Notices {
                    after: first,
                    limit: 10,
                },
            },
        )
        .await;
        let v2: serde_json::Value = serde_json::from_str(&after.message).unwrap();
        assert_eq!(v2["notices"].as_array().unwrap().len(), 1);
        // A new start reads the newest seq back and only grows from there.
        let again = test_state(config_from_text(&cfg));
        assert_eq!(
            *again.notice_seq.lock().unwrap(),
            v["last_seq"].as_u64().unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// arch-host-link: an answer stamped with an earlier start of the host
    /// is refused; one stamped with this start, or unstamped (CLI, TUI), is
    /// delivered.
    #[tokio::test]
    async fn arch_host_link_a_stale_answer_does_not_answer_a_new_question() {
        let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
        let pending = |state: &AppState| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            state.pending_asks.lock().unwrap().insert(
                1,
                PendingAsk {
                    reply: tx,
                    ask: ServerMsg::Log {
                        level: homelab_proto::LogLevel::Info,
                        source: "HOST".into(),
                        msg: "question".into(),
                        req: None,
                        ts: None,
                        by: None,
                        step: None,
                    },
                },
            );
            rx
        };
        let _rx = pending(&state);
        let stale = handle_rpc(
            &state,
            RpcRequest {
                id: 1,
                command: Rpc::Answer {
                    id: 1,
                    allow: true,
                    boot: Some("0-1".into()),
                },
            },
        )
        .await;
        assert!(
            !stale.ok && stale.message.contains("earlier start"),
            "{}",
            stale.message
        );
        assert!(
            state.pending_asks.lock().unwrap().contains_key(&1),
            "the question still waits"
        );
        let now = handle_rpc(
            &state,
            RpcRequest {
                id: 2,
                command: Rpc::Answer {
                    id: 1,
                    allow: true,
                    boot: Some(state.boot_id.clone()),
                },
            },
        )
        .await;
        assert!(now.ok, "{}", now.message);
        let _rx = pending(&state);
        let plain = handle_rpc(
            &state,
            RpcRequest {
                id: 3,
                command: Rpc::Answer {
                    id: 1,
                    allow: false,
                    boot: None,
                },
            },
        )
        .await;
        assert!(plain.ok, "{}", plain.message);
    }

    // ── arch-tokens and arch-host-link (homelab-admin, 2026-09-28) ──────

    fn sha_hex(t: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(t.as_bytes())
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// A state with a legacy token and two scoped ones, its audit log in a
    /// directory of its own.
    fn scoped_state(tag: &str) -> (AppState, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("homelab-scope-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n\
             [[tokens]]\nname = \"dash-read\"\nscope = \"read\"\nsha256 = \"{}\"\n\
             [[tokens]]\nname = \"dash-operate\"\nscope = \"operate\"\nsha256 = \"{}\"\n\
             [[tokens]]\nname = \"dash-all\"\nscope = \"all\"\nsha256 = \"{}\"\n",
            dir.display(),
            sha_hex("read-token-aaaaaaaaaaaaaaaa"),
            sha_hex("operate-token-bbbbbbbbbbbbbb"),
            sha_hex("all-token-cccccccccccccccccc"),
        );
        (test_state(config_from_text(&raw)), dir)
    }

    /// Opens a session with `token`, sends `commands` in order and returns
    /// the replies in the order they ARRIVED, as (id, ok, message).
    async fn session_replies(
        addr: SocketAddr,
        token: &str,
        commands: Vec<(u64, Rpc)>,
    ) -> Vec<(u64, bool, String)> {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
        let (ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("connect");
        let (mut tx, mut rx) = ws.split();
        let n = commands.len();
        for (id, command) in commands {
            let req = RpcRequest { id, command };
            tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
                .await
                .unwrap();
        }
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(Ok(frame)) = rx.next().await {
                if let WsMsg::Text(t) = frame {
                    if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t) {
                        out.push((r.id, r.ok, r.message));
                        if out.len() == n {
                            return;
                        }
                    }
                }
            }
        })
        .await;
        out
    }

    /// Answers every command with "ran <name>"; a non-read command takes a
    /// moment, so a read sent after it can overtake it only beside the queue.
    async fn naming_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
        if !req.command.is_read_only() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        RpcResponse {
            id: req.id,
            ok: true,
            message: format!("ran {}", req.command.name()),
            deferred: None,
        }
    }

    /// Answers with the token name the request runs under.
    async fn by_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
        RpcResponse {
            id: req.id,
            ok: true,
            message: requested_by().unwrap_or_else(|| "<none>".into()),
            deferred: None,
        }
    }

    /// milestone act: the token name reaches the handler of a queued
    /// request and of a read beside the queue alike; outside a session
    /// there is none.
    #[tokio::test]
    async fn act_the_token_name_reaches_the_operation_queued_or_beside() {
        let (state, _dir) = scoped_state("by");
        let addr = serve_state_on_loopback(state, by_handler).await;
        let replies = session_replies(
            addr,
            "operate-token-bbbbbbbbbbbbbb",
            vec![
                (
                    1,
                    Rpc::SessionOptions {
                        reads_beside_queue: true,
                    },
                ),
                (2, Rpc::BackupDevices),
                (3, Rpc::GetState),
            ],
        )
        .await;
        for id in [2, 3] {
            let r = replies.iter().find(|r| r.0 == id).expect("a reply");
            assert_eq!(r.2, "dash-operate", "{:?}", replies);
        }
        assert_eq!(requested_by(), None);
    }

    /// feat-platform-10 (milestone follow).
    ///
    /// Over real sockets: the dashboard's session attaches; a `homelab ui`
    /// step from another token reaches it with that token's name and scope,
    /// and the dashboard's answer is the CLI's reply. A read token may read
    /// the state but not drive; a step never waits behind the queue.
    #[tokio::test]
    async fn follow_ui_steps_are_relayed_to_the_attached_dashboard_by_scope() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let (state, _dir) = scoped_state("ui");
        let addr = serve_state_on_loopback(state, naming_handler).await;
        // The dashboard: attaches, then answers every step with what it saw.
        let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
        request.headers_mut().insert(
            "Authorization",
            "Bearer all-token-cccccccccccccccccc".parse().unwrap(),
        );
        let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let (mut tx, mut rx) = ws.split();
        let attach = RpcRequest {
            id: 1,
            command: Rpc::UiAttach,
        };
        tx.send(WsMsg::Text(serde_json::to_string(&attach).unwrap().into()))
            .await
            .unwrap();
        let (attached_tx, attached_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut attached_tx = Some(attached_tx);
            let mut next = 100;
            while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
                match serde_json::from_str::<ServerMsg>(&t) {
                    Ok(ServerMsg::RpcDone(r)) if r.id == 1 => {
                        if let Some(a) = attached_tx.take() {
                            let _ = a.send(());
                        }
                    }
                    Ok(ServerMsg::Ui {
                        relay,
                        by,
                        scope,
                        step,
                    }) => {
                        next += 1;
                        let reply = RpcRequest {
                            id: next,
                            command: Rpc::UiReply {
                                relay,
                                ok: true,
                                message: format!("{} {:?} {}", by, scope, step.verb()),
                            },
                        };
                        tx.send(WsMsg::Text(serde_json::to_string(&reply).unwrap().into()))
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
            }
        });
        attached_rx.await.unwrap();
        let replies = session_replies(
            addr,
            "operate-token-bbbbbbbbbbbbbb",
            vec![
                // A deploy-like command first: the step must not wait for it.
                (1, Rpc::BackupDevices),
                (
                    2,
                    Rpc::Ui {
                        step: homelab_proto::UiStep::Close,
                    },
                ),
            ],
        )
        .await;
        assert_eq!(replies.first().map(|r| r.0), Some(2), "{:?}", replies);
        assert_eq!(replies[0].2, "dash-operate Operate close");
        let read = session_replies(
            addr,
            "read-token-aaaaaaaaaaaaaaaa",
            vec![
                (
                    1,
                    Rpc::Ui {
                        step: homelab_proto::UiStep::State,
                    },
                ),
                (
                    2,
                    Rpc::Ui {
                        step: homelab_proto::UiStep::Close,
                    },
                ),
            ],
        )
        .await;
        let state = read.iter().find(|r| r.0 == 1).unwrap();
        assert!(state.1 && state.2 == "dash-read Read state", "{:?}", read);
        let close = read.iter().find(|r| r.0 == 2).unwrap();
        assert!(
            !close.1 && close.2.contains("needs scope Operate"),
            "{:?}",
            read
        );
        // A session that is not the dashboard cannot answer for it.
        let forged = session_replies(
            addr,
            "operate-token-bbbbbbbbbbbbbb",
            vec![(
                1,
                Rpc::UiReply {
                    relay: 1,
                    ok: true,
                    message: "forged".into(),
                },
            )],
        )
        .await;
        assert!(!forged[0].1, "{:?}", forged);
    }

    /// milestone act: every line of an operation carries who asked for it.
    #[test]
    fn act_the_sink_stamps_the_token_name_on_every_line() {
        use homelab_core::sink::PipelineEvent;
        let (tx, mut rx) = broadcast::channel(16);
        let sink = BroadcastSink {
            log_tx: tx,
            req: Some(3),
            recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            recent_cap: 10,
            timings: std::sync::Mutex::new(Vec::new()),
            subject: std::sync::Mutex::new(None),
            by: Some("admin".into()),
        };
        sink.emit(PipelineEvent::StepStarted {
            op: "deploy-media".into(),
            step: "pull".into(),
        });
        sink.emit(PipelineEvent::Line {
            level: homelab_core::sink::Level::Info,
            source: "HOST".into(),
            msg: "pulled".into(),
        });
        for _ in 0..2 {
            match rx.try_recv().unwrap() {
                ServerMsg::Log { by, req, .. } => {
                    assert_eq!((by.as_deref(), req), (Some("admin"), Some(3)))
                }
                other => panic!("{:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn arch_tokens_a_read_token_cannot_operate_and_the_refusal_is_audited() {
        let (state, dir) = scoped_state("read");
        let addr = serve_state_on_loopback(state, naming_handler).await;
        let replies = session_replies(
            addr,
            "read-token-aaaaaaaaaaaaaaaa",
            vec![(1, Rpc::GetState), (2, Rpc::ZfsReplicate)],
        )
        .await;
        assert_eq!(replies.len(), 2, "{:?}", replies);
        let read = replies.iter().find(|r| r.0 == 1).unwrap();
        let op = replies.iter().find(|r| r.0 == 2).unwrap();
        assert!(read.1 && read.2 == "ran get_state", "{:?}", read);
        assert!(
            !op.1,
            "an operate command on a read token is refused: {:?}",
            op
        );
        assert!(
            op.2.contains("refused") && op.2.contains("dash-read"),
            "{:?}",
            op
        );
        let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
        assert!(
            audit.contains("refused token=dash-read") && audit.contains("cmd=zfs_replicate"),
            "{}",
            audit
        );
    }

    #[tokio::test]
    async fn arch_tokens_an_operate_token_cannot_destroy_exec_or_update_the_host() {
        let (state, _dir) = scoped_state("operate");
        let addr = serve_state_on_loopback(state, naming_handler).await;
        let replies = session_replies(
            addr,
            "operate-token-bbbbbbbbbbbbbb",
            vec![
                (1, Rpc::BackupDevices),
                (
                    2,
                    Rpc::ExecIn {
                        vmid: 106,
                        command: "ls".into(),
                    },
                ),
                (
                    3,
                    Rpc::DestroyRecorded {
                        stack: "media".into(),
                        confirm: "media".into(),
                        skip_backup: false,
                    },
                ),
                (
                    4,
                    Rpc::SelfUpdateHost {
                        binary_b64: String::new(),
                    },
                ),
            ],
        )
        .await;
        let by_id = |id: u64| replies.iter().find(|r| r.0 == id).cloned().unwrap();
        assert!(by_id(1).1, "operate may back up: {:?}", replies);
        for id in [2, 3, 4] {
            assert!(
                !by_id(id).1 && by_id(id).2.contains("refused"),
                "{:?}",
                by_id(id)
            );
        }
    }

    #[tokio::test]
    async fn arch_tokens_a_scope_all_command_by_a_named_token_is_audited_before_it_runs() {
        let (state, dir) = scoped_state("all");
        let addr = serve_state_on_loopback(state, naming_handler).await;
        let replies = session_replies(
            addr,
            "all-token-cccccccccccccccccc",
            vec![(
                1,
                Rpc::ForgetStack {
                    stack: "drill".into(),
                },
            )],
        )
        .await;
        assert!(replies[0].1, "{:?}", replies);
        let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
        assert!(
            audit.contains("scope-all token=dash-all") && audit.contains("cmd=forget_stack"),
            "{}",
            audit
        );
    }

    #[tokio::test]
    async fn arch_tokens_an_unknown_token_opens_no_session() {
        let (state, _dir) = scoped_state("unknown");
        assert_eq!(
            identify(
                Some("Bearer not-a-token-at-all"),
                &state.config.token,
                &state.config.tokens
            ),
            None
        );
        assert_eq!(
            identify(
                Some("Bearer read-token-aaaaaaaaaaaaaaaa"),
                &state.config.token,
                &state.config.tokens
            ),
            Some(Identity {
                name: "dash-read".into(),
                scope: homelab_proto::Scope::Read
            })
        );
        assert_eq!(
            identify(
                Some("Bearer 0123456789abcdef0123"),
                &state.config.token,
                &state.config.tokens
            )
            .map(|i| i.scope),
            Some(homelab_proto::Scope::All)
        );
        assert_eq!(
            identify(None, &state.config.token, &state.config.tokens),
            None
        );
    }

    #[test]
    fn arch_tokens_a_bad_token_list_is_refused_at_start() {
        let e = |name: &str, sha: &str| TokenEntry {
            name: name.into(),
            scope: homelab_proto::Scope::Read,
            sha256: sha.into(),
        };
        let good = sha_hex("x");
        assert!(validate_tokens(&[e("a", &good)]).is_ok());
        assert!(
            validate_tokens(&[e("a", &good), e("a", &good)]).is_err(),
            "duplicate names"
        );
        assert!(
            validate_tokens(&[e("legacy", &good)]).is_err(),
            "reserved name"
        );
        assert!(validate_tokens(&[e("a", "abc")]).is_err(), "short digest");
        assert!(
            validate_tokens(&[e("a", &good.to_uppercase())]).is_err(),
            "uppercase digest"
        );
        assert!(validate_tokens(&[e(" ", &good)]).is_err(), "empty name");
    }

    /// arch-host-link: without the option, replies keep the order of the
    /// requests (the TUI depends on it); with it, a read overtakes a slow
    /// command and is told apart by its id.
    #[tokio::test]
    async fn arch_host_link_reads_skip_the_queue_only_for_a_session_that_asks() {
        let (state, _dir) = scoped_state("beside");
        let addr = serve_state_on_loopback(state, naming_handler).await;
        let plain = session_replies(
            addr,
            "all-token-cccccccccccccccccc",
            vec![(1, Rpc::BackupDevices), (2, Rpc::GetState)],
        )
        .await;
        assert_eq!(
            plain.iter().map(|r| r.0).collect::<Vec<_>>(),
            vec![1, 2],
            "{:?}",
            plain
        );
        let asked = session_replies(
            addr,
            "all-token-cccccccccccccccccc",
            vec![
                (
                    1,
                    Rpc::SessionOptions {
                        reads_beside_queue: true,
                    },
                ),
                (2, Rpc::BackupDevices),
                (3, Rpc::GetState),
            ],
        )
        .await;
        assert_eq!(
            asked.iter().map(|r| r.0).collect::<Vec<_>>(),
            vec![1, 3, 2],
            "{:?}",
            asked
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
                    req: None,
                    step: None,
                    ts: None,
                    by: None,
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
                            command: Rpc::Answer {
                                boot: None,
                                id,
                                allow: true,
                            },
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
        let state = test_state(load_config_from(path.clone()));
        let _ = std::fs::remove_file(&path);
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
        assert!(
            !backup_due(4, 7, now - 25 * 3600, now),
            "outside the night window (fix-129)"
        );
        assert!(!backup_due(4, 4, now - 3600, now), "backed up an hour ago");
        assert!(backup_due(4, 4, 0, now), "never backed up");
    }

    /// fix-129 (expert panel, restart-skips-night, 2026-09-27): a backup was
    /// due only while the local hour equalled `backup_hour`, and the first
    /// check came twenty minutes after start. A self-update, crash or power
    /// cut inside that hour cost the whole night: no backups, updates,
    /// host-meta or fleet check. What was not done in the hour is caught up
    /// in the next one, and the first check comes a minute after start.
    #[test]
    fn fix_129_a_night_missed_by_a_restart_is_caught_up_before_morning() {
        let now = 1_800_000_000u64;
        let stale = now - 25 * 3600;
        assert!(
            backup_due(4, 5, stale, now),
            "04:xx lost, caught up at 05:xx"
        );
        assert!(!backup_due(4, 6, stale, now), "not into the day");
        assert!(!backup_due(4, 3, stale, now), "not before the hour");
        assert!(backup_due(23, 0, stale, now), "the window wraps midnight");
        assert!(
            !backup_due(4, 5, now - 3600, now),
            "done in the hour: not again"
        );
        let st = NightlyState {
            last_integrity_check: 0,
            last_second_copy: 0,
            second_copy_configured: Default::default(),
            last_host_meta: stale,
            last_zfs: now,
            last_restore_drill: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            zfs_configured: false,
            devices_configured: false,
        };
        assert_eq!(
            nightly_plan(4, 5, now, &[("a".into(), true, stale)], &st),
            vec![NightlyTask::Stack("a".into()), NightlyTask::HostMeta]
        );
        assert!(
            Duration::from_secs(SCHEDULER_FIRST_CHECK_S) <= Duration::from_secs(60),
            "the first look comes soon after a start, not a tick later"
        );
    }

    /// G14: the drill rides the backup hour and only when it is due.
    #[test]
    fn g14_the_restore_drill_is_planned_only_in_the_backup_hour_and_only_when_due() {
        let never = NightlyState {
            last_host_meta: 0,
            last_zfs: 0,
            last_restore_drill: 0,
            last_integrity_check: 0,
            restore_drill_interval_s: 90 * 24 * 3600,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        };
        assert!(
            nightly_plan(4, 4, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
            "never drilled, and it is the hour: it must be planned"
        );
        assert!(
            !nightly_plan(4, 7, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
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

    /// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): the
    /// second copy runs once a night when configured, the rotating restic
    /// check once a night either way, both in the backup hour.
    #[test]
    fn fix96_the_second_copy_and_the_check_are_planned_once_a_night() {
        let now = 1_000_000;
        let due = NightlyState {
            last_host_meta: now,
            last_zfs: 0,
            last_restore_drill: now,
            last_integrity_check: 0,
            restore_drill_interval_s: 90 * 24 * 3600,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: true,
            last_second_copy: 0,
        };
        let plan = nightly_plan(4, 4, now, &[], &due);
        assert!(plan.contains(&NightlyTask::SecondCopy), "{:?}", plan);
        assert!(plan.contains(&NightlyTask::IntegrityCheck), "{:?}", plan);
        assert!(
            // fix-129: the night is backup_hour plus a catch-up hour, so
            // "outside" starts two hours later.
            nightly_plan(4, 7, now, &[], &due).is_empty(),
            "outside the two-hour night nothing runs"
        );
        let unconfigured = NightlyState {
            second_copy_configured: false,
            ..due
        };
        let plan = nightly_plan(4, 4, now, &[], &unconfigured);
        assert!(!plan.contains(&NightlyTask::SecondCopy));
        assert!(
            plan.contains(&NightlyTask::IntegrityCheck),
            "the Google Drive copy is checked with or without a second one"
        );
        let done = NightlyState {
            last_second_copy: now - 3600,
            last_integrity_check: now - 3600,
            ..due
        };
        assert!(nightly_plan(4, 4, now, &[], &done).is_empty());
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: fresh,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
            },
        );
        assert!(plan.is_empty());

        // Outside the night window (fix-129: 04:00-06:00): nothing at all.
        assert!(nightly_plan(
            4,
            7,
            now,
            &[("a".into(), true, stale)],
            &NightlyState {
                last_restore_drill: now,
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: stale,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: stale,
                zfs_configured: true,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: stale,
                last_zfs: now - 3600,
                zfs_configured: true,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
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
                home_address_whitelist: None,
                tiles: Default::default(),
                log_files: Vec::new(),
                registry_login: None,
                retention: None,
                data_mounts: Vec::new(),
                native_only: false,
                on_demand: false,
                syslog_receivers: vec![],
                firewall: None,
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
                    timezone: "host".into(),
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
                applied_source: None,
                vmid: 108,
                hostname: "108-app-a".into(),
                apps: vec![],
                applied_at: 0,
                last_backup: 0,
                applied_hash: String::new(),
                manifest: Some(mk(1024)),
                enabled: true,
                natives: Vec::new(),
                incomplete_step: None,
                route_file: None,
                extra_route_files: Vec::new(),
                pushed_file_hashes: std::collections::BTreeMap::new(),
            },
        );
        hs.stacks.insert(
            "b".into(),
            homelab_core::state::StackState {
                applied_source: None,
                vmid: 109,
                hostname: "109-app-b".into(),
                apps: vec![],
                applied_at: 0,
                last_backup: 0,
                applied_hash: String::new(),
                manifest: Some(mk(4096)),
                enabled: true,
                natives: Vec::new(),
                incomplete_step: None,
                route_file: None,
                extra_route_files: Vec::new(),
                pushed_file_hashes: std::collections::BTreeMap::new(),
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
                    // this call created for the child it spawned. The one
                    // exception to the workspace's `unsafe_code = "deny"`.
                    #[allow(unsafe_code)]
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
    /// feat-platform-3: the request this operation runs for, stamped on
    /// every line; None for work nobody asked for over the line.
    req: Option<u64>,
    /// feat-platform-3: the newest lines, for `CurrentOp`.
    recent: Arc<std::sync::Mutex<std::collections::VecDeque<ServerMsg>>>,
    recent_cap: usize,
    /// arch-history: this operation's steps with their times, and what its
    /// first step said it was about.
    timings: std::sync::Mutex<Vec<homelab_core::history::StepTiming>>,
    subject: std::sync::Mutex<Option<String>>,
    /// milestone act: the token name of the session that asked for this
    /// operation, stamped on every line; None for the host's own work.
    by: Option<String>,
}

tokio::task_local! {
    /// milestone act (homelab-admin, 2026-09-28): the token name of the
    /// session whose request is being handled. Set by `serve_ws` around the
    /// handler, read where an operation's sink is built, so the name reaches
    /// every line and the history entry without a new handler parameter.
    static REQUESTED_BY: String;
}

/// The token name of the session this task handles a request for, if any.
fn requested_by() -> Option<String> {
    REQUESTED_BY.try_with(|n| n.clone()).ok()
}

impl Sink for BroadcastSink {
    fn emit(&self, event: PipelineEvent) {
        match &event {
            PipelineEvent::StepStarted { op, step } => {
                if let Ok(mut subject) = self.subject.lock() {
                    subject.get_or_insert_with(|| op.clone());
                }
                if let Ok(mut t) = self.timings.lock() {
                    t.push(homelab_core::history::StepTiming {
                        step: step.clone(),
                        start: unix_now(),
                        end: 0,
                        changed: false,
                    });
                }
            }
            PipelineEvent::StepFinished { step, changed, .. } => {
                if let Ok(mut t) = self.timings.lock() {
                    if let Some(open) = t.iter_mut().rev().find(|x| x.end == 0 && &x.step == step) {
                        open.end = unix_now();
                        open.changed = *changed;
                    }
                }
            }
            _ => {}
        }
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
                    req: self.req,
                    ts: Some(unix_now()),
                    step: None,
                    by: self.by.clone(),
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
                    req: self.req,
                    ts: Some(unix_now()),
                    step: Some(homelab_proto::StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished: false,
                        changed: false,
                    }),
                    by: self.by.clone(),
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
                    req: self.req,
                    ts: Some(unix_now()),
                    step: Some(homelab_proto::StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished: true,
                        changed,
                    }),
                    by: self.by.clone(),
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
        if matches!(msg, ServerMsg::Log { .. }) {
            if let Ok(mut ring) = self.recent.lock() {
                ring.push_back(msg.clone());
                while ring.len() > self.recent_cap {
                    ring.pop_front();
                }
            }
        }
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
    /// fix-104 (long-silences, 2026-09-27): what holds `op_lock`, so a
    /// command that has to wait is told what for, at once.
    busy: Arc<std::sync::Mutex<Option<homelab_core::oplock::Holder>>>,
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
    /// feat-platform-2: the newest reading of every container's real status.
    live_status: Arc<std::sync::RwLock<Option<homelab_core::ops::livestatus::LiveStatus>>>,
    /// feat-platform-3: the newest operation lines, for `CurrentOp`.
    recent: Arc<std::sync::Mutex<std::collections::VecDeque<ServerMsg>>>,
    /// arch-host-link: this start of the host, stamped on every question.
    boot_id: String,
    /// fix-121: when this daemon started. A self-update marker armed before
    /// this moment names this binary as the new one; one armed later was
    /// armed by this daemon for its successor.
    started_at: u64,
    /// feat-platform-10: `homelab ui` steps, handed to the dashboard.
    ui: Arc<ui_relay::UiRelay>,
    /// Decision notify-routing: the newest notice's `seq`, read back from
    /// notices.jsonl at start so it only grows.
    notice_seq: Arc<std::sync::Mutex<u64>>,
    /// fix-120: the `[[tokens]]` list in force right now. Starts as
    /// `config.tokens` (what host.toml said at start) and is overwritten in
    /// place by `TokenIssue`/`TokenRevoke`, so a newly issued token works at
    /// once — host.toml marks `tokens` `Apply::Restart` for the generic
    /// settings path, but this dedicated one does not wait for a restart.
    tokens: Arc<std::sync::RwLock<Vec<TokenEntry>>>,
    /// fix-52 residual: unix seconds of the scheduler's last sign of
    /// progress — touched when a tick wakes and after each stack's night
    /// work. The watchdog feeder refuses to send `WATCHDOG=1` once this
    /// goes stale, so a scheduler wedged inside a single await (not a
    /// panic, which `supervise()` already catches) is still noticed.
    scheduler_heartbeat: Arc<std::sync::atomic::AtomicU64>,
    /// fix-122: the runtime debug toggle's live end — reloading this swaps
    /// the `EnvFilter` both the journald sink and the JSONL ring read from,
    /// with no restart. A throwaway handle outside `main()` (every test, and
    /// anything built before `init_production_logging` runs): reloading it
    /// changes nothing, because nothing reads from it.
    log_filter: LogFilterHandle,
}

/// fix-122: the subscriber's filter, reloadable from `Rpc::SetHostConfig`
/// and `Rpc::ApplyHostConfig` without a restart.
type LogFilterHandle =
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>;

/// fix-122: a handle usable nowhere — not wired to any global subscriber —
/// for `AppState`s that will never have logging live (every test, and
/// `AppState::new` before `main` attaches the real one).
fn inert_log_filter_handle() -> LogFilterHandle {
    tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new("info")).1
}

/// fix-52 residual: how long the scheduler may go without a heartbeat touch
/// before the watchdog feeder stops pinging systemd. Idle ticks are 20
/// minutes apart (`SCHEDULER_FIRST_CHECK_S` then `20 * 60`) and a single
/// stack's backup or update can legitimately run long, so this has to clear
/// both with margin — it is not a tight liveness check, it is "a hung
/// nightly round is eventually noticed" rather than "never".
const SCHEDULER_WATCHDOG_STALE_S: u64 = 3600;

/// fix-52 residual: the decision the watchdog-feeder loop makes every tick.
/// Pure so it is unit-tested without a real systemd socket.
fn scheduler_is_alive(now: u64, heartbeat: u64, stale_after_s: u64) -> bool {
    now.saturating_sub(heartbeat) <= stale_after_s
}

impl AppState {
    fn new(config: Config, log_tx: broadcast::Sender<ServerMsg>) -> Self {
        let last_seq = std::fs::read_to_string(notices_path(&config.state_dir))
            .map(|t| {
                homelab_core::notify::parse_notices(&t)
                    .iter()
                    .map(|n| n.seq)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        AppState {
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            settings: Arc::new(std::sync::RwLock::new(config.initial_settings.clone())),
            tokens: Arc::new(std::sync::RwLock::new(config.tokens.clone())),
            config,
            log_tx,
            op_lock: Arc::new(Mutex::new(())),
            busy: Arc::new(std::sync::Mutex::new(None)),
            pending_asks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            next_ask_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            ui: Arc::new(ui_relay::UiRelay::default()),
            notice_seq: Arc::new(std::sync::Mutex::new(last_seq)),
            damper: Arc::new(std::sync::Mutex::new(
                homelab_core::notify::NotifyDamper::new(20 * 3600),
            )),
            auth_failures: Arc::new(AuthFailures::default()),
            live_status: Arc::new(std::sync::RwLock::new(None)),
            recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            boot_id: format!(
                "{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                std::process::id()
            ),
            // fix-52 residual: fresh as of construction, so the watchdog
            // feeder does not see a stale scheduler before it has had its
            // first chance to tick.
            scheduler_heartbeat: Arc::new(std::sync::atomic::AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            )),
            log_filter: inert_log_filter_handle(),
        }
    }

    /// fix-122: attaches the real, globally-wired filter handle
    /// `init_production_logging` returned — only `main` calls this; every
    /// test keeps the inert one from `new`, which is exactly as capable as
    /// no handle at all, since no test installs a global subscriber for it
    /// to reach.
    fn with_log_filter(mut self, handle: LogFilterHandle) -> Self {
        self.log_filter = handle;
        self
    }

    /// fix-120: a snapshot of the tokens in force right now, for the
    /// runtime paths that authenticate a connection — never `config.tokens`
    /// directly, which stays frozen at what host.toml said at start.
    fn live_tokens(&self) -> Vec<TokenEntry> {
        self.tokens
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// fix-52 residual: stamp the scheduler's heartbeat with the current time.
fn touch_scheduler_heartbeat(state: &AppState) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state
        .scheduler_heartbeat
        .store(now, std::sync::atomic::Ordering::Relaxed);
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
    // arch-tokens: any known token may read the version, scope read included.
    if identify(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
        &state.live_tokens(),
    )
    .is_some()
    {
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
            boot: Some(self.state.boot_id.clone()),
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
///
/// `main` builds its own subscriber (`init_production_logging`, below) —
/// journald plus the JSONL ring, one reloadable filter for both. This one
/// stays test-only: it pins the ANSI-free, span-carrying shape of a single
/// journald line without the ring's file I/O getting in the way.
#[cfg(test)]
fn journal_subscriber<W>(
    writer: W,
    default_filter: &str,
) -> impl tracing::Subscriber + Send + Sync + use<W>
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

/// fix-122 (AR15's JSONL ring + runtime debug toggle, Kenny's go
/// 2026-10-01, `main`'s own subscriber — `journal_subscriber` above stays
/// as it was, for the test that pins its ANSI-free, span-carrying shape):
/// journald (no ANSI, same as `journal_subscriber`) plus a size-capped
/// JSONL ring under `<state_dir>/logs/host.jsonl` (`RingWriter`), behind
/// one `EnvFilter` both sinks share — reloadable at runtime
/// (`Rpc::SetHostConfig`/`Rpc::ApplyHostConfig`'s `log_level`) through the
/// `LogFilterHandle` this returns, with no restart.
///
/// `RUST_LOG` wins when set, exactly as `journal_subscriber` already did;
/// `initial_level` (`config.log_level`, host.toml's own) is the fallback.
fn init_production_logging(
    state_dir: &str,
    initial_level: &str,
    ring_max_bytes: u64,
) -> LogFilterHandle {
    use tracing_subscriber::prelude::*;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| initial_level.into());
    let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);

    let journal_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(std::io::stderr);

    let ring_path = std::path::Path::new(state_dir)
        .join("logs")
        .join("host.jsonl");
    let ring_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .json()
        .with_writer(RingWriter::new(ring_path, ring_max_bytes));

    tracing_subscriber::registry()
        .with(filter)
        .with(journal_layer)
        .with(ring_layer)
        .init();
    handle
}

/// fix-122: writes to `logs/host.jsonl`, cutting the file back to half its
/// cap — the same shape `compact_journal_file` cuts `journal.jsonl` in —
/// once a write leaves it over `max_bytes`. Reopened on every write rather
/// than held open: the daemon's own log volume is small (a home fleet, not
/// a datacentre), and this way a rotated or removed file is simply
/// recreated on the next line instead of silently writing nowhere.
#[derive(Clone)]
struct RingWriter {
    path: std::path::PathBuf,
    max_bytes: u64,
}

impl RingWriter {
    fn new(path: std::path::PathBuf, max_bytes: u64) -> Self {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        RingWriter { path, max_bytes }
    }
}

impl std::io::Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(buf)?;
        let over = f
            .metadata()
            .map(|m| m.len() > self.max_bytes)
            .unwrap_or(false);
        drop(f);
        if over {
            compact_log_ring_file(&self.path, self.max_bytes);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RingWriter {
    type Writer = RingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// fix-122: `compact_journal_file`'s shape, for `logs/host.jsonl` instead
/// of `journal.jsonl` — same atomic write-then-rename, same 0600, no
/// "interrupted operation" to pin (`logring::compact_ring` keeps only the
/// newest lines, nothing more).
fn compact_log_ring_file(path: &std::path::Path, max_bytes: u64) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Some(cut) = homelab_core::logring::compact_ring(&content, max_bytes as usize) else {
        return;
    };
    let tmp = path.with_extension("jsonl.compact.tmp");
    let written = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(cut.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("{}: could not compact :: {}", path.display(), e);
    }
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
        | Rpc::GetApplied { stack }
        | Rpc::GetBackups { stack }
        | Rpc::RestoreNative { stack, .. }
        | Rpc::RevealSecret { stack, .. }
        | Rpc::SetSecret { stack, .. } => Some(stack.clone()),
        Rpc::InstallNative { manifest, .. }
        | Rpc::InstallNativeRelease { manifest, .. }
        | Rpc::AdoptService(manifest) => Some(manifest.stack_name.clone()),
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
        // `--selfcheck` stays the bare version: the self-update reads it.
        Some("--selfcheck") if args.len() == 1 => {
            println!("{}", VERSION);
            std::process::exit(0);
        }
        // fix-141: a person asking also learns which tree it was built from.
        Some("--version") if args.len() == 1 => {
            println!("{} ({})", VERSION, BUILD);
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

    // fix-122: config loads first now, so the subscriber can start with
    // `state_dir` (the ring's home) and `log_level` (its seed filter)
    // already known, rather than the bootstrap "info"-to-stderr-only that
    // used to run before it. `load_config` never logs — a parse failure or
    // a refused value goes to stderr directly and exits — so nothing is
    // lost by moving it ahead of the subscriber.
    let config = load_config();
    let log_filter = init_production_logging(
        &config.state_dir,
        &config.log_level,
        config.log_ring_max_bytes,
    );

    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState::new(config.clone(), log_tx).with_log_filter(log_filter);

    // fix-125: records written before the private modes existed.
    tighten_private_paths(&config.state_dir);
    // fix-131: bound the daemon's own records before anything runs, so
    // nothing writes the journal while it is cut.
    compact_journal_file(&config.state_dir, config.journal_max_bytes);
    prune_incidents(
        &config.state_dir,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        config.incident_bundle_max_age_days,
        config.incident_bundle_max_count,
    );
    // rule-20: the previous run's orphaned push-staging files, if any.
    cleanup_push_staging(
        &config.state_dir,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );

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

    // F3: boot notification — after a power cut or crash-restart, the
    // dashboard's centre hears that the daemon is back and whether anything
    // was left mid-flight. Delayed so the network is up.
    {
        let boot_state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let err = if interrupted.is_empty() {
                None
            } else {
                Some(format!("interrupted: {}", interrupted.join("; ")))
            };
            // Decision notify-routing (2026-09-30): a notice in the
            // dashboard's centre; pushed only when work was interrupted
            // (push-edge, 2026-09-30).
            let ok = interrupted.is_empty();
            let ex = homelab_core::notify::explain_event("host-online", "boot", ok, err.as_deref());
            publish_notice(
                &boot_state,
                &RealExecutor,
                NoticeFacts {
                    op: "host-online".into(),
                    label: "boot".into(),
                    ok,
                    deferred: false,
                    since: boot_state.started_at,
                    ex,
                    urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Boot {
                        interrupted: !ok,
                    }),
                    incident: None,
                    req: None,
                    by: None,
                    findings: Vec::new(),
                },
            )
            .await;
        });
    }

    // feat-platform-2: read every container's real status on a timer, so
    // GetState answers from the newest reading instead of fixed values.
    {
        let st = state.clone();
        tokio::spawn(async move { status_loop(st).await });
    }
    // replace-kuma (Kenny, 2026-09-30): the host watches the dashboard, as
    // the dashboard watches the host.
    if let Some(url) = state.config.watch_url.clone() {
        let st = state.clone();
        tokio::spawn(async move { watch_dashboard(st, url).await });
    }
    // fix-156: read the house's address once at start, so a restart (a
    // release, a power cut) does not leave the dashboard's second lock and
    // CrowdSec's whitelist without it until the night.
    {
        let st = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            run_mutating_op(&st, &RealExecutor, 0, "home-address-whitelist", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await },
                )
            })
            .await;
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
    // fix-52 residual: taken before `state` moves into `app_router`.
    let scheduler_heartbeat = state.scheduler_heartbeat.clone();

    let app = app_router(state);

    // A4: TLS with a self-signed cert; the client pins this fingerprint.
    // fix-128: a clear line instead of a panic when even a new pair cannot be
    // made or loaded (a full or read-only disk); systemd restarts and the
    // journal says why.
    let (certs, fingerprint) = match tls::ensure_cert(&config.state_dir, "homelab-host") {
        Ok(pair) => pair,
        Err(e) => {
            error!(
                "FATAL: cannot make or read the TLS certificate in {} :: {} — check that the \
                 directory is writable and the disk is not full",
                config.state_dir, e
            );
            std::process::exit(1);
        }
    };
    info!(
        "homelab-host v{} ({}) listening on {} (TLS)",
        VERSION, BUILD, config.listen
    );
    info!("TLS fingerprint SHA256:{}", fingerprint);
    let tls_config =
        match axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.cert_pem, &certs.key_pem)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "FATAL: {} and {} do not load as a TLS pair :: {} — move both aside and \
                 restart; the daemon makes a new pair (clients must then re-pin)",
                    certs.cert_pem, certs.key_pem, e
                );
                std::process::exit(1);
            }
        };

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
    //
    // fix-52 residual: a ping every 10 s used to prove only that THIS loop
    // was still scheduled, which says nothing about the scheduler — a
    // nightly round wedged inside one `await` (not a panic; `supervise()`
    // already turns a panic or a dead task into exit 1) kept being fed
    // forever. Now the ping is withheld once the scheduler's own heartbeat
    // goes stale, so systemd's `WatchdogSec` eventually restarts a daemon
    // whose scheduler stopped making progress, not just one whose process
    // stopped existing.
    sd_notify("READY=1");
    {
        let heartbeat = scheduler_heartbeat;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let last = heartbeat.load(std::sync::atomic::Ordering::Relaxed);
                if scheduler_is_alive(now, last, SCHEDULER_WATCHDOG_STALE_S) {
                    sd_notify("WATCHDOG=1");
                } else {
                    tracing::error!(
                        "scheduler heartbeat is {} s old — withholding WATCHDOG=1 so systemd \
                         restarts a daemon whose nightly round has stalled",
                        now.saturating_sub(last)
                    );
                }
            }
        });
    }

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

/// fix-129 (expert panel, restart-skips-night, 2026-09-27): seconds from
/// start to the scheduler's first look. It waited a whole tick (20 min), so
/// a restart late in the backup hour missed the hour altogether. A minute
/// leaves the boot notification and the network their head start.
const SCHEDULER_FIRST_CHECK_S: u64 = 60;

/// fix-129: how many hours the nightly window lasts, starting at
/// `backup_hour`. The second hour is a catch-up: a daemon restarted (a
/// self-update, a crash, a power cut) in the first hour picks up there what
/// is still due, instead of skipping the night. With `backup_hour = 4` the
/// window ends at 06:00, before the house wakes up.
const NIGHT_WINDOW_HOURS: u8 = 2;

/// fix-129: is `local_hour` inside the nightly window that opens at
/// `cfg_hour`? Wraps midnight.
fn in_night_window(cfg_hour: u8, local_hour: u8) -> bool {
    (local_hour + 24 - cfg_hour) % 24 < NIGHT_WINDOW_HOURS
}

fn backup_due(cfg_hour: u8, local_hour: u8, last_backup: u64, now: u64) -> bool {
    // fix-129: due and not yet run since the last window, anywhere in the
    // window; it was only in the configured hour itself.
    in_night_window(cfg_hour, local_hour) && now.saturating_sub(last_backup) >= 20 * 3600
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
    /// fix-96: `restic copy` of every repository into the second repository
    /// set, after the backups and before the ZFS replication carries the
    /// pool to HDD18TB.
    SecondCopy,
    /// fix-96: `restic check` of one repository, both copies, in turn.
    IntegrityCheck,
}

/// fix-94, fix-156: after a gateway deploy, whether to read the house's
/// address again. Only a deploy that finished: a failed one may have left
/// CrowdSec half-started, and reloading it then helps nobody. The address
/// comes from a public service (fix-156), not the router.
fn home_address_after_deploy(vmid: u16, ok: bool, gateway_vmid: u16) -> bool {
    ok && vmid == gateway_vmid
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
    /// fix-96: whether a second copy is configured, when it last ran, and
    /// when the rotating check last ran.
    second_copy_configured: bool,
    last_second_copy: u64,
    last_integrity_check: u64,
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
    // G14: at most one per round, and only in the nightly window (fix-129)
    // — a restore pulls a whole snapshot back over the same link the
    // backups just used.
    if in_night_window(cfg_hour, local_hour)
        && homelab_core::ops::restoredrill::due(
            st.last_restore_drill,
            now,
            st.restore_drill_interval_s,
        )
    {
        plan.push(NightlyTask::RestoreDrill);
    }
    // fix-96: once a night each, in the backup hour.
    if st.second_copy_configured && backup_due(cfg_hour, local_hour, st.last_second_copy, now) {
        plan.push(NightlyTask::SecondCopy);
    }
    if local_hour == cfg_hour
        && homelab_core::ops::restoredrill::due(
            st.last_integrity_check,
            now,
            homelab_core::ops::secondcopy::DEFAULT_CHECK_INTERVAL_S,
        )
    {
        plan.push(NightlyTask::IntegrityCheck);
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
    pg: Option<&homelab_core::ops::restoredrill::PostgresCheck>,
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
            // fix-62: every restored SQLite database (found by content, not
            // by name, so no app is named here) must pass its own integrity
            // check — a file restic restored without error can still be a
            // corrupt database. Silent when `sqlite3` is not on the host
            // (an older template): that is a gap in what this drill can
            // prove, not a failed drill.
            let bad_sqlite: Vec<(String, String)> = exec
                .run(&Cmd::new(
                    "sh",
                    &[
                        "-c",
                        &format!(
                            "command -v sqlite3 >/dev/null 2>&1 || exit 0; \
                             find {} -type f | while read -r f; do \
                             magic=$(head -c 16 \"$f\" 2>/dev/null); \
                             case \"$magic\" in \
                             'SQLite format 3'*) \
                             out=$(sqlite3 \"$f\" 'PRAGMA integrity_check;' 2>&1); \
                             [ \"$out\" = ok ] || printf '%s\\t%s\\n' \"$f\" \"$out\" ;; \
                             esac; done",
                            target
                        ),
                    ],
                    600,
                ))
                .await
                .map(|o| {
                    o.stdout
                        .lines()
                        .filter_map(|l| l.split_once('\t'))
                        .map(|(p, why)| (p.to_string(), why.trim().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            homelab_core::ops::restoredrill::with_sqlite_checks(
                homelab_core::ops::restoredrill::with_archives(
                    verdict(count, largest),
                    &unreadable,
                ),
                &bad_sqlite,
            )
        }
    };
    // fix-62: a stack that declares a Postgres dump (`postgres_check_image`
    // on the mount) gets a throwaway Postgres restore check — only when the
    // ordinary checks above already passed, because a throwaway container
    // started against files that are not even readable proves nothing new.
    let outcome = match (pg, &outcome) {
        (Some(pg), Outcome::Passed { .. }) => {
            let ready = run_postgres_drill_check(exec, pg, target).await;
            homelab_core::ops::restoredrill::with_postgres_check(outcome, ready)
        }
        _ => outcome,
    };
    // Always: a drill that leaves a full restore behind fills the disk the
    // backups need.
    let _ = exec.run(&Cmd::new("rm", &["-rf", target], 300)).await;
    outcome
}

/// fix-62: start a throwaway Postgres container, inside the owning stack's
/// own container (it already has docker), against the data the drill just
/// restored, and read whether it reaches "ready to accept connections" —
/// `restic` returning files without error says nothing about whether
/// Postgres can still open them. `None`: the check itself could not run
/// (docker missing, the copy into the container failed) — a gap in what
/// this drill can prove, not a failed drill; `Some(false)` is a real
/// failure.
async fn run_postgres_drill_check(
    exec: &RealExecutor,
    pg: &homelab_core::ops::restoredrill::PostgresCheck,
    target: &str,
) -> Option<bool> {
    use homelab_core::executor::shq;
    const SCRATCH: &str = "/tmp/homelab-pg-drill";
    let src = format!("{}{}", target, pg.host_path);
    // The host side reads the data the drill already restored (no second
    // restic restore) and hands it to the container over tar, the same
    // direction `restore_native`'s safety copy uses the other way.
    let copy = format!(
        "tar -cf - -C {src} . | pct exec {vmid} -- sh -c 'rm -rf {scratch} && mkdir -p \
         {scratch} && tar -xf - -C {scratch}'",
        src = shq(&src),
        vmid = pg.vmid,
        scratch = SCRATCH,
    );
    match exec.run(&Cmd::new("sh", &["-c", &copy], 300)).await {
        Ok(o) if o.success() => {}
        _ => return None,
    }
    // 0: ready. 1: started but never became ready — a real failure. 2 (or
    // anything else): docker itself could not run the image at all.
    let check = format!(
        "docker rm -f homelab-pg-drill >/dev/null 2>&1; \
         cid=$(docker run -d --name homelab-pg-drill -v {scratch}:/var/lib/postgresql/data \
         {image}) || exit 2; \
         ok=0; \
         for i in $(seq 1 30); do \
         docker logs \"$cid\" 2>&1 | grep -q 'database system is ready to accept connections' \
         && ok=1 && break; sleep 1; done; \
         docker rm -f \"$cid\" >/dev/null 2>&1; rm -rf {scratch}; [ \"$ok\" = 1 ]",
        scratch = SCRATCH,
        image = shq(&pg.image),
    );
    match homelab_core::executor::pct_sh(exec, pg.vmid, &check, 60).await {
        Ok(o) if o.success() => Some(true),
        Ok(o) if o.code == 1 => Some(false),
        _ => None,
    }
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
    // Both live in `ops::watched::feeds`, which the recorded watched-backup
    // state uses too.
    watched
        .iter()
        .filter(|w| {
            !devices
                .iter()
                .any(|d| homelab_core::ops::watched::feeds(&w.rclone_path, restic_base, &d.name))
        })
        .map(|w| format!("{} → {}", w.name, w.rclone_path))
        .collect()
}

/// Decision "Fleet check speed": a device backup this suite just made is
/// recorded for every watcher on its repository, so the fleet check knows
/// without listing Google Drive.
async fn record_device_backup(state: &AppState, device: &str) {
    let watchers: Vec<(String, String)> = state
        .config
        .watched_backups
        .iter()
        .map(|w| (w.name.clone(), w.rclone_path.clone()))
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let fed = homelab_core::ops::watched::record_device_backup(
        &RealExecutor,
        &state.config.state_dir,
        &watchers,
        &state.config.backup.restic_base,
        device,
        now,
    )
    .await;
    if !fed.is_empty() {
        info!(
            "device backup {} recorded for watcher(s) {}",
            device,
            fed.join(", ")
        );
    }
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

/// arch-tokens: who is on the other end. The single legacy `token` is scope
/// `all`; a `[[tokens]]` entry matches on the SHA-256 of the presented token,
/// compared with every byte folded in (fix-120) and every entry tried, so
/// neither which entry matched nor where a guess differed shows in the time.
fn identify(header: Option<&str>, legacy: &str, tokens: &[TokenEntry]) -> Option<Identity> {
    use sha2::{Digest, Sha256};
    if bearer_ok(header, legacy) {
        return Some(Identity {
            name: "legacy".into(),
            scope: homelab_proto::Scope::All,
        });
    }
    let presented = header?.strip_prefix("Bearer ")?;
    let digest: String = Sha256::digest(presented.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    let mut found = None;
    for t in tokens {
        let same = t
            .sha256
            .bytes()
            .zip(digest.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
            && t.sha256.len() == digest.len();
        if same && found.is_none() {
            found = Some(Identity {
                name: t.name.clone(),
                scope: t.scope,
            });
        }
    }
    found
}

/// arch-tokens: the audit line for a command refused for its scope, or run
/// under scope `all`. The command's name only, never its payload.
fn scope_audit_line(
    ts: u64,
    who: &Identity,
    command: &homelab_proto::Command,
    refused: bool,
) -> String {
    format!(
        "{} {} token={} scope={:?} cmd={} needs={:?}\n",
        ts,
        if refused { "refused" } else { "scope-all" },
        who.name,
        who.scope,
        command.name(),
        command.scope()
    )
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
    let _guard = lock_ops(state).await;
    let _busy = BusyMark::set(state, "the nightly backup", jobs.len(), None);
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
                BusyMark::step(state);
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
    let end = unix_now();
    record_history(
        state,
        &homelab_core::history::HistoryEntry::Phase {
            start: end.saturating_sub(phase_started.elapsed().as_secs()),
            end,
            name: "backup".into(),
            count: stacks_in_phase,
        },
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

/// arch-history: append one entry to `<state_dir>/history.jsonl` (0600, one
/// write per line), and prune the file when it has grown past its size.
/// Best effort: history must never fail the operation it describes.
fn record_history(state: &AppState, entry: &homelab_core::history::HistoryEntry) {
    let path = format!("{}/history.jsonl", state.config.state_dir);
    if let Err(e) = append_audit(&path, &entry.to_line()) {
        tracing::warn!("history.jsonl :: {}", e);
        return;
    }
    let big = std::fs::metadata(&path)
        .map(|m| m.len() as usize > state.config.history_max_bytes)
        .unwrap_or(false);
    if big {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(kept) = homelab_core::history::prune(
                &text,
                unix_now(),
                state.config.history_max_age_s,
                state.config.history_max_bytes / 2,
            ) {
                let tmp = format!("{}.tmp", path);
                if std::fs::write(&tmp, kept).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
        }
    }
}

/// feat-platform-2: one app's (running, restarts) from a reading. No
/// reading yet, or its guest was not probed: the old answer (running, 0),
/// said as such by `FleetState.status_measured_at` being None. Probed and
/// absent: the app has no container at all, which is not running.
fn live_app(
    reading: Option<&homelab_core::ops::livestatus::LiveStatus>,
    vmid: u16,
    app: &str,
) -> (bool, u32) {
    let Some(apps) = reading.and_then(|r| r.apps.get(&vmid)) else {
        return (true, 0);
    };
    match apps.get(app) {
        Some(a) => (a.running, a.restarts),
        None => (false, 0),
    }
}

/// fix-68: the fleet snapshot `GetState` broadcasts and `Status` now
/// returns directly, built once so neither drifts from the other.
async fn build_fleet_state(state: &AppState, exec: &RealExecutor) -> homelab_proto::FleetState {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
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
    let (_, fingerprint) = tls::ensure_cert(&state.config.state_dir, "homelab-host").unwrap_or((
        tls::CertPaths {
            cert_pem: String::new(),
            key_pem: String::new(),
        },
        "unknown".into(),
    ));
    // feat-platform-2: the newest status reading, when there is one.
    let reading = state.live_status.read().ok().and_then(|r| r.clone());
    let stacks = hs
        .stacks
        .values()
        .map(|s| homelab_proto::StackView {
            applied_source: s.applied_source.clone(),
            name: s
                .hostname
                .rsplit("-app-")
                .next()
                .unwrap_or(&s.hostname)
                .to_string(),
            vmid: s.vmid,
            hostname: s.hostname.clone(),
            // fix-160: a native stack's units are its apps, also on
            // stacks adopted before install-native recorded them.
            apps: s
                .apps
                .iter()
                .chain(
                    s.natives
                        .iter()
                        .map(|n| &n.unit)
                        .filter(|u| !s.apps.contains(u)),
                )
                .map(|a| {
                    let (running, restarts) = live_app(reading.as_ref(), s.vmid, a);
                    homelab_proto::AppView {
                        name: a.clone(),
                        running,
                        restarts,
                    }
                })
                .collect(),
            drift: false, // computed client-side from applied_hash
            applied_hash: s.applied_hash.clone(),
            env_sealed: true,
            online: reading
                .as_ref()
                .and_then(|r| r.guests.get(&s.vmid))
                .map(|g| g.running)
                .unwrap_or(true),
            enabled: s.enabled,
            usage: reading
                .as_ref()
                .and_then(|r| r.guests.get(&s.vmid))
                .map(|g| homelab_proto::GuestUsage {
                    cpu_permille: g.cpu_permille,
                    ram_used_mb: g.mem_used_mb,
                    ram_max_mb: g.mem_max_mb,
                    uptime_s: g.uptime_s,
                }),
        })
        .collect();
    homelab_proto::FleetState {
        status_measured_at: reading.as_ref().map(|r| r.measured_at),
        host: homelab_proto::HostView {
            home_address: hs.home_address.clone(),
            name: "pve-01".into(),
            cpu_pct: 0,
            // feat-platform-2: was a fixed 0; used over total, both
            // from `free -m` (C6).
            ram_pct: if cap.0 > 0 {
                u64::from(cap.1) * 100 / u64::from(cap.0)
            } else {
                0
            },
            disk_pct: df,
            tls_fingerprint: fingerprint,
            ram_total_mb: cap.0,
            ram_used_mb: cap.1,
            ram_committed_mb: cap.2,
            cores_total: cap.3,
            load1_x100: cap.4,
        },
        stacks,
    }
}

/// feat-platform-2: one reading every `status_interval_s`, the managed
/// stacks taken from state.json each time so a new stack is read at once.
async fn status_loop(state: AppState) {
    let exec = RealExecutor;
    loop {
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let hs = store.load().await.unwrap_or_default();
        let targets: Vec<homelab_core::ops::livestatus::Target> = hs
            .stacks
            .iter()
            .map(|(name, s)| homelab_core::ops::livestatus::Target {
                vmid: s.vmid,
                stack: name.clone(),
                units: s.natives.iter().map(|n| n.unit.clone()).collect(),
            })
            .collect();
        let reading = homelab_core::ops::livestatus::read(&exec, &targets, unix_now()).await;
        for (vmid, why) in &reading.probe_errors {
            tracing::debug!(vmid, "status reading :: {}", why);
        }
        if let Ok(mut slot) = state.live_status.write() {
            *slot = Some(reading);
        }
        tokio::time::sleep(Duration::from_secs(state.config.status_interval_s)).await;
    }
}

async fn scheduler_loop(state: AppState) {
    // fix-122: the nightly updates run inside a span naming their stack.
    use tracing::Instrument as _;
    let exec = RealExecutor;
    // fix-129: the first look soon after start, then every 20 minutes.
    let mut wait = Duration::from_secs(SCHEDULER_FIRST_CHECK_S);
    loop {
        tokio::time::sleep(wait).await;
        wait = Duration::from_secs(20 * 60);
        // fix-52 residual: the loop woke up on its own, which is the one
        // thing an `await` stuck forever cannot do.
        touch_scheduler_heartbeat(&state);
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
                                   // password-chain-bus-factor: the standing checks exist whether or
                                   // not nightly backups are scheduled — a disabled scheduler must not
                                   // also silence the one question that is on a clock rather than a
                                   // deploy.
        {
            let tick_now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            record_state(&store, "ensure standing checks", |s| {
                homelab_core::ops::manualchecks::ensure_standing_checks(s, tick_now)
            })
            .await;
        }
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
        if !in_night_window(hour, local_hour) {
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
                        // fix-115: only units that have a repository.
                        homelab_core::ops::restoredrill::backed_up_units(&st.natives),
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
        // fix-62: every stack's vmid and mounts, for `postgres_check` to
        // find whether tonight's repository is a Postgres data directory —
        // also read before the loop below consumes `snapshot.stacks`.
        let pg_stacks: Vec<(u16, Vec<homelab_core::manifest::MountSpec>, String)> = snapshot
            .stacks
            .iter()
            .map(|(name, st)| {
                (
                    st.vmid,
                    st.manifest
                        .as_ref()
                        .map(|m| m.storage.clone())
                        .unwrap_or_default(),
                    name.clone(),
                )
            })
            .collect();
        // fix-96: every repository with the retention its source keeps, for
        // the second copy and the rotating restic check.
        let copy_policies = homelab_core::ops::secondcopy::repo_policies(
            &snapshot,
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
            &tiers,
        );
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
                second_copy_configured: state.config.second_copy_dataset.is_some(),
                last_second_copy: snapshot.last_second_copy,
                last_integrity_check: snapshot.last_integrity_check,
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
                // fix-138: the same decision the night tests reach.
                let what = match homelab_core::ops::night::backup_work(st)? {
                    homelab_core::ops::night::BackupWork::Native(n) => BackupWhat::Native(n),
                    homelab_core::ops::night::BackupWork::Compose(m) => BackupWhat::Compose(m),
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
        // fix-52 residual: the batch — which can run for hours — returned.
        touch_scheduler_heartbeat(&state);

        // fix-138 (expert panel, host-monolith-untested, 2026-09-27): what
        // each stack's night does is decided by `ops::night::stack_night`,
        // which the night tests reach; this loop only runs the operations,
        // writes state and logs.
        let updates_parked = snapshot.updates_parked.clone();
        for (name, st) in snapshot.stacks {
            let due = plan.contains(&NightlyTask::Stack(name.clone()));
            let backup = backup_done
                .get(&name)
                .cloned()
                .unwrap_or(NightBackup::Failed);
            let night = homelab_core::ops::night::stack_night(
                &name,
                &st,
                due,
                updates_parked.contains_key(&name),
                &backup,
            );
            for (level, line) in &night.lines {
                match level {
                    homelab_core::sink::Level::Warn | homelab_core::sink::Level::Error => {
                        tracing::warn!("{}", line)
                    }
                    _ => info!("{}", line),
                }
            }
            if night.record_last_backup {
                // Record last_backup so tomorrow's check is accurate.
                record_state(&store, "last_backup", |s| {
                    if let Some(rec) = s.stacks.get_mut(&name) {
                        rec.last_backup = now;
                    }
                })
                .await;
            }
            let mut update_ok = true;
            match night.updates {
                homelab_core::ops::night::UpdateWork::None => {}
                homelab_core::ops::night::UpdateWork::Compose(m) => {
                    let m2 = *m;
                    let r = run_mutating_op(&state, &exec, 0, "scheduled-update", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::update::update(ctx, &m2, None, true).await
                        })
                    })
                    .instrument(stack_span(&name))
                    .await;
                    update_ok = r.ok;
                }
                homelab_core::ops::night::UpdateWork::Native { release, own_cmd } => {
                    let applied = Some(st.applied_at);
                    // B1: the orchestrator's own release update.
                    for native in release {
                        let r =
                            run_mutating_op(&state, &exec, 0, "scheduled-release-update", |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::release_update(ctx, &native).await
                                })
                            })
                            .instrument(stack_span(&name))
                            .await;
                        update_ok &= r.ok;
                    }
                    for native in own_cmd {
                        let r =
                            run_mutating_op(&state, &exec, 0, "scheduled-update-native", |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::update_native(ctx, &native, applied)
                                        .await
                                })
                            })
                            .instrument(stack_span(&name))
                            .await;
                        update_ok &= r.ok;
                    }
                }
            }
            // H8 / fix-59: only a failed update parks, and only the updates;
            // a failed or deferred backup is simply tried again tomorrow.
            if night.settle_park {
                park_after_night(&state, &exec, &store, &name, update_ok, now).await;
            }
            // fix-52 residual: this stack's update work (which can itself
            // run long) finished without the loop ever stalling on it.
            touch_scheduler_heartbeat(&state);
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
                // fix-62: a data pool, not the state dir on pve-root — the
                // drill's own target used to fill the root disk.
                let target = format!(
                    "{}/restore-drill",
                    state.config.restore_drill_scratch_dir.trim_end_matches('/')
                );
                let pg = homelab_core::ops::restoredrill::postgres_check(&pg_stacks, &repo);
                let outcome = run_restore_drill(&exec, &cfg, &repo, &target, pg.as_ref()).await;
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
                } else {
                    record_device_backup(&state, &name).await;
                }
            }
        }

        // fix-94: the house's public address is dynamic; keep CrowdSec's
        // whitelist on the gateway equal to it. Cheap when nothing changed:
        // one read of the file, one GET against the router, no write.
        // fix-156: every night, whatever else the night does.
        run_mutating_op(&state, &exec, 0, "home-address-whitelist", |ctx| {
            Box::pin(async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await })
        })
        .await;

        // fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): every
        // repository copied into the second repository set, after all of
        // tonight's backups and before the ZFS replication below, so the
        // replica on HDD18TB carries tonight's copy too.
        if plan.contains(&NightlyTask::SecondCopy) {
            if let Some(ds) = state.config.second_copy_dataset.clone() {
                let cfg = state.config.backup.clone();
                let repos = copy_policies.clone();
                let report = run_mutating_op(&state, &exec, 0, "second-copy", |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::secondcopy::copy_all(ctx, &cfg, &ds, &repos).await
                    })
                })
                .await;
                if !report.ok {
                    tracing::error!(
                        "scheduler: the second copy did not complete — the repositories it names \
                         exist on Google Drive only tonight"
                    );
                }
            }
        }

        // fix-96: one repository checked per night, both copies; once a month
        // per repository the check also reads a slice of the data. Read after
        // the copy above, which records which repositories have a copy yet.
        if plan.contains(&NightlyTask::IntegrityCheck) {
            if let Ok(fresh) = store.load().await {
                let names: Vec<String> = copy_policies.iter().map(|p| p.repo.clone()).collect();
                if let Some(repo) = homelab_core::ops::secondcopy::pick(&fresh, &names) {
                    let subset = homelab_core::ops::secondcopy::data_subset(
                        &fresh,
                        &repo,
                        now,
                        state.config.integrity_data_read_interval_s,
                    );
                    let local = state
                        .config
                        .second_copy_dataset
                        .clone()
                        .filter(|_| homelab_core::ops::secondcopy::check_local(&fresh, &repo));
                    let cfg = state.config.backup.clone();
                    let report = run_mutating_op(&state, &exec, 0, "restic-check", |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::secondcopy::check_repo(
                                ctx,
                                &cfg,
                                &repo,
                                local.as_deref(),
                                subset,
                            )
                            .await
                        })
                    })
                    .await;
                    if !report.ok {
                        tracing::error!(
                            "scheduler: restic check found a problem — {}",
                            report.message
                        );
                    }
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

        // fix-83 (manual-images-latest-unpinned, 2026-09-27): record the
        // digest every `manual` container runs, and ask each declared
        // upstream for its latest release, so the fleet check below can say
        // when a pinned app has fallen behind. Read-only on the containers;
        // GitHub is asked at most once a night (the answer is cached in
        // state), every call is bounded, and state is loaded again after the
        // reading so a deploy that saved meanwhile is not overwritten.
        if let Ok(snapshot) = store.load().await {
            let (facts, notes) = homelab_core::ops::pins::gather(
                &exec,
                &snapshot,
                &state.config.safety.no_touch,
                now,
                state.config.upstream_max_age_s,
            )
            .await;
            for n in notes {
                info!("{}", n);
            }
            if let Ok(mut s) = store.load().await {
                homelab_core::ops::pins::apply(&mut s, facts);
                let _ = store.save(s).await;
            }
        }

        // fix-143 (Cloudflare nightly comparison, owner decision
        // 2026-10-01): once a night, like the integrity check — this used
        // to run only from the workstation (`homelab check`), so a night
        // nobody ran it went unwatched.
        if let Ok(fresh) = store.load().await {
            if homelab_core::ops::restoredrill::due(fresh.last_edge_check, now, 24 * 3600) {
                run_nightly_edge_check(&state, &exec, now).await;
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
            let mut live = gather_live_facts(&exec, &state, &[], true).await;
            if let Ok(snapshot) = store.load().await {
                // fix-142: the in-container half of the nightly hash
                // comparison — only the host can ask this, since it needs
                // `pct exec`, so it runs here and nowhere else. Only stacks
                // with a recorded push (deployed since this was built) are
                // asked.
                let stacks_with_hashes: Vec<(String, u16)> = snapshot
                    .stacks
                    .iter()
                    .filter(|(_, st)| !st.pushed_file_hashes.is_empty())
                    .map(|(name, st)| (name.clone(), st.vmid))
                    .collect();
                live.container_file_hashes =
                    homelab_core::ops::facts::container_file_hashes(&exec, &stacks_with_hashes)
                        .await;
                let findings = homelab_core::ops::fleetcheck::evaluate(
                    &snapshot,
                    &live,
                    now,
                    state.config.backup_max_age_s,
                    homelab_core::ops::fleetcheck::GrowthLimits::default(),
                    state.config.tile_watch_source.as_deref(),
                    state.config.patch_threshold_s,
                    state.config.host_meta_max_age_s,
                    state.config.capacity_thresholds,
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
                        state.config.nightly_report_repeat_s,
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
                    tracing::warn!("{}", render_findings(&findings));
                    // The finding text is already in the log above; the
                    // notice exists so it leaves the machine. Decision
                    // notify-routing (2026-09-30): the whole report goes to
                    // the dashboard's centre; the phone only when something
                    // is broken (F86: one payload shape for every event).
                    let broken = findings
                        .iter()
                        .filter(|f| f.severity == homelab_core::ops::fleetcheck::Severity::Broken)
                        .count();
                    publish_notice(
                        &state,
                        &exec,
                        NoticeFacts {
                            op: "fleet-check".into(),
                            label: "nightly".into(),
                            ok: false,
                            deferred: false,
                            since: now,
                            ex: homelab_core::notify::explain_fleet_check(&findings),
                            urgency: homelab_core::notify::urgency(
                                &homelab_core::notify::Event::FleetCheck { broken },
                            ),
                            incident: None,
                            req: None,
                            by: None,
                            findings: findings.clone(),
                        },
                    )
                    .await;
                }
            }
        }

        // fix-131: the daemon's own records, bounded every night. Under the
        // operation lock, so no operation appends to the journal while it
        // is cut; off the async workers, since both are plain file work.
        {
            let _guard = state.op_lock.lock().await;
            let dir = state.config.state_dir.clone();
            let max_age_days = state.config.incident_bundle_max_age_days;
            let max_count = state.config.incident_bundle_max_count;
            let journal_max_bytes = state.config.journal_max_bytes;
            let _ = tokio::task::spawn_blocking(move || {
                compact_journal_file(&dir, journal_max_bytes);
                prune_incidents(&dir, now, max_age_days, max_count);
            })
            .await;
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
    let Some(who) = identify(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
        &state.live_tokens(),
    ) else {
        log_refused(&state, peer, "/api/ws");
        return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
    };
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
        .on_upgrade(move |socket| ws_session(socket, state, who))
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

async fn ws_session(socket: WebSocket, state: AppState, who: Identity) {
    serve_ws(socket, state, who, |st, req| async move {
        handle_rpc(&st, req).await
    })
    .await
}

/// The first frame of every session: which host this is.
fn hello() -> ServerMsg {
    ServerMsg::Hello {
        version: VERSION.into(),
        proto: homelab_proto::PROTO_VERSION,
        build: Some(BUILD.into()),
    }
}

/// One client's session: the Hello, the forwarder that carries broadcasts and
/// answers out, and the loop that reads requests. The request handler is a
/// parameter so a test can drive the real session over a real socket with a
/// handler that asks a question (fix-66).
async fn serve_ws<H, Fut>(socket: WebSocket, state: AppState, who: Identity, handler: H)
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let (mut tx, mut rx) = socket.split();
    // feat-platform-10: which session this is, for the UI relay.
    static SESSIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let session = SESSIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let hello = hello();
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
        let by = who.name.clone();
        tokio::spawn(async move {
            use tracing::Instrument as _;
            while let Some(req) = work_rx.recv().await {
                let span = rpc_span(&req);
                let resp = REQUESTED_BY
                    .scope(by.clone(), handler(state.clone(), req).instrument(span))
                    .await;
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                // fix-121: an answered request is what accepts an update.
                accept_pending_update(&state.config.state_dir, state.started_at);
            }
        })
    };

    let mut reads_beside = false;
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
        // arch-tokens: a command above the token's scope is refused here,
        // before it reaches the queue, and the refusal is audited.
        if req.command.scope() > who.scope {
            let audit = format!("{}/audit.log", state.config.state_dir);
            if let Err(e) = append_audit(
                &audit,
                &scope_audit_line(unix_now(), &who, &req.command, true),
            ) {
                tracing::warn!("audit.log :: {}", e);
            }
            tracing::warn!(token = %who.name, cmd = req.command.name(), "refused: outside the token's scope");
            let resp = RpcResponse {
                id: req.id,
                ok: false,
                message: format!(
                    "refused: {} needs scope {:?}; token \"{}\" has scope {:?}",
                    req.command.name(),
                    req.command.scope(),
                    who.name,
                    who.scope
                ),
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        if req.command.scope() == homelab_proto::Scope::All && who.name != "legacy" {
            let audit = format!("{}/audit.log", state.config.state_dir);
            if let Err(e) = append_audit(
                &audit,
                &scope_audit_line(unix_now(), &who, &req.command, false),
            ) {
                tracing::warn!("audit.log :: {}", e);
            }
        }
        // feat-platform-10: the UI relay is the session's own business, like
        // the session options: never queued behind a deploy.
        match req.command {
            Rpc::UiAttach => {
                state.ui.attach(session, &who.name, out_tx.clone());
                info!(token = %who.name, session, "the dashboard attached for UI steps");
                let resp = RpcResponse {
                    id: req.id,
                    ok: true,
                    message: "attached: UI steps come to this session".into(),
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::UiReply {
                relay,
                ok,
                ref message,
            } => {
                let (ok, message) = match state.ui.reply(session, relay, ok, message.clone()) {
                    Ok(()) => (true, "delivered".to_string()),
                    Err(e) => (false, e),
                };
                let resp = RpcResponse {
                    id: req.id,
                    ok,
                    message,
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::UiHold {
                relay,
                wait_s,
                ref note,
            } => {
                let (ok, message) = match state.ui.hold(session, relay, wait_s, note.clone()) {
                    Ok(()) => (true, "held".to_string()),
                    Err(e) => (false, e),
                };
                let resp = RpcResponse {
                    id: req.id,
                    ok,
                    message,
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::Ui { ref step } => {
                info!(token = %who.name, step = step.verb(), "UI step relayed to the dashboard");
                let (out_tx, ui, step) = (out_tx.clone(), state.ui.clone(), step.clone());
                let (by, scope, id) = (who.name.clone(), who.scope, req.id);
                tokio::spawn(async move {
                    // Live view: a note from a held step ("paused by the
                    // viewer …") goes to this CLI alone.
                    let notes = out_tx.clone();
                    let note = move |note: String| {
                        let _ = notes.try_send(ServerMsg::UiNote { note });
                    };
                    let (ok, message) = ui
                        .relay(&by, scope, step, ui_relay::RELAY_WAIT, &note)
                        .await;
                    let resp = RpcResponse {
                        id,
                        ok,
                        message,
                        deferred: None,
                    };
                    let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                });
                continue;
            }
            _ => {}
        }
        // arch-host-link: a session that matches replies by id asks for its
        // reads to skip the queue; the CLI and TUI never ask.
        if let homelab_proto::Command::SessionOptions { reads_beside_queue } = req.command {
            reads_beside = reads_beside_queue;
            let resp = RpcResponse {
                id: req.id,
                ok: true,
                message: format!("reads beside the queue: {}", reads_beside),
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        if runs_beside_the_queue(&req.command) || (reads_beside && req.command.is_read_only()) {
            let (out_tx, state, handler) = (out_tx.clone(), state.clone(), handler.clone());
            let by = who.name.clone();
            tokio::spawn(async move {
                use tracing::Instrument as _;
                let span = rpc_span(&req);
                let resp = REQUESTED_BY
                    .scope(by, handler(state.clone(), req).instrument(span))
                    .await;
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
    // fix-158: the attached dashboard's end hands the steps back to the
    // most recent earlier dashboard still connected, and says which.
    match state.ui.detach(session) {
        ui_relay::Detached::NotAttached => {}
        ui_relay::Detached::FellBackTo(back, token) => info!(
            session,
            back,
            token = %token,
            "the attached dashboard's session ended; UI steps go to the earlier dashboard session {back} again"
        ),
        ui_relay::Detached::NoneLeft => info!(
            session,
            "the attached dashboard's session ended; no dashboard is attached for UI steps"
        ),
    }
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
        req: None,
        step: None,
        ts: None,
        by: None,
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
/// the class of silence this project exists to remove.
///
/// Decision notify-routing (2026-09-30): a notice in the dashboard's centre,
/// and pushed at once (push-edge, 2026-09-30): a parked stack loses its
/// nightly updates until someone enables it again.
/// The op name carries the stack: two stacks parking on the same night are
/// two notices, not one.
async fn notify_auto_disabled(state: &AppState, exec: &RealExecutor, stack: &str, why: &str) {
    let op = format!("stack-disabled-{}", stack);
    let ex = homelab_core::notify::explain_event(&op, stack, false, Some(why));
    publish_notice(
        state,
        exec,
        NoticeFacts {
            label: stack.to_string(),
            ok: false,
            deferred: false,
            since: unix_now(),
            urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Parked),
            incident: None,
            req: None,
            by: None,
            findings: Vec::new(),
            op,
            ex,
        },
    )
    .await;
}

/// replace-kuma: ask the dashboard's health address every minute; five
/// minutes without an answer is an urgent notice (pushed at once, since the
/// dashboard that would show it is the thing that is gone), and its return
/// another.
/// Decision "deploys are known outages" (Kenny, 2026-09-30): true while the
/// admin stack is being deployed, or the host itself is updating or
/// restarting — the dashboard is expected to be briefly unreachable, so the
/// host's own watch of it must not start (or advance) the down timer then.
fn dashboard_outage_expected(state: &AppState) -> bool {
    let Ok(b) = state.busy.lock() else {
        return false;
    };
    b.as_ref().is_some_and(|h| {
        h.stack.as_deref() == Some("admin")
            || matches!(h.what.as_str(), "self-update" | "restart-host")
    })
}

async fn watch_dashboard(state: AppState, url: String) {
    let mut t = tokio::time::interval(Duration::from_secs(state.config.watch_interval_s));
    let mut failing_since: Option<u64> = None;
    let mut told = false;
    loop {
        t.tick().await;
        if dashboard_outage_expected(&state) {
            // A known outage: skip this round entirely so the timer restarts
            // from zero once the deploy, update or restart has ended, same
            // as a tile the dashboard's own watch treats as "deploying".
            failing_since = None;
            continue;
        }
        let out = RealExecutor
            .run(&homelab_core::executor::Cmd::new(
                "curl",
                &["-sf", "-m", "10", "-o", "/dev/null", &url],
                20,
            ))
            .await;
        let ok = out.as_ref().map(|o| o.success()).unwrap_or(false);
        let now = unix_now();
        if ok {
            failing_since = None;
            if std::mem::take(&mut told) {
                publish_notice(&state, &RealExecutor, watch_facts(true, now, &url)).await;
            }
            continue;
        }
        let since = *failing_since.get_or_insert(now);
        if !told && now.saturating_sub(since) >= state.config.watch_down_after_s {
            told = true;
            publish_notice(&state, &RealExecutor, watch_facts(false, since, &url)).await;
        }
    }
}

fn watch_facts(ok: bool, since: u64, url: &str) -> NoticeFacts {
    NoticeFacts {
        op: "watch-dashboard".into(),
        label: "watch".into(),
        ok,
        deferred: false,
        since,
        ex: homelab_core::notify::Explained {
            title: if ok {
                "The dashboard answers again".into()
            } else {
                "The dashboard does not answer".into()
            },
            stack: None,
            what: format!(
                "{} {}",
                url,
                if ok {
                    "answers"
                } else {
                    "gave no answer for five minutes"
                }
            ),
            consequence: if ok {
                "Nothing to do.".into()
            } else {
                "Nothing watches the services every minute while it is gone.".into()
            },
            remedy: if ok {
                "Nothing to do.".into()
            } else {
                "Look at the dashboard's container and its unit from the host.".into()
            },
            page: homelab_core::notify::page::HOST.into(),
        },
        // Urgent both ways: its loss is "a service that does not answer", and
        // its return goes where the loss went.
        urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Alert {
            alertname: homelab_core::notify::SERVICE_DOWN_ALERTS[0],
        }),
        incident: None,
        req: None,
        by: None,
        findings: Vec::new(),
    }
}

/// Decision notify-routing (2026-09-30): what one notice is made of.
struct NoticeFacts {
    op: String,
    label: String,
    ok: bool,
    deferred: bool,
    /// Since when it is so (the operation's start).
    since: u64,
    ex: homelab_core::notify::Explained,
    urgency: homelab_core::notify::Urgency,
    incident: Option<String>,
    req: Option<u64>,
    by: Option<String>,
    /// The nightly check's findings (empty for everything else).
    findings: Vec<homelab_core::ops::fleetcheck::Finding>,
}

/// Where the host keeps its notices for the dashboard (0600, one line each).
fn notices_path(state_dir: &str) -> String {
    format!("{}/notices.jsonl", state_dir)
}

/// Decision notify-routing (Kenny, 2026-09-30): every event becomes a notice
/// the dashboard reads (`Command::Notices`); only an urgent one is also
/// pushed at once, through kyu (fallback: Home Assistant) as before, with
/// the short text and the link to the dashboard page. The damper (H13)
/// still keeps one failure from paging every night; it now judges pushes
/// only, so the centre keeps every occurrence.
async fn publish_notice(state: &AppState, exec: &RealExecutor, f: NoticeFacts) {
    let now = unix_now();
    let push = if !f.urgency.urgent {
        "centre only".to_string()
    } else if !state
        .damper
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .should_send(&f.op, f.ok, Some(&f.ex.what), now)
    {
        "not pushed again: the same failure went out within 20 h".to_string()
    } else {
        let url = homelab_core::notify::click_url(&state.config.dashboard_url, &f.ex.page);
        let short = homelab_core::notify::push_short(&f.ex.title, &f.ex.remedy);
        let payload = homelab_core::notify::push_payload(
            "homelab-host",
            &f.op,
            &f.label,
            f.ok,
            Some(&short),
            VERSION,
            Some(url.as_str()).filter(|u| !u.is_empty()),
        );
        match notify_raw(state, exec, payload).await {
            Ok(()) => "sent".to_string(),
            Err(why) => format!("failed: {}", why),
        }
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = {
        let mut last = state
            .notice_seq
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *last = homelab_core::notify::next_seq(*last, now_ms);
        *last
    };
    let notice = homelab_core::notify::HostNotice {
        seq,
        at: now,
        since: f.since,
        op: f.op,
        label: f.label,
        ok: f.ok,
        deferred: f.deferred,
        stack: f.ex.stack,
        title: f.ex.title,
        what: f.ex.what,
        consequence: f.ex.consequence,
        remedy: f.ex.remedy,
        page: f.ex.page,
        urgent: f.urgency.urgent,
        routed: f.urgency.why.to_string(),
        push,
        incident: f.incident,
        req: f.req,
        by: f.by,
        findings: f.findings,
    };
    record_notice(state, &notice);
}

/// Append one notice (0600) and prune the file by the history's limits.
/// Best effort, like the history: a notice must never fail its operation.
fn record_notice(state: &AppState, n: &homelab_core::notify::HostNotice) {
    let path = notices_path(&state.config.state_dir);
    let line = match serde_json::to_string(n) {
        Ok(l) => l + "\n",
        Err(e) => {
            tracing::warn!("notices.jsonl :: {}", e);
            return;
        }
    };
    if let Err(e) = append_audit(&path, &line) {
        tracing::warn!("notices.jsonl :: {}", e);
        return;
    }
    let big = std::fs::metadata(&path)
        .map(|m| m.len() as usize > state.config.history_max_bytes)
        .unwrap_or(false);
    if big {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(kept) = homelab_core::notify::prune_notices(
                &text,
                unix_now(),
                state.config.history_max_age_s,
                state.config.history_max_bytes / 2,
            ) {
                let tmp = format!("{}.tmp", path);
                if std::fs::write(&tmp, kept).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
        }
    }
}

/// F3: a notice after every mutating operation (see [`publish_notice`]).
/// Never blocks or fails the operation itself.
async fn notify(
    state: &AppState,
    exec: &RealExecutor,
    label: &str,
    report: &homelab_core::runner::OperationReport,
    since: u64,
    req: Option<u64>,
    incident: Option<String>,
) {
    let ex = homelab_core::notify::explain_op(&homelab_core::notify::OpFacts {
        op: &report.op,
        label,
        ok: report.ok,
        deferred: report.deferred.as_deref(),
        error: report.error.as_ref(),
        incident: incident.as_deref(),
    });
    let urgency = homelab_core::notify::urgency(&homelab_core::notify::Event::Op {
        label,
        ok: report.ok,
        deferred: report.deferred.is_some(),
    });
    publish_notice(
        state,
        exec,
        NoticeFacts {
            op: report.op.clone(),
            label: label.to_string(),
            ok: report.ok,
            deferred: report.deferred.is_some(),
            since,
            ex,
            urgency,
            incident,
            req,
            by: requested_by(),
            findings: Vec::new(),
        },
    )
    .await;
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
/// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
/// `--cacert` path for `url`, or the reason it is refused. `Ok(None)` for a
/// plain `http://` route, which needs no pin (migration is stepwise).
/// Re-checked at every send, not only at boot: the certificate file can be
/// rewritten — a regenerated hub certificate — without the host restarting,
/// and a stale pin must stop the send, not weaken into trusting whatever is
/// on disk now.
fn pinned_cacert(cfg: &Config, url: &str) -> Result<Option<String>, String> {
    if !url.starts_with("https://") {
        return Ok(None);
    }
    let path = cfg
        .notify_tls_cert
        .as_deref()
        .ok_or("https:// route with no notify_tls_cert configured")?;
    let want = cfg
        .notify_tls_fingerprint
        .as_deref()
        .ok_or("https:// route with no notify_tls_fingerprint configured")?;
    let pem =
        std::fs::read_to_string(path).map_err(|e| format!("notify_tls_cert {}: {}", path, e))?;
    let got = homelab_core::notify::cert_fingerprint(&pem)
        .map_err(|e| format!("notify_tls_cert {}: {}", path, e))?;
    if got != want {
        return Err(format!(
            "notify_tls_cert {} does not match notify_tls_fingerprint ({} on disk, {} \
             configured) — the hub's certificate changed, or this is the wrong file",
            path, got, want
        ));
    }
    Ok(Some(path.to_string()))
}

async fn notify_raw(state: &AppState, exec: &RealExecutor, payload: String) -> Result<(), String> {
    let primary = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .notify_webhook
        .clone();
    let fallback = state.config.notify_fallback_webhook.clone();
    let urls = homelab_core::notify::route(primary.as_deref(), fallback.as_deref());
    if urls.is_empty() {
        return Err("no notification route is configured (notify_webhook)".into());
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
        // fix-126: the pin is re-checked here, not only at boot — the
        // certificate file can be rewritten (a regenerated hub certificate)
        // without the host restarting.
        let cacert = match pinned_cacert(&state.config, url) {
            Ok(c) => c,
            Err(e) => {
                last = e;
                tracing::warn!(
                    "notification route {} refused: {}",
                    homelab_core::notify::route_for_log(url),
                    last
                );
                continue;
            }
        };
        let owned = homelab_core::notify::curl_args_pinned(
            &payload,
            url,
            header_file.as_deref(),
            cacert.as_deref(),
        );
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
    if delivered {
        Ok(())
    } else {
        Err(last)
    }
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

/// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01): one
/// GET against the Cloudflare API, through the Executor rather than
/// `std::process::Command` directly (unlike `client/src/edge.rs`, which runs
/// on the workstation and is free to shell out itself) and with the token in
/// a 0600 header file rather than on curl's argv (fix-35's own pattern,
/// reused rather than the client's `-K -` on stdin, which `Executor` has no
/// way to feed).
async fn edge_get(
    exec: &RealExecutor,
    header_file: &str,
    path: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", homelab_core::ops::edge::API, path);
    let args = [
        "-sS",
        "--fail",
        "-m",
        "20",
        "-H",
        &format!("@{}", header_file),
        &url,
    ];
    let out = exec
        .run(&Cmd::new("curl", &args, 25))
        .await
        .map_err(|e| format!("GET {}: {}", path, e))?;
    if !out.success() {
        return Err(format!("GET {}: curl exited {}", path, out.code));
    }
    let v: serde_json::Value =
        serde_json::from_str(&out.stdout).map_err(|e| format!("GET {}: {}", path, e))?;
    if v.get("success") != Some(&serde_json::Value::Bool(true)) {
        return Err(format!("GET {}: the API did not answer success", path));
    }
    Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

/// fix-143: the live edge, projected like the capture — the host's own copy
/// of `client/src/edge.rs::fetch_live`.
async fn fetch_live_edge(
    exec: &RealExecutor,
    header_file: &str,
    ids: &homelab_core::ops::edge::EdgeIds,
) -> Result<homelab_core::ops::edge::EdgeState, String> {
    use homelab_core::ops::edge::{project_apps, project_dns, project_tunnel, EdgeState};
    let tunnel = edge_get(
        exec,
        header_file,
        &format!("/accounts/{}/cfd_tunnel/{}", ids.account_id, ids.tunnel_id),
    )
    .await?;
    let config = edge_get(
        exec,
        header_file,
        &format!(
            "/accounts/{}/cfd_tunnel/{}/configurations",
            ids.account_id, ids.tunnel_id
        ),
    )
    .await?;
    let apps = edge_get(
        exec,
        header_file,
        &format!("/accounts/{}/access/apps", ids.account_id),
    )
    .await?;
    let dns = edge_get(
        exec,
        header_file,
        &format!("/zones/{}/dns_records", ids.zone_id),
    )
    .await?;
    Ok(EdgeState {
        tunnels: serde_json::Value::Array(vec![project_tunnel(&tunnel, &config)]),
        apps: project_apps(&apps),
        dns: project_dns(&dns),
    })
}

/// fix-143: the nightly half of the Cloudflare edge comparison — the same
/// `core::ops::edge` logic `homelab check` already runs from the
/// workstation, run here too so a flipped Access app is noticed even on a
/// night nobody ran the CLI. `cloudflare_token` absent is reported as "not
/// configured", never as a failure: an unasked question must never become a
/// finding (the same rule `prometheus_url`/`loki_url` follow).
async fn run_nightly_edge_check(state: &AppState, exec: &RealExecutor, now: u64) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let Some(token) = state.config.cloudflare_token.clone() else {
        record_state(&store, "edge check", |s| {
            s.last_edge_check = now;
            s.last_edge_findings = 0;
            s.last_edge_error = Some("not configured (no cloudflare_token in host.toml)".into());
        })
        .await;
        return;
    };
    let captured_dir = std::path::Path::new(&state.config.state_dir).join("repo/captured/gateway");
    let (findings, error) = match homelab_core::ops::edge::load_capture(&captured_dir) {
        Ok((ids, captured)) => {
            let header_file = format!("{}/secrets/cloudflare.header", state.config.state_dir);
            let header = format!("authorization: Bearer {}\n", token.trim());
            match exec.write_file(&header_file, &header, 0o600).await {
                Ok(()) => match fetch_live_edge(exec, &header_file, &ids).await {
                    Ok(live) => (
                        homelab_core::ops::edge::compare_edge(&captured, &live),
                        None,
                    ),
                    Err(e) => (
                        Vec::new(),
                        Some(format!("the Cloudflare API did not answer: {e}")),
                    ),
                },
                Err(e) => (
                    Vec::new(),
                    Some(format!("cannot write the header file: {e}")),
                ),
            }
        }
        Err(e) => (Vec::new(), Some(format!("the capture does not read: {e}"))),
    };
    if let Some(e) = &error {
        tracing::warn!("edge check: not compared — {}", e);
    } else if !findings.is_empty() {
        tracing::warn!(
            "edge check: {} finding(s) against captured/gateway/ — {}",
            findings.len(),
            homelab_core::ops::fleetcheck::render(&findings)
        );
    } else {
        info!("edge check: Cloudflare agrees with captured/gateway/");
    }
    let count = findings.len();
    record_state(&store, "edge check", |s| {
        s.last_edge_check = now;
        s.last_edge_findings = count;
        s.last_edge_error = error;
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
    run_mutating_op_for_stack(state, exec, req_id, label, None, op).await
}

/// Decision "deploys are known outages" (Kenny, 2026-09-30): the same as
/// [`run_mutating_op`], but names the one stack this operation acts on, so
/// `Rpc::Tiles` can say it is deploying rather than down while this runs.
async fn run_mutating_op_for_stack<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    stack: Option<&str>,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    let _guard = lock_ops(state).await;
    let _busy = BusyMark::set(state, label, 0, stack);
    run_op_locked(state, exec, req_id, label, op).await
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// fix-104 (long-silences, 2026-09-27): take the operation lock, and when it
/// is taken, tell whoever is watching what this command waits for before
/// waiting. A command typed during the nightly batch used to hang with no
/// word for as long as the batch ran.
async fn lock_ops(state: &AppState) -> tokio::sync::MutexGuard<'_, ()> {
    if let Ok(g) = state.op_lock.try_lock() {
        return g;
    }
    let holder = state.busy.lock().ok().and_then(|b| b.clone());
    let msg = homelab_core::oplock::waiting_message(holder.as_ref(), unix_now());
    info!("{}", msg);
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        level: homelab_proto::LogLevel::Warn,
        source: "HOST".into(),
        msg,
    });
    state.op_lock.lock().await
}

/// fix-104: one progress line of a long read (check, doctor, today) to
/// whoever is watching. Nobody connected, nothing sent.
fn progress_line(state: &AppState, line: &str) {
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        level: homelab_proto::LogLevel::Info,
        source: "CHECK".into(),
        msg: line.to_string(),
    });
}

/// fix-104: records what holds the operation lock for as long as it lives.
struct BusyMark<'a> {
    state: &'a AppState,
}

impl<'a> BusyMark<'a> {
    fn set(state: &'a AppState, what: &str, total: usize, stack: Option<&str>) -> Self {
        if let Ok(mut b) = state.busy.lock() {
            *b = Some(homelab_core::oplock::Holder {
                what: what.to_string(),
                started_unix: unix_now(),
                done: 0,
                total,
                stack: stack.map(str::to_string),
            });
        }
        BusyMark { state }
    }

    /// One more item of a batch is finished.
    fn step(state: &AppState) {
        if let Ok(mut b) = state.busy.lock() {
            if let Some(h) = b.as_mut() {
                h.done += 1;
            }
        }
    }
}

impl Drop for BusyMark<'_> {
    fn drop(&mut self) {
        if let Ok(mut b) = self.state.busy.lock() {
            *b = None;
        }
    }
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
        // Work the host starts itself runs with id 0: no request asked.
        req: (req_id != 0).then_some(req_id),
        recent: state.recent.clone(),
        recent_cap: state.config.recent_lines,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: requested_by(),
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
    // tile-watch-watcher-out (found 2026-10-01): the fleet-wide tile-watch
    // targets and the watcher stack itself, read once per op from the same
    // snapshot `deploy`'s own prior-state read uses — a deploy in progress
    // folds its own, fresher manifest in on top of this (`with_fresh_target`),
    // so a load a moment old is never wrong, only momentarily incomplete for
    // the very stack being deployed. A snapshot that fails to load leaves
    // both empty, same as `tile_watch_source` unset: no OUT rule is derived.
    let (tile_watch_targets, tile_watch_watcher) =
        match homelab_core::state::StateStore::new(exec, &state.config.state_dir)
            .load()
            .await
        {
            Ok(snapshot) => (
                homelab_core::ops::tiles::fleet_tile_watch_targets(&snapshot),
                state
                    .config
                    .tile_watch_source
                    .as_deref()
                    .and_then(|s| homelab_core::ops::tiles::tile_watch_watcher(&snapshot, s)),
            ),
            Err(_) => (Vec::new(), None),
        };
    let ctx = OpCtx {
        exec,
        sink: &sink,
        journal: &journal,
        safety: state.config.safety.clone(),
        state_dir: state.config.state_dir.clone(),
        now_unix: now,
        metrics_targets_dir: state.config.metrics_targets_dir.clone(),
        tile_watch_source: state.config.tile_watch_source.clone(),
        tile_watch_targets,
        tile_watch_watcher,
        // C1/C2: the same Loki the coverage check already asks about, so
        // there is not a second address to keep in step with the first.
        loki_url: state.config.loki_url.clone(),
        backup: state.config.backup.clone(),
        registry_cache: state.config.registry_cache.clone(),
        default_log_rotation: state.config.default_log_rotation.clone(),
        asker: &asker,
    };
    let report = op(&ctx).await;
    // arch-history: one line for this operation, whatever its outcome.
    record_history(
        state,
        &homelab_core::history::HistoryEntry::Op {
            start: now,
            end: unix_now(),
            label: label.to_string(),
            subject: broadcast.subject.lock().ok().and_then(|s| s.clone()),
            req: (req_id != 0).then_some(req_id),
            by: broadcast.by.clone(),
            ok: report.ok,
            deferred: report.deferred.clone(),
            error: report.error.as_ref().map(|e| e.what.clone()),
            steps: broadcast
                .timings
                .lock()
                .map(|t| t.clone())
                .unwrap_or_default(),
        },
    );
    // AR14: a failure's incident bundle first, so its notice can name it.
    let bundle = if !report.ok && report.deferred.is_none() {
        let versions = format!("host={}\nproto={}\n", VERSION, homelab_proto::PROTO_VERSION);
        Some(
            homelab_core::incidents::write_bundle(
                exec,
                &state.config.state_dir,
                now,
                &report,
                &sink.events(),
                &versions,
            )
            .await,
        )
    } else {
        None
    };
    let incident = match &bundle {
        Some(Ok(dir)) => dir.rsplit('/').next().map(str::to_string),
        _ => None,
    };
    // F3, best-effort; decision notify-routing: a notice, pushed if urgent.
    notify(
        state,
        exec,
        label,
        &report,
        now,
        (req_id != 0).then_some(req_id),
        incident,
    )
    .await;
    // rule-20: this operation's own leftovers, success or not — a push that
    // failed partway is exactly the case `push_content_staged`'s own cleanup
    // never reaches (that `rm` is only on its success path).
    cleanup_push_staging(&state.config.state_dir, now);
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
        let bundle_note = match bundle {
            Some(Ok(dir)) => format!(" :: incident bundle {}", dir),
            Some(Err(e)) => format!(" :: (bundle write failed: {})", e),
            None => String::new(),
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
    digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
    host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
) -> homelab_core::ops::today::Today {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // The doctor's readings and the fleet check's are independent, so they
    // are taken side by side (ops::pool) rather than one after the other:
    // 27 s + 67 s on pve on 2026-09-29, before either was made concurrent.
    // They share no round: doctor asks each recorded stack whether its
    // container exists and its secret files are sealed; the check asks each
    // container about disk, memory, logs, configuration and updates.
    let doctor_fut = async {
        let mut probes = gather_probes(
            exec,
            &state.config.state_dir,
            state.config.mirror_remote.as_deref(),
            state.config.backup.staging_dir.as_deref(),
            &state.config.restore_drill_scratch_dir,
            now,
            &|line: &str| progress_line(state, line),
        )
        .await;
        probes.failed_auth = Some(state.auth_failures.snapshot());
        gather_security_probes(exec, &ProbeContext::of(&state.config), now, &mut probes).await;
        homelab_core::doctor::diagnose(&probes)
    };
    let (checks, mut live) = futures_util::join!(
        doctor_fut,
        gather_live_facts(exec, state, stack_files, false)
    );
    live.digests = digests;
    // fix-110: as `Rpc::FleetCheck`, for the `homelab today` path.
    live.declared_host_config = host_config;
    live.live_host_config = live_host_config_table(
        &std::fs::read_to_string(&state.config.config_path).unwrap_or_default(),
    );
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
                state.config.backup_max_age_s,
                homelab_core::ops::fleetcheck::GrowthLimits::default(),
                state.config.tile_watch_source.as_deref(),
                state.config.patch_threshold_s,
                state.config.host_meta_max_age_s,
                state.config.capacity_thresholds,
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

/// `watched_fresh`: true lists every watched backup on its remote and records
/// the answer (the nightly round); false reads what the host recorded
/// (`homelab check`, `homelab today`; decision "Fleet check speed").
async fn gather_live_facts(
    exec: &RealExecutor,
    state: &AppState,
    stack_files: &[(String, u16)],
    watched_fresh: bool,
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
        state_dir: state.config.state_dir.clone(),
        gateway_vmid: state.config.safety.gateway_vmid,
        gateway_routes_dir: state.config.safety.gateway_routes_dir.clone(),
        no_touch: state.config.safety.no_touch.to_vec(),
        prometheus_url: state.config.prometheus_url.clone(),
        loki_url: state.config.loki_url.clone(),
        loki_vmid: state.config.loki_vmid,
        logs_window: sane_window(&state.config.logs_window),
        now_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        watched_fresh,
    };
    // fix-104: each phase goes to whoever is watching; the check took 41 s
    // with nothing on the screen after "link up".
    let progress = |line: &str| progress_line(state, line);
    let (mut facts, notes) =
        homelab_core::ops::facts::gather_live_facts_with(exec, &inp, stack_files, &progress).await;
    for n in notes {
        info!("{}", n);
    }
    // fix-96: so the check can say when a configured second copy stops.
    facts.second_copy_dataset = state.config.second_copy_dataset.clone();
    // rule-20: pve's own capacity — not a managed container's rootfs
    // (`growth`) and not a stack's declared data pool (`pools`).
    facts.host_capacity = gather_host_capacity(
        exec,
        state.config.prometheus_url.as_deref(),
        state.config.tsdb_retention_size_mib,
        state.config.backup.staging_dir.as_deref(),
        state.config.backup.staging_cap_mib,
    )
    .await;
    facts
}

/// rule-20 (disk-audit, 2026-10-01): pve's root filesystem, the local-lvm
/// thin pool (data and metadata), every ZFS pool, pve's own journald
/// against its own cap (`hostunits::JOURNALD_CAP`, 2G), and — only when
/// both are configured — Prometheus' TSDB against the cap Kenny declared
/// for it. Every command runs directly on pve (this daemon's own host, not
/// a container); a command that fails or does not parse simply contributes
/// no fact, same as an unasked question.
async fn gather_host_capacity(
    exec: &dyn Executor,
    prometheus_url: Option<&str>,
    tsdb_retention_size_mib: Option<u64>,
    native_backup_staging_dir: Option<&str>,
    native_backup_staging_cap_mib: u64,
) -> Vec<homelab_core::ops::fleetcheck::HostCapacityFact> {
    use homelab_core::ops::fleetcheck::{parse_df_pcent, HostCapacityFact, HostCapacityMetric};
    let mut out = Vec::new();

    if let Ok(o) = exec
        .run(&Cmd::new("df", &["--output=pcent", "/"], 15))
        .await
    {
        if let Some(pct) = parse_df_pcent(&o.stdout) {
            out.push(HostCapacityFact {
                metric: HostCapacityMetric::PveRoot,
                subject: "pve".into(),
                used_pct: pct,
                detail: format!("{}% full", pct),
            });
        }
    }

    // The default Proxmox thin pool: volume group `pve`, logical volume
    // `data`. A host configured with a different pool name is not measured
    // here — that is a fact to add, not a guess to make.
    if let Ok(o) = exec
        .run(&Cmd::new(
            "lvs",
            &[
                "--noheadings",
                "-o",
                "data_percent,metadata_percent",
                "pve/data",
            ],
            15,
        ))
        .await
    {
        if let Some((data, meta)) =
            homelab_core::ops::fleetcheck::parse_thin_pool_percents(&o.stdout)
        {
            out.push(HostCapacityFact {
                metric: HostCapacityMetric::ThinPoolData,
                subject: "local-lvm".into(),
                used_pct: data,
                detail: format!("{}% full (data)", data),
            });
            out.push(HostCapacityFact {
                metric: HostCapacityMetric::ThinPoolMeta,
                subject: "local-lvm".into(),
                used_pct: meta,
                detail: format!("{}% full (metadata)", meta),
            });
        }
    }

    if let Ok(o) = exec
        .run(&Cmd::new(
            "zpool",
            &["list", "-H", "-o", "name,capacity"],
            15,
        ))
        .await
    {
        for (pool, pct) in homelab_core::ops::fleetcheck::parse_zpool_capacities(&o.stdout) {
            out.push(HostCapacityFact {
                metric: HostCapacityMetric::ZfsPool,
                subject: pool.clone(),
                used_pct: pct,
                detail: format!("{}% full", pct),
            });
        }
    }

    if let Ok(o) = exec
        .run(&Cmd::new("journalctl", &["--disk-usage"], 15))
        .await
    {
        if let Some(used_mib) =
            homelab_core::ops::fleetcheck::parse_journal_disk_usage_mib(&o.stdout)
        {
            // hostunits::JOURNALD_CAP: SystemMaxUse=2G.
            let cap_mib: u64 = 2048;
            let pct = ((used_mib * 100) / cap_mib).min(255) as u8;
            out.push(HostCapacityFact {
                metric: HostCapacityMetric::Journald,
                subject: "pve".into(),
                used_pct: pct,
                detail: format!("{} MiB of its {} MiB cap ({}%)", used_mib, cap_mib, pct),
            });
        }
    }

    if let (Some(base), Some(cap_mib)) = (prometheus_url, tsdb_retention_size_mib) {
        let q = format!(
            "{}/api/v1/query?query=sum(prometheus_tsdb_storage_blocks_bytes)",
            base.trim_end_matches('/')
        );
        if let Ok(o) = exec
            .run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
            .await
        {
            if let Some(bytes) = homelab_core::ops::fleetcheck::parse_prometheus_scalar(&o.stdout) {
                let used_mib = (bytes / 1024.0 / 1024.0).round() as u64;
                let pct = ((used_mib * 100) / cap_mib.max(1)).min(255) as u8;
                out.push(HostCapacityFact {
                    metric: HostCapacityMetric::PrometheusTsdb,
                    subject: "Prometheus".into(),
                    used_pct: pct,
                    detail: format!(
                        "{} MiB of its {} MiB retention.size ({}%)",
                        used_mib, cap_mib, pct
                    ),
                });
            }
        }
    }

    // rule-20 (coordinator, 2026-10-01): the native-backup staging directory
    // is emptied after every run (backup.rs, DEFAULT_STAGING_DIR) — so
    // anything found here at all is worth a look well before it could ever
    // fill its own cap, which is why its thresholds are far lower than
    // every other reading above.
    if let Some(dir) = native_backup_staging_dir {
        if let Ok(o) = exec.run(&Cmd::new("du", &["-sm", dir], 30)).await {
            if let Some(used_mib) = homelab_core::ops::fleetcheck::parse_du_sm(&o.stdout) {
                let cap = native_backup_staging_cap_mib.max(1);
                let pct = ((used_mib * 100) / cap).min(255) as u8;
                out.push(HostCapacityFact {
                    metric: HostCapacityMetric::NativeBackupStaging,
                    subject: dir.to_string(),
                    used_pct: pct,
                    detail: format!(
                        "{} MiB of its {} MiB cap ({}%) — it should be empty between runs",
                        used_mib, cap, pct
                    ),
                });
            }
        }
    }

    out
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

/// fix-103: one rendering for `homelab check`, the TUI and the nightly log —
/// a summary first, then the findings grouped by severity.
fn render_findings(findings: &[homelab_core::ops::fleetcheck::Finding]) -> String {
    homelab_core::ops::fleetcheck::render(findings)
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
        // Answered by the session loop itself; reaching here means a caller
        // bypassed it (a test harness), which changes nothing.
        // arch-history: what the host did since a moment.
        Rpc::History { since, limit } => {
            let text = std::fs::read_to_string(format!("{}/history.jsonl", state.config.state_dir))
                .unwrap_or_default();
            let entries =
                homelab_core::history::select(homelab_core::history::parse(&text), since, limit);
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "entries": entries }).to_string(),
                deferred: None,
            }
        }
        // Decision notify-routing: the notices after the dashboard's cursor.
        Rpc::Notices { after, limit } => {
            let text =
                std::fs::read_to_string(notices_path(&state.config.state_dir)).unwrap_or_default();
            let all = homelab_core::notify::parse_notices(&text);
            let last_seq = all.iter().map(|n| n.seq).max().unwrap_or(0);
            let notices = homelab_core::notify::notices_after(all, after, limit.min(1000));
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "notices": notices, "last_seq": last_seq })
                    .to_string(),
                deferred: None,
            }
        }
        // feat-platform-3: what runs now, and the newest lines.
        Rpc::CurrentOp => {
            let holder = state.busy.lock().ok().and_then(|b| b.clone());
            let view = homelab_proto::CurrentOpView {
                holder: holder.as_ref().map(|h| h.what.clone()),
                started_unix: holder.as_ref().map(|h| h.started_unix),
                lines: state
                    .recent
                    .lock()
                    .map(|r| r.iter().cloned().collect())
                    .unwrap_or_default(),
            };
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&view).unwrap_or_default(),
                deferred: None,
            }
        }
        Rpc::SessionOptions { .. } => RpcResponse {
            id: req.id,
            ok: true,
            message: "session options are set per session".into(),
            deferred: None,
        },
        // feat-platform-10: answered by the session loop (`serve_ws`).
        Rpc::Ui { .. } | Rpc::UiAttach | Rpc::UiReply { .. } | Rpc::UiHold { .. } => RpcResponse {
            id: req.id,
            ok: false,
            message: "UI steps are relayed by the session, not run".into(),
            deferred: None,
        },
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
            deferred: None,
        },
        // fix-68: this used to dump raw `pct list` plus the whole of
        // state.json (1,473 lines live, measured 2026-09-27) for the
        // operator to read by eye. It now hands back the same fleet
        // snapshot `homelab ui`/the dashboard use, which the client turns
        // into a short human table, or prints as-is for `--json`.
        Rpc::Status => {
            let fleet = build_fleet_state(state, &exec).await;
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&fleet).unwrap_or_else(|_| "{}".into()),
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
            let vmid = spec.manifest.vmid;
            let stack_name = spec.manifest.stack_name.clone();
            let resp = run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "deploy",
                Some(&stack_name),
                |ctx| Box::pin(async move { deploy(ctx, &spec).await }),
            )
            .await;
            let _ = std::fs::remove_dir_all(&dir);
            // fix-94: every gateway deploy re-reads the house's address and
            // keeps CrowdSec's whitelist equal to it. Its own operation, so
            // its lines reach the client before this deploy's answer does,
            // and a failure there is reported as itself rather than as a
            // failed deploy.
            if home_address_after_deploy(vmid, resp.ok, state.config.safety.gateway_vmid) {
                run_mutating_op(state, &exec, req.id, "home-address-whitelist", |ctx| {
                    Box::pin(
                        async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await },
                    )
                })
                .await;
            }
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
            let resp =
                run_mutating_op_for_stack(state, &exec, req.id, "backup", Some(&stack), |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await
                    })
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
            app,
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
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "restore", Some(&stack_name), |ctx| {
                Box::pin(async move {
                    homelab_core::ops::backup::restore_app(
                        ctx,
                        &manifest,
                        &cfg,
                        &snapshot,
                        !skip_safety_copy,
                        app.as_deref(),
                    )
                    .await
                })
            })
            .await
        }
        Rpc::UpdateStack { manifest, app } => {
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "update", Some(&stack_name), |ctx| {
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
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "resize", Some(&stack_name), |ctx| {
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
            let stack_name = m.stack_name.clone();
            run_mutating_op_for_stack(state, &exec, req.id, "adopt", Some(&stack_name), |ctx| {
                Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
            })
            .await
        }
        Rpc::InstallNative {
            manifest,
            binary_b64,
            unit_file,
        } => {
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "install-native",
                Some(&stack_name),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::install_native(
                            ctx,
                            &manifest,
                            &binary_b64,
                            &unit_file,
                        )
                        .await
                    })
                },
            )
            .await
        }
        // TUI parity round: the dashboard's install-native; the host fetches
        // and verifies the release itself (CT 120 has no gh).
        Rpc::InstallNativeRelease {
            manifest,
            unit_file,
            tag,
        } => {
            if let Err(problems) = homelab_core::native::validate_native(&manifest) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("service.yml invalid: {}", problems.join("; ")),
                    deferred: None,
                };
            }
            if manifest.release_repo.is_none() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "{} declares no release_repo — there is no release to install; add \
                         release_repo to its service.yml",
                        manifest.unit
                    ),
                    deferred: None,
                };
            }
            let stack_name = manifest.stack_name.clone();
            run_mutating_op_for_stack(
                state,
                &exec,
                req.id,
                "install-native",
                Some(&stack_name),
                |ctx| {
                    Box::pin(async move {
                        homelab_core::ops::native::install_release(ctx, &manifest, &tag, &unit_file)
                            .await
                    })
                },
            )
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
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "backup-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::backup_native(ctx, &m, &cfg).await
                                })
                            },
                        )
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
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "update-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::update_native(ctx, &m, stored_at)
                                        .await
                                })
                            },
                        )
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
        Rpc::RollbackNative { stack, unit } => {
            match native_from_state(&state.config.state_dir, &stack)
                .await
                .and_then(|(services, _)| {
                    homelab_core::ops::native::select_unit(&services, unit.as_deref())
                }) {
                Ok(m) => {
                    run_mutating_op_for_stack(
                        state,
                        &exec,
                        req.id,
                        "rollback-native",
                        Some(&stack),
                        |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::rollback_native(ctx, &m).await
                            })
                        },
                    )
                    .await
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
                        let r = run_mutating_op_for_stack(
                            state,
                            &exec,
                            req.id,
                            "release-update-native",
                            Some(&stack),
                            |ctx| {
                                Box::pin(async move {
                                    homelab_core::ops::native::release_update(ctx, &m).await
                                })
                            },
                        )
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
            // The same exclusion the deploy applies: the gateway holds other
            // stacks' routes.
            let keep =
                homelab_core::ops::deploy::generated_dirs(&state.config.safety, manifest.vmid);
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
        Rpc::FleetCheck {
            stack_files,
            digests,
            json,
            host_config,
        } => {
            let mut live = gather_live_facts(&exec, state, &stack_files, false).await;
            // fix-142: what the client's files say, for the repository comparison.
            live.digests = digests;
            // fix-110: as above, for config/host.toml against this host's own.
            live.declared_host_config = host_config;
            live.live_host_config = live_host_config_table(
                &std::fs::read_to_string(&state.config.config_path).unwrap_or_default(),
            );
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
                state.config.backup_max_age_s,
                homelab_core::ops::fleetcheck::GrowthLimits::default(),
                state.config.tile_watch_source.as_deref(),
                state.config.patch_threshold_s,
                state.config.host_meta_max_age_s,
                state.config.capacity_thresholds,
            );
            RpcResponse {
                id: req.id,
                ok: homelab_core::ops::fleetcheck::check_passes(&findings),
                message: if json {
                    serde_json::json!({
                        "passes": homelab_core::ops::fleetcheck::check_passes(&findings),
                        "findings": findings,
                    })
                    .to_string()
                } else {
                    render_findings(&findings)
                },
                deferred: None,
            }
        }
        // fix-68: always ok with a JSON body. A part that could not be read
        // travels inside it as `unread`, so the TUI can tell this reply from
        // any other by its shape and never mistakes a failure of it for the
        // end of an operation it has open.
        Rpc::Today {
            stack_files,
            digests,
            host_config,
        } => RpcResponse {
            id: req.id,
            ok: true,
            message: serde_json::to_string(
                &gather_today(&exec, state, &stack_files, digests, host_config).await,
            )
            .unwrap_or_default(),
            deferred: None,
        },
        // T69: the operator answered a suspended step. Delivering it is all
        // that happens here — the step itself is parked on a channel inside
        // the operation, not on this task.
        Rpc::Answer { id, allow, boot } => {
            // arch-host-link: an answer to a question from an earlier start of
            // this host must not answer today's question with the same id.
            if boot.as_ref().is_some_and(|b| *b != state.boot_id) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "question {} was asked by an earlier start of the host — not delivered",
                        id
                    ),
                    deferred: None,
                };
            }
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
                if report.ok {
                    record_device_backup(state, &name).await;
                }
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
        Rpc::Tiles { bare } => {
            // The plain executor: a reading may read a key on its way.
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let mut st = store.load().await.unwrap_or_default();
            if bare {
                for s in st.stacks.values_mut() {
                    if let Some(m) = s.manifest.as_mut() {
                        for t in m.tiles.values_mut() {
                            t.reading = None;
                        }
                    }
                }
            }
            let tiles = homelab_core::ops::tiles::read_tiles(&RealExecutor, &st).await;
            // Decision "deploys are known outages" (Kenny, 2026-09-30): AR12
            // holds the operation lock strictly one at a time, so at most
            // one stack is ever deploying from the host's own view.
            let deploying_stack = state
                .busy
                .lock()
                .ok()
                .and_then(|b| b.as_ref().and_then(|h| h.stack.clone()));
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({
                    "tiles": tiles,
                    "deploying_stack": deploying_stack,
                    "watch_interval_s": state.config.watch_interval_s,
                    "watch_down_after_s": state.config.watch_down_after_s,
                })
                .to_string(),
                deferred: None,
            }
        }
        // feat-overview-10 (homelab-admin, 2026-10-01): the dashboard's
        // backup calendar, its own query reading restic directly (not
        // through state.json's cached `last_backup`, which only ever holds
        // the newest night — a calendar needs every one of them).
        Rpc::BackupCalendar { stacks } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let st = store.load().await.unwrap_or_default();
            let cfg = state.config.backup.clone();
            let wanted: Vec<&String> = if stacks.is_empty() {
                st.stacks.keys().collect()
            } else {
                stacks.iter().collect()
            };
            let mut by_stack = serde_json::Map::new();
            let mut skipped = Vec::new();
            for name in wanted {
                let Some(entry) = st.stacks.get(name) else {
                    skipped.push(format!("{name}: not a known stack"));
                    continue;
                };
                let Some(m) = entry.manifest.as_ref() else {
                    skipped.push(format!("{name}: no manifest on record"));
                    continue;
                };
                if m.backs_up_nothing() {
                    continue;
                }
                let times = homelab_core::ops::backup::snapshot_nights_unix(&exec, m, &cfg).await;
                by_stack.insert(name.clone(), serde_json::json!(times));
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "stacks": by_stack, "skipped": skipped }).to_string(),
                deferred: None,
            }
        }
        Rpc::ListManualChecks { json } => {
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
                message: if json {
                    serde_json::json!({
                        "now": now,
                        "checks": rows
                            .iter()
                            .map(|(id, rec)| serde_json::json!({ "id": id, "record": rec }))
                            .collect::<Vec<_>>(),
                    })
                    .to_string()
                } else {
                    homelab_core::ops::manualchecks::render_listing(&rows, now)
                },
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
        // feat-backup-1: per-repository status (D25's owning-app repos for
        // a compose stack, one per unit for a native stack), joined with
        // the restore-drill verdict the nightly drill already recorded.
        Rpc::GetBackups { stack } => {
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await.unwrap_or_default();
            let Some(st) = snapshot.stacks.get(&stack) else {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("no such stack '{}'", stack),
                    deferred: None,
                };
            };
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
            let (native, statuses) = if let Some(m) = &st.manifest {
                (
                    false,
                    homelab_core::ops::backup::backup_status(
                        &exec,
                        m,
                        &cfg,
                        &snapshot.restore_drills,
                    )
                    .await,
                )
            } else {
                let mut v = Vec::new();
                for n in &st.natives {
                    v.push(
                        homelab_core::ops::backup::repo_status_of(
                            &exec,
                            &cfg,
                            &snapshot.restore_drills,
                            n.unit.clone(),
                        )
                        .await,
                    );
                }
                (true, v)
            };
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::json!({ "native": native, "repos": statuses }).to_string(),
                deferred: None,
            }
        }
        // feat-backup-3: read-only, never stops anything.
        Rpc::BrowseSnapshot {
            owner,
            snapshot,
            path,
        } => {
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
            match homelab_core::ops::backup::browse_snapshot(&exec, &cfg, &owner, &snapshot, &path)
                .await
            {
                Ok(entries) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e.to_string(),
                    deferred: None,
                },
            }
        }
        Rpc::RestoreNative {
            stack,
            snapshot,
            confirm,
        } => match native_from_state(&state.config.state_dir, &stack).await {
            Ok((services, _)) => {
                let cfg = state.config.backup.clone();
                let mut resp = RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!("no services on stack '{}'", stack),
                    deferred: None,
                };
                for m in services {
                    let cfg = cfg.clone();
                    let snap = snapshot.clone();
                    let confirm = confirm.clone();
                    let r = run_mutating_op_for_stack(
                        state,
                        &exec,
                        req.id,
                        "restore-native",
                        Some(&stack),
                        |ctx| {
                            Box::pin(async move {
                                homelab_core::ops::native::restore_native(
                                    ctx,
                                    &m,
                                    &cfg,
                                    &snap,
                                    confirm.as_deref(),
                                )
                                .await
                            })
                        },
                    )
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
        },
        // feat-secrets-1: the value, read straight from the host's own
        // vault (never a process, never traced). Audited before the read,
        // naming what was asked for, never the value.
        Rpc::RevealSecret { stack, secret } => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let audit = format!("{} reveal-secret stack={} secret={:?}\n", ts, stack, secret);
            if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &audit) {
                tracing::warn!("audit.log: could not record the reveal :: {}", e);
            }
            match crate::secrets::reveal(&state.config.state_dir, &stack, &secret).await {
                Ok(content) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: content,
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // feat-secrets-2: through latch, exactly the one relative path a
        // deploy would read back — the rest of latch is untouched. Audited
        // before the write, never the value; `latch_put` itself never goes
        // through `Executor`/`Cmd` either (core::ops::secrets).
        Rpc::SetSecret {
            stack,
            secret,
            content,
        } => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let rel = homelab_core::ops::secrets::latch_rel_path(&stack, &secret);
            let audit = format!(
                "{} set-secret stack={} secret={:?} rel={}\n",
                ts, stack, secret, rel
            );
            if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &audit) {
                tracing::warn!("audit.log: could not record the write :: {}", e);
            }
            let env = std::env::var("HOMELAB_LATCH_ENV").unwrap_or_default();
            let project_root = format!("{}/repo", state.config.state_dir);
            let result = tokio::task::spawn_blocking(move || {
                crate::secrets::latch_put(&project_root, &env, &rel, &content)
            })
            .await;
            match result {
                Ok(Ok(())) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "{} written in latch; the other files latch holds for this stack are \
                         untouched; redeploy the stack for the running container to pick it up",
                        homelab_core::ops::secrets::latch_rel_path(&stack, &secret)
                    ),
                    deferred: None,
                },
                Ok(Err(e)) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("internal: {e}"),
                    deferred: None,
                },
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
        // feat-settings-1: read and write host.toml for the dashboard. The
        // answers go to the asking session only; nothing is broadcast, so a
        // TUI's settings screen keeps its unsaved edits.
        Rpc::GetHostConfig => {
            let path = &state.config.config_path;
            let raw = std::fs::read_to_string(path).unwrap_or_default();
            match host_config_view(path, &raw) {
                Ok(view) => RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::to_string(&view).unwrap_or_default(),
                    deferred: None,
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::SetHostConfig {
            changes,
            expect_sha256,
        } => {
            // fix-false-alarm (2026-09-30): a `watch_url` the host cannot
            // reach would not fail until the notice five minutes later —
            // the same request `watch_dashboard` runs every minute, once,
            // before the value is even written.
            if let Some(url) = watch_url_to_check(&changes) {
                let out = RealExecutor
                    .run(&homelab_core::executor::Cmd::new(
                        "curl",
                        &["-sf", "-m", "10", "-o", "/dev/null", &url],
                        20,
                    ))
                    .await;
                let reached = out.as_ref().map(|o| o.success()).unwrap_or(false);
                if let Some(reason) = watch_url_refusal(&url, reached) {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: reason,
                        deferred: None,
                    };
                }
            }
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let outcome = apply_host_config_changes(&raw, &changes, &expect_sha256)
                .and_then(|(text, saved)| write_config_file(&path, &text).map(|()| (text, saved)));
            match outcome {
                Ok((text, saved)) => {
                    // The G8 keys are live: the scheduler hour, the webhook,
                    // the retention and (fix-122) the log level read the
                    // settings, not the file.
                    if let Ok(file) = toml::from_str::<FileConfig>(&text) {
                        let new_level = file
                            .log_level
                            .clone()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| "info".to_string());
                        {
                            let mut live = state
                                .settings
                                .write()
                                .unwrap_or_else(PoisonError::into_inner);
                            live.backup_hour = file.backup_hour;
                            live.notify_webhook = file.notify_webhook;
                            live.retention = file
                                .retention
                                .unwrap_or_else(homelab_core::retention::default_tiers);
                            live.log_level = new_level.clone();
                        }
                        // fix-122: reload the filter both the journald sink
                        // and the JSONL ring read from. `startup_problems`
                        // already refused an unparsable directive before the
                        // file was written, so this falls back to "info"
                        // only in the theoretical case of a file edited by
                        // hand between the write above and this line.
                        let _ = state.log_filter.reload(
                            tracing_subscriber::EnvFilter::try_new(&new_level)
                                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                        );
                    }
                    info!(
                        "host.toml changed via the dashboard: {}",
                        changes.keys().cloned().collect::<Vec<_>>().join(", ")
                    );
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&saved).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // fix-120 (per-machine tokens, owner decision 2026-10-01): mint,
        // list and revoke `[[tokens]]` entries. The scope-all audit line
        // above already named who issued or revoked.
        Rpc::TokenIssue { name, scope } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let plain = match random_token() {
                Ok(t) => t,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: e,
                        deferred: None,
                    }
                }
            };
            match issue_token_in(&raw, &name, scope, &plain)
                .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
            {
                Ok(tokens) => {
                    *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
                    info!(name = %name, scope = ?scope, "token issued");
                    let issued = homelab_proto::TokenIssued {
                        name,
                        scope,
                        token: plain,
                    };
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&issued).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::TokenList => {
            let mut views: Vec<homelab_proto::TokenView> = state
                .live_tokens()
                .into_iter()
                .map(|t| homelab_proto::TokenView {
                    name: t.name,
                    scope: t.scope,
                })
                .collect();
            if !state.config.token.is_empty() {
                views.insert(
                    0,
                    homelab_proto::TokenView {
                        name: "legacy".into(),
                        scope: homelab_proto::Scope::All,
                    },
                );
            }
            RpcResponse {
                id: req.id,
                ok: true,
                message: serde_json::to_string(&views).unwrap_or_default(),
                deferred: None,
            }
        }
        Rpc::TokenRevoke { name } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            match revoke_token_in(&raw, &name)
                .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
            {
                Ok(tokens) => {
                    *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
                    info!(name = %name, "token revoked");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: format!("token {:?} revoked", name),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        // fix-110: `config/host.toml` sent whole — `homelab host apply`, or
        // the dashboard's declarative commit of it. Same outcome shape as
        // `SetHostConfig`: the G8 keys go live immediately, the rest waits
        // for the host's next start.
        Rpc::ApplyHostConfig {
            toml: declared,
            expect_sha256,
        } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let outcome = apply_host_config_whole(&raw, &declared, expect_sha256.as_deref())
                .and_then(|(text, saved)| write_config_file(&path, &text).map(|()| (text, saved)));
            match outcome {
                Ok((text, saved)) => {
                    if let Ok(file) = toml::from_str::<FileConfig>(&text) {
                        let new_level = file
                            .log_level
                            .clone()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| "info".to_string());
                        {
                            let mut live = state
                                .settings
                                .write()
                                .unwrap_or_else(PoisonError::into_inner);
                            live.backup_hour = file.backup_hour;
                            live.notify_webhook = file.notify_webhook;
                            live.retention = file
                                .retention
                                .unwrap_or_else(homelab_core::retention::default_tiers);
                            live.log_level = new_level.clone();
                        }
                        // fix-122: same live reload as `SetHostConfig`.
                        let _ = state.log_filter.reload(
                            tracing_subscriber::EnvFilter::try_new(&new_level)
                                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                        );
                    }
                    info!("host.toml applied from config/host.toml (fix-110)");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&saved).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
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
        // Owner decision 2026-09-30 (item 2): "Save and restart the host".
        // Refused outright rather than queued: waiting behind a running
        // job and then restarting mid-nightly-round is not what "restart
        // the host" should ever mean.
        Rpc::RestartHost => {
            if let Some(holder) = state.busy.lock().ok().and_then(|b| b.clone()) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!(
                        "refused: an operation is running ({}); running jobs are refused while a job runs",
                        holder.what
                    ),
                    deferred: None,
                };
            }
            run_mutating_op(state, &exec, req.id, "restart-host", |ctx| {
                Box::pin(async move { homelab_core::ops::restarthost::restart_host(ctx).await })
            })
            .await
        }
        Rpc::Doctor { json } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut probes = gather_probes(
                &exec,
                &state.config.state_dir,
                state.config.mirror_remote.as_deref(),
                state.config.backup.staging_dir.as_deref(),
                &state.config.restore_drill_scratch_dir,
                now,
                &|line: &str| progress_line(state, line),
            )
            .await;
            probes.failed_auth = Some(state.auth_failures.snapshot());
            gather_security_probes(&exec, &ProbeContext::of(&state.config), now, &mut probes).await;
            let checks = homelab_core::doctor::diagnose(&probes);
            let overall = homelab_core::doctor::overall(&checks);
            if json {
                return RpcResponse {
                    id: req.id,
                    ok: true,
                    message: serde_json::json!({ "overall": overall, "checks": checks })
                        .to_string(),
                    deferred: None,
                };
            }
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
            let fleet = build_fleet_state(state, &exec).await;
            let _ = state.log_tx.send(ServerMsg::State(Box::new(fleet)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "state".into(),
                deferred: None,
            }
        }
        // fix-131: one bundle, read on the workstation.
        Rpc::IncidentShow { name } => {
            let (ok, message) = match incident_text(&state.config.state_dir, &name) {
                Ok(text) => (true, text),
                Err(why) => (false, why),
            };
            RpcResponse {
                id: req.id,
                ok,
                message,
                deferred: None,
            }
        }
        Rpc::Incidents { json } => {
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
                message: if json {
                    serde_json::json!({ "incidents": list }).to_string()
                } else if list.is_empty() {
                    "no incidents recorded".into()
                } else {
                    format!("incidents:\n  {}", list.join("\n  "))
                },
                deferred: None,
            }
        }
    }
}

/// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
/// remove the bundles older than `BUNDLE_MAX_AGE_DAYS` and those beyond the
/// newest `BUNDLE_MAX_COUNT`; returns how many went. Nothing pruned them.
fn prune_incidents(state_dir: &str, now: u64, max_age_days: u64, max_count: usize) -> usize {
    use homelab_core::incidents::bundles_to_prune;
    let dir = format!("{}/incidents", state_dir);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut removed = 0;
    for name in bundles_to_prune(&names, now, max_age_days, max_count) {
        match std::fs::remove_dir_all(format!("{}/{}", dir, name)) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("incidents: could not remove {} :: {}", name, e),
        }
    }
    if removed > 0 {
        info!(
            "incidents: removed {} bundle(s) older than {} days or beyond the newest {}",
            removed, max_age_days, max_count
        );
    }
    removed
}

/// rule-20: remove `push-staging-*` files under the state dir that
/// `util::stale_push_staging` judges orphaned — left behind by a push the
/// daemon never finished. Called at start (the previous run's leftovers)
/// and after every mutating operation (that operation's own, if it failed
/// partway). Best-effort and silent on an empty/missing directory: a state
/// dir with nothing staged is the common case, not a fault.
fn cleanup_push_staging(state_dir: &str, now: u64) -> usize {
    use homelab_core::ops::util::{stale_push_staging, STALE_PUSH_STAGING_MAX_AGE_S};
    let entries: Vec<(String, u64)> = std::fs::read_dir(state_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let mtime = e
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((name, mtime))
        })
        .collect();
    let mut removed = 0;
    for name in stale_push_staging(&entries, now, STALE_PUSH_STAGING_MAX_AGE_S) {
        match std::fs::remove_file(format!("{}/{}", state_dir, name)) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!("push-staging cleanup: could not remove {} :: {}", name, e),
        }
    }
    if removed > 0 {
        info!(
            "push-staging cleanup: removed {} orphaned file(s) older than {}s",
            removed, STALE_PUSH_STAGING_MAX_AGE_S
        );
    }
    removed
}

/// fix-131: cut journal.jsonl back when it has outgrown its limit
/// (`incidents::compact_journal`), written whole through a temp file.
/// Called only while nothing writes the journal: at start, and under the
/// operation lock.
///
/// gap-26: `max_bytes` used to be the hardcoded `incidents::JOURNAL_MAX_BYTES`
/// — unlike the incident-bundle limits right beside it in host.toml, this one
/// had no key and no dashboard row. The caller now passes `config.journal_max_bytes`.
fn compact_journal_file(state_dir: &str, max_bytes: u64) {
    let path = format!("{}/journal.jsonl", state_dir);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(cut) = homelab_core::incidents::compact_journal(&content, max_bytes as usize) else {
        return;
    };
    let tmp = format!("{}.compact.tmp", path);
    let written = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(cut.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &path)
    })();
    match written {
        Ok(()) => info!(
            "journal.jsonl: cut from {} to {} bytes, interrupted operations kept",
            content.len(),
            cut.len()
        ),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!("journal.jsonl: could not compact :: {}", e);
        }
    }
}

/// fix-131: one incident bundle as text: what failed, the versions, the
/// end of the transcript and where the rest is. `name` must be a bundle
/// name as `homelab incidents` lists it, checked before it becomes a path.
fn incident_text(state_dir: &str, name: &str) -> Result<String, String> {
    const TAIL: usize = 60;
    let plain = !name.is_empty()
        && name.split_once('-').is_some_and(|(ts, op)| {
            !op.is_empty() && !ts.is_empty() && ts.chars().all(|c| c.is_ascii_digit())
        })
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !plain {
        return Err(format!(
            "'{}' is not a bundle name :: `homelab incidents` lists them (<unix-time>-<operation>)",
            name
        ));
    }
    let dir = format!("{}/incidents/{}", state_dir, name);
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!(
            "no bundle named {} :: `homelab incidents` lists the ones kept (bundles older than {} \
             days are removed)",
            name,
            homelab_core::incidents::BUNDLE_MAX_AGE_DAYS
        ));
    }
    let read = |f: &str| std::fs::read_to_string(format!("{}/{}", dir, f)).unwrap_or_default();
    let mask = homelab_core::executor::mask_secrets;
    let mut out = format!("incident {}\n", name);
    if let Ok(report) = serde_json::from_str::<serde_json::Value>(&read("report.json")) {
        let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
        out.push_str(&format!("  operation: {}\n", s(&report["op"])));
        let err = &report["error"];
        if !err.is_null() {
            out.push_str(&format!("  what:   {}\n", mask(&s(&err["what"]))));
            out.push_str(&format!("  why:    {}\n", mask(&s(&err["why"]))));
            out.push_str(&format!("  remedy: {}\n", mask(&s(&err["remedy"]))));
        }
    }
    let versions = read("versions.txt");
    out.push_str(&format!(
        "  versions: {}\n",
        versions.split_whitespace().collect::<Vec<_>>().join(" ")
    ));
    let lines: Vec<String> = read("events.jsonl")
        .lines()
        .filter_map(|l| serde_json::from_str::<PipelineEvent>(l).ok())
        .filter_map(|e| match e {
            PipelineEvent::Line { msg, .. } => Some(mask(&msg)),
            _ => None,
        })
        .collect();
    let from = lines.len().saturating_sub(TAIL);
    out.push_str(&format!(
        "last {} of {} transcript line(s):\n",
        lines.len() - from,
        lines.len()
    ));
    for l in &lines[from..] {
        out.push_str(&format!("  {}\n", l));
    }
    out.push_str(&format!(
        "the whole bundle, root only on the host: {} (commands.sh replays what ran)\n",
        dir
    ));
    Ok(out)
}

/// fix-130: what the new doctor probes need from the configuration.
struct ProbeContext {
    listen: String,
    exec_enabled: bool,
    config_path: String,
    password_file: String,
    privileged_vmids: Vec<u16>,
    no_touch: Vec<u16>,
    drill_interval_s: u64,
    state_dir: String,
    /// fix-130 (second half): where to read the gateway's routes directory,
    /// to name any file in it that no stack declares (the same judgement the
    /// fleet check carries, `fleetcheck::unowned_route_files`).
    gateway_vmid: u16,
    gateway_routes_dir: String,
}

impl ProbeContext {
    fn of(config: &Config) -> Self {
        ProbeContext {
            listen: config.listen.to_string(),
            exec_enabled: config.exec_enabled,
            config_path: config.config_path.clone(),
            password_file: config.backup.password_file.clone(),
            privileged_vmids: config.safety.privileged_vmids.clone().unwrap_or_default(),
            no_touch: config.safety.no_touch.clone(),
            drill_interval_s: config.restore_drill_interval_s,
            state_dir: config.state_dir.clone(),
            gateway_vmid: config.safety.gateway_vmid,
            gateway_routes_dir: config.safety.gateway_routes_dir.clone(),
        }
    }
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the probes
/// doctor lacked. Cheap reads only: `stat`, one loop over the container
/// configurations, state, `test -s` and one `rclone about`. A probe that
/// cannot be read stays `None`, so doctor leaves its line out rather than
/// guess.
async fn gather_security_probes(
    exec: &dyn Executor,
    pc: &ProbeContext,
    now_unix: u64,
    probes: &mut homelab_core::doctor::Probes,
) {
    use homelab_core::doctor::{DrillProbe, DriveSpace, Exposure, Freshness, Privileged};
    probes.exposure = Some(Exposure {
        listen: pc.listen.clone(),
        exec_enabled: pc.exec_enabled,
    });

    // Private files: group or world bits set is a finding. A path that does
    // not exist prints nothing on stdout, which is right: absent is not loose.
    let private = [
        pc.config_path.clone(),
        pc.password_file.clone(),
        format!("{}/secrets", pc.state_dir),
        format!("{}/tls-key.pem", pc.state_dir),
        format!("{}/audit.log", pc.state_dir),
        format!("{}/incidents", pc.state_dir),
    ];
    let mut args: Vec<&str> = vec!["-c", "%a %n"];
    args.extend(private.iter().map(String::as_str));
    let stat_cmd = Cmd::new("stat", &args, 20);

    // Privileged containers. Templates are left out (the golden -priv
    // template is privileged on purpose and never runs), and the no-touch
    // guests' configurations are not even read: the list is law.
    let skip = pc
        .no_touch
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("|");
    let script = format!(
        "for f in /etc/pve/lxc/*.conf; do v=$(basename \"$f\" .conf); \
         case \"$v\" in {}) continue;; esac; \
         grep -q '^template: 1' \"$f\" && continue; \
         grep -q '^unprivileged: 1' \"$f\" || echo \"$v\"; done",
        if skip.is_empty() { "-".into() } else { skip }
    );
    let privileged_cmd = Cmd::new("sh", &["-c", &script], 20);
    let state_path = format!("{}/state.json", pc.state_dir);
    // Present and not empty; the content never leaves the file.
    let password_cmd = Cmd::new("test", &["-s", &pc.password_file], 10);
    // Drive's space, trash included: pruned packs stay in the trash and
    // count against the quota until it is emptied.
    let offsite_configured = probes.offsite_configured;
    let about_cmd = Cmd::new("rclone", &["about", "gdrive:", "--json"], 60);
    let about_fut = async {
        if offsite_configured {
            Some(exec.run(&about_cmd).await)
        } else {
            None
        }
    };
    // fix-130 (second half): the same `ls -1A` the fleet check runs, so
    // doctor can name an unowned route file without waiting for a nightly
    // round. Any extension counts — a `.bak` is still a file nobody owns.
    let routes_cmd = homelab_core::executor::attach_sh(
        pc.gateway_vmid,
        &format!("ls -1A '{}'", pc.gateway_routes_dir),
        30,
    );
    // ops::pool: six independent reads, overlapped; polled in the order
    // they used to run.
    let (stat_out, privileged_out, state_raw, password_out, about_out, routes_out) = futures_util::join!(
        exec.run(&stat_cmd),
        exec.run(&privileged_cmd),
        exec.read_file(&state_path),
        exec.run(&password_cmd),
        about_fut,
        exec.run(&routes_cmd)
    );

    if let Ok(out) = stat_out {
        probes.loose_files = Some(
            out.stdout
                .lines()
                .filter_map(|l| l.split_once(' '))
                .filter(|(mode, _)| u32::from_str_radix(mode, 8).is_ok_and(|m| m & 0o077 != 0))
                .map(|(mode, path)| format!("{} ({})", path, mode))
                .collect(),
        );
    }

    if let Ok(out) = privileged_out {
        let mut vmids: Vec<u16> = out
            .stdout
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        vmids.sort_unstable();
        let outside_policy = vmids
            .iter()
            .copied()
            .filter(|v| !pc.privileged_vmids.contains(v))
            .collect();
        probes.privileged = Some(Privileged {
            vmids,
            outside_policy,
        });
    }

    // Host-meta and the restore drill, from the record.
    let hs: Option<homelab_core::state::HostState> = state_raw
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    if let Some(hs) = &hs {
        let age = |t: u64| (t > 0).then(|| now_unix.saturating_sub(t) / 3600);
        probes.host_meta = Some(Freshness {
            age_h: age(hs.last_host_meta),
        });
        probes.restore_drill = Some(DrillProbe {
            age_h: age(hs.last_restore_drill),
            interval_h: pc.drill_interval_s / 3600,
            failing: hs
                .restore_drills
                .iter()
                .filter_map(|(repo, r)| r.last_error.as_ref().map(|e| format!("{}: {}", repo, e)))
                .collect(),
        });
    }

    // fix-130 (second half): judged the same way the fleet check judges it
    // (`fleetcheck::unowned_route_files`) — only when both the listing and
    // the state parsed; an unreadable gateway is no fact, not an empty one.
    if let (Some(hs), Ok(out)) = (&hs, &routes_out) {
        if out.success() {
            let on_disk: Vec<String> = out
                .stdout
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            probes.unowned_route_files = Some(homelab_core::ops::fleetcheck::unowned_route_files(
                hs, &on_disk,
            ));
        }
    }

    probes.password_file_ok = password_out.ok().map(|o| o.success());

    if let Some(Ok(out)) = about_out {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out.stdout) {
            let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
            if n("total") > 0 {
                probes.drive = Some(DriveSpace {
                    total: n("total"),
                    free: n("free"),
                    trashed: n("trashed"),
                });
            }
        }
    }
}

/// Gather doctor probes (F6). I/O stays here; the verdict logic is in core.
/// H8 hardening: the probe layer now feeds REAL data — backup freshness per
/// stack from state.json, offsite reachability via a quick rclone listing,
/// mirror lag via unpushed-commit count. Generic over the executor so the
/// healthy/broken matrix is testable with MockExecutor.
/// `progress` gets one line per phase (fix-104, long-silences, 2026-09-27:
/// `homelab doctor` took 24 s with nothing on the screen after "link up").
async fn gather_probes(
    exec: &dyn Executor,
    state_dir: &str,
    mirror_remote: Option<&str>,
    staging_dir: Option<&str>,
    restore_scratch_dir: &str,
    now_unix: u64,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> homelab_core::doctor::Probes {
    use homelab_core::doctor::{Probes, StackProbe};
    progress("doctor: every managed container, its backup age and its sealed secrets…");
    let state_raw = exec.read_file(&format!("{}/state.json", state_dir)).await;
    let state_parses = state_raw
        .as_ref()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
        .unwrap_or(true);

    // Per-stack backup freshness + container presence from state.json.
    // ops::pool: the stacks are asked a few at a time, in state order.
    let stacks: Vec<(String, homelab_core::state::StackState)> = state_raw
        .as_ref()
        .ok()
        .and_then(|raw| serde_json::from_str::<homelab_core::state::HostState>(raw).ok())
        .map(|hs| hs.stacks.into_iter().collect())
        .unwrap_or_default();
    let stacks_fut = homelab_core::ops::pool::bounded(
        stacks
            .iter()
            .map(|(name, st)| async move {
                // Present = Proxmox has a configuration for it, which is what
                // `pct status` answered; read off pmxcfs without pct's 0.45 s of
                // Perl start-up (measured 2026-09-29).
                let present = exec
                    .read_file(&homelab_core::executor::lxc_conf_path(st.vmid))
                    .await
                    .is_ok();
                // gap-27: sealed = every secret file on the container has a
                // vault copy. It was hard-coded true, so the check never fired.
                let env_sealed = !present
                    || homelab_core::ops::facts::unsealed_secret_files(exec, state_dir, name, st)
                        .await
                        .is_empty();
                StackProbe {
                    name: name.clone(),
                    backup_age_h: (st.last_backup > 0)
                        .then(|| now_unix.saturating_sub(st.last_backup) / 3600),
                    container_present: present,
                    env_sealed,
                    // fix-130: a native stack's services are backed up whole.
                    nothing_to_back_up: !st.is_native()
                        && st.manifest.as_ref().is_some_and(|m| m.backs_up_nothing()),
                }
            })
            .collect(),
        homelab_core::ops::pool::READ_CONCURRENCY,
    );

    // Offsite: is the gdrive remote configured, and does a cheap listing work?
    let offsite_fut = async {
        progress("doctor: the offsite remote (one listing on Google Drive)…");
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
        (offsite_configured, offsite_token_valid)
    };

    // Mirror lag: commits not yet on the mirror remote.
    let repo = format!("{}/repo", state_dir);
    let mirror_fut = async {
        progress("doctor: mirror, disk and the daemon's own units…");
        match mirror_remote {
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
        }
    };
    // Independent reads, overlapped (ops::pool); polled in the order they
    // used to run.
    let (managed_stacks, (offsite_configured, offsite_token_valid), mirror_behind) =
        futures_util::join!(stacks_fut, offsite_fut, mirror_fut);
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
    // fix-113 ADDENDUM: free % on the chassis backup staging directory —
    // only asked when one is configured; `None` outer = not configured, so
    // `diagnose` raises no finding over a feature nobody turned on.
    let staging_disk_free_pct = match staging_dir {
        Some(dir) => Some(
            exec.run(&Cmd::new("df", &["--output=pcent", dir], 20))
                .await
                .ok()
                .and_then(|o| {
                    o.stdout
                        .lines()
                        .nth(1)
                        .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                        .map(|used| 100u64.saturating_sub(used))
                }),
        ),
        None => None,
    };
    // fix-62: free % on the restore drill's scratch directory — always
    // asked, since a default applies when host.toml names none.
    let restore_scratch_disk_free_pct = exec
        .run(&Cmd::new(
            "df",
            &["--output=pcent", restore_scratch_dir],
            20,
        ))
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
        staging_disk_free_pct,
        restore_scratch_disk_free_pct,
        state_parses,
        managed_stacks,
        offsite_configured,
        offsite_token_valid,
        mirror_behind,
        interrupted_ops: interrupted,
        // fix-120: filled in by the caller, which holds the counter.
        failed_auth: None,
        exposure: None,
        loose_files: None,
        privileged: None,
        host_meta: None,
        restore_drill: None,
        password_file_ok: None,
        drive: None,
        unowned_route_files: None,
    }
}
