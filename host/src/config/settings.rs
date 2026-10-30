//! The resolved settings the daemon runs with.

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
