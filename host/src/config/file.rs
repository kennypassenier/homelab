//! The settings file as written: its structs and the defaults and sanity rules for single values.

use super::*;

/// One externally-made backup to keep an eye on.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct WatchedBackup {
    /// What the nightly report calls it.
    pub(crate) name: String,
    /// An rclone path, e.g. `gdrive:homelab-backups/OPNSense-backups`.
    pub(crate) rclone_path: String,
    /// Older than this and it becomes a finding. OPNsense runs its backup
    /// cron at 01:00 with up to an hour of jitter, so 26 leaves room for a
    /// late night without crying wolf.
    #[serde(default = "default_watched_max_age_hours")]
    pub(crate) max_age_hours: u64,
}

pub(crate) fn default_watched_max_age_hours() -> u64 {
    26
}

/// Who a session belongs to, decided once when it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) name: String,
    pub(crate) scope: homelab_proto::Scope,
}

/// fix-122: `tracing_subscriber::EnvFilter::try_new` almost never returns
/// `Err` — its directive grammar is deliberately forgiving, so garbage like
/// "not a directive!!" parses as a bare directive and is accepted rather
/// than refused (measured 2026-10-01: `try_new` on exactly that string is
/// `Ok`). A log level a person can type wrong needs a check that actually
/// rejects the wrong ones, so this re-checks the grammar `EnvFilter` itself
/// documents: comma-separated directives, each either a bare level keyword
/// or `target[span]=level`.
pub(crate) fn log_level_is_valid(s: &str) -> bool {
    fn is_level(s: &str) -> bool {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "trace" | "debug" | "info" | "warn" | "error" | "off"
        ) || matches!(s, "0" | "1" | "2" | "3" | "4" | "5")
    }
    fn is_target(s: &str) -> bool {
        // A span suffix, `target[span]`, narrows further; only the target
        // itself (a crate/module path) needs validating here.
        let target = s.split('[').next().unwrap_or(s);
        !target.is_empty()
            && target
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && target
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-'))
    }
    let s = s.trim();
    !s.is_empty()
        && s.split(',').map(str::trim).all(|d| {
            if d.is_empty() {
                return false;
            }
            match d.split_once('=') {
                Some((target, level)) => is_target(target) && is_level(level),
                None => is_level(d),
            }
        })
}

/// host.toml as written: the one serde representation of the file.
///
/// config-four-representations (see REGISTER.md fix-134): the same struct is
/// read, checked for keys it does not read (`unknown_keys`), and written
/// back by a settings save (`render_settings_toml`). Every field is optional
/// so that what is written back is only what the file said; defaults are
/// applied when `Config` is resolved.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub(crate) struct FileConfig {
    pub(crate) token: Option<String>,
    /// arch-tokens (homelab-admin, 2026-09-28): one entry per machine, each
    /// with a scope. Only the SHA-256 of a token is stored here; the token
    /// itself lives with the client. `token` above keeps working as scope
    /// `all` under the name "legacy" until it is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tokens: Option<Vec<TokenEntry>>,
    pub(crate) listen: Option<String>,
    pub(crate) state_dir: Option<String>,
    pub(crate) backup_hour: Option<u8>,
    pub(crate) notify_webhook: Option<String>,
    /// Bearer token sent with the notification POST, when the target needs
    /// one. Added 2026-08-31: Kenny chose to route homelab warnings through
    /// the kyu hub rather than straight at Home Assistant (R2), so that a
    /// warning survives HA being the thing that is broken — and the hub
    /// requires a token where the HA webhook did not.
    pub(crate) notify_auth_bearer: Option<String>,
    /// G14: how long a passed restore drill counts for. B3 says quarterly;
    /// the default is 90 days. Kenny's, not the author's — a house that
    /// changes little wants it longer, one being rebuilt wants it shorter.
    pub(crate) restore_drill_interval_s: Option<u64>,
    /// fix-62 (restore-drill-covers-almost-nothing, 2026-10-01): where the
    /// nightly restore drill restores a repository to — a data pool, not the
    /// root disk, same reasoning as `native_backup_staging_dir`.
    pub(crate) restore_drill_scratch_dir: Option<String>,
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
    /// fix-93 (2026-09-27): the container Loki runs in. Loki's LAN port
    /// takes pushes only since then, so the coverage check asks from inside
    /// this container. Absent = asked at `loki_url` from the host.
    pub(crate) loki_vmid: Option<u16>,
    /// How far back the log-coverage question looks. A quiet service is not
    /// a broken one: `homepage` and `kp-soft` ship a few hundred lines a day
    /// and none at all in a given hour, and the first version of this check
    /// reported both as "logs are going nowhere". A check that alarms on
    /// healthy silence is a check that gets ignored, and then the real
    /// silence goes unnoticed too.
    pub(crate) logs_window: Option<String>,
    pub(crate) retention: Option<Vec<homelab_proto::RetentionTier>>,
    pub(crate) exec_enabled: Option<bool>,
    /// The latch environment the Secrets page writes to (`prod`).
    pub(crate) latch_env: Option<String>,
    pub(crate) mirror_remote: Option<String>,
    pub(crate) no_touch: Option<Vec<u16>>,
    /// fix-120 (api-token-is-root, 2026-09-27): vmids a deploy may make a
    /// privileged container. Absent = the ones the fleet runs today
    /// (`homelab_core::safety::FLEET_PRIVILEGED_VMIDS`). ssh-edited only,
    /// like exec_enabled: a policy the token could change is no policy.
    pub(crate) privileged_vmids: Option<Vec<u16>>,
    /// fix-120: host directories `data_mounts:` may borrow. Absent = the ones
    /// the fleet uses today (`FLEET_DATA_MOUNT_ROOTS`).
    pub(crate) data_mount_roots: Option<Vec<String>>,
    pub(crate) gateway_vmid: Option<u16>,
    pub(crate) gateway_routes_dir: Option<String>,
    /// Route A (Kenny, form J1, 2026-09-02): devices this suite may not
    /// touch, that can nevertheless hand over their own configuration. One
    /// GET per night, straight into restic. Absent = nothing is fetched.
    pub(crate) device_backups: Option<Vec<homelab_core::ops::devicebackup::DeviceBackup>>,
    /// O1: backups this orchestrator does not make but does watch. Each is
    /// a folder on an rclone remote that some device outside the suite
    /// writes to — today the OPNsense plugin that uploads the router's
    /// configuration every night. Absent = nothing is watched, which is what
    /// every fleet had before this existed.
    pub(crate) watched_backups: Option<Vec<WatchedBackup>>,
    /// E8: ZFS snapshot+replication jobs (replaces the old cron script).
    pub(crate) zfs_jobs: Option<Vec<homelab_core::ops::zfs::ZfsJob>>,
    /// D60: `[registry_cache] host = "10.10.10.17"` plus one `[[registry_cache.upstreams]]`
    /// per mirrored registry. Absent = no cache.
    pub(crate) registry_cache: Option<homelab_core::ops::registry_cache::CacheCfg>,
    /// rule-20 (disk-audit, 2026-10-01): `[default_log_rotation]` — the
    /// fleet default a data mount gets when it declares no `rotate:` of its
    /// own and does not opt out. Absent = no fleet default (today's
    /// behaviour: only an explicit `rotate:` rotates anything).
    pub(crate) default_log_rotation: Option<homelab_core::ops::guards::FleetLogRotationDefault>,
    /// Where restic writes. Was a string literal in BackupCfg::default(),
    /// which meant exactly one backup target could ever be addressed while
    /// the scope asks for two (deployment project, F39 / standing rule 27).
    pub(crate) restic_base: Option<String>,
    /// Path on this host to the file holding the restic password.
    pub(crate) restic_password_file: Option<String>,
    /// Seconds a single snapshot may take. Default 4 h: a first multi-GB
    /// upload over a residential uplink is slow.
    pub(crate) restic_snapshot_timeout_s: Option<u64>,
    /// Seconds a single restore may take. Default 4 h, matching the snapshot
    /// side (F38, see REGISTER.md).
    pub(crate) restic_restore_timeout_s: Option<u64>,
    /// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): where a
    /// `backup_pause: chassis` native backup stages its local copy before
    /// restic uploads it. Absent/empty = no staging.
    pub(crate) native_backup_staging_dir: Option<String>,
    /// fix-113 ADDENDUM: the staging cap, in MiB, before its 20% margin.
    pub(crate) native_backup_staging_cap_mib: Option<u64>,
    /// T1: directory the orchestrator writes per-stack Prometheus discovery
    /// files into. Absent = off, and the scrape list stays hand-maintained.
    pub(crate) metrics_targets_dir: Option<String>,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// dashboard's own address (CT 120), from which the firewall step of a
    /// deploy derives an inbound rule per stack for every tile that opens on
    /// that stack's own container. Absent or empty = feature off, and no
    /// rule is derived.
    pub(crate) tile_watch_source: Option<String>,
    /// trusted-sources (Kenny, 2026-10-22): addresses every stack firewall
    /// lets in on every port (`firewall::trusted_rules`). Absent = none.
    pub(crate) trusted_sources: Option<Vec<String>>,
    /// T69: how long a suspended step waits for an operator before giving
    /// up and answering `Unattended`. Long enough that Kenny can read the
    /// question and decide, short enough that a forgotten window does not
    /// hold the global op lock all night — the lock is held for the whole
    /// operation, so a question nobody answers blocks every other one.
    pub(crate) ask_timeout_s: Option<u64>,
    /// Decision "23 constants" (Kenny, 2026-09-30): how long a container's
    /// updates may stand before `homelab check` says so. Default 7 days.
    pub(crate) patch_threshold_s: Option<u64>,
    /// How stale a stack's backup may be before it counts as a finding.
    /// Default 48 hours.
    pub(crate) backup_max_age_s: Option<u64>,
    /// How old the daemon's own state backup (vault, state.json, TLS,
    /// host.toml) may be before it counts as a finding. Default 48 hours.
    pub(crate) host_meta_max_age_s: Option<u64>,
    /// How often the nightly report repeats an unchanged set of findings.
    /// Default 7 days.
    pub(crate) nightly_report_repeat_s: Option<u64>,
    /// fix-131: incident bundles older than this many days are pruned.
    /// Default 90.
    pub(crate) incident_bundle_max_age_days: Option<u64>,
    /// fix-131: at most this many incident bundles are kept. Default 200.
    pub(crate) incident_bundle_max_count: Option<usize>,
    /// gap-26: `journal.jsonl` is cut back to half of this many bytes once
    /// it grows past it (`compact_journal_file`). Default 4 MiB
    /// (`incidents::JOURNAL_MAX_BYTES`, see REGISTER.md).
    pub(crate) journal_max_bytes: Option<u64>,
    /// fix-122 (AR15's JSONL ring, Kenny's go 2026-10-01): size cap in bytes
    /// of `logs/host.jsonl`, the daemon's own trace ring — cut back to half
    /// of it, the same way `journal.jsonl` is, once it grows past it.
    /// Default [`homelab_core::logring::LOG_RING_MAX_BYTES`].
    pub(crate) log_ring_max_bytes: Option<u64>,
    /// fix-122: a `tracing`/`EnvFilter` directive (e.g. "info", "debug",
    /// "homelab_host=debug,info"), applied to both the journald sink and the
    /// JSONL ring. Default "info". Editable from the dashboard and applied
    /// live — no restart — unlike `log_ring_max_bytes` above it.
    pub(crate) log_level: Option<String>,
    /// How long one GitHub answer about a pinned upstream stands. Default
    /// 20 hours.
    pub(crate) upstream_max_age_s: Option<u64>,
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
    pub(crate) backup_concurrency: Option<usize>,
    /// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): the ZFS
    /// dataset that holds the second repository set, e.g. `HDD4TB/restic`.
    /// Absent = no second copy (the nightly `restic check` of the Google
    /// Drive copy runs either way).
    pub(crate) second_copy_dataset: Option<String>,
    /// backup-stall (correction 2026-10-22): seconds a backup snapshot or
    /// upload may go without progress before it is stopped. Default 600.
    pub(crate) backup_stall_s: Option<u64>,
    /// fix-96: how often one repository's check also reads a slice of its
    /// data. Default 30 days (Kenny: monthly).
    pub(crate) integrity_data_read_interval_s: Option<u64>,
    /// feat-platform-2 (homelab-admin, 2026-09-28): seconds between two
    /// readings of every container's real status. Default 60; at least 10.
    pub(crate) status_interval_s: Option<u64>,
    /// feat-platform-3: how many of the newest operation lines the host
    /// keeps in memory for a client that connects mid-operation. Default 2000.
    pub(crate) recent_lines: Option<usize>,
    /// arch-history: days of history.jsonl kept (default 90) and its size
    /// ceiling in MiB (default 16); past the ceiling the oldest half goes.
    pub(crate) history_days: Option<u64>,
    pub(crate) history_max_mib: Option<usize>,
    /// Decision notify-detail (2026-09-30): the dashboard's public address,
    /// for the link a push carries (`click_url`). Unset: no link.
    pub(crate) dashboard_url: Option<String>,
    /// replace-kuma (2026-09-30): the dashboard's health address, asked every
    /// minute; five minutes without an answer is an urgent notice, as the
    /// dashboard does for the host. Unset: not watched.
    pub(crate) watch_url: Option<String>,
    /// Owner decision "default plus per tile" (2026-09-30): how often the
    /// host asks `watch_url`, and the fleet default the `Tiles` RPC hands
    /// the dashboard for its own minute watch. Default 60.
    pub(crate) watch_interval_s: Option<u64>,
    /// How long `watch_url` (or, by fleet default, a tile) may fail before
    /// it counts as down. Default 300.
    pub(crate) watch_down_after_s: Option<u64>,
    /// fix-184 (2026-10-02): the stack `watch_url` answers for, declared
    /// here rather than guessed — while the host runs install-native,
    /// update-native, deploy or an adopt on this one stack, `watch_url`'s
    /// own down notice is suppressed, the same as it already is for a
    /// self-update or a host restart. Unset: no stack's own deploy ever
    /// suppresses it, only a self-update or host restart does.
    pub(crate) watch_stack: Option<String>,
    /// rule-20 (disk-audit, 2026-10-01): `[capacity_thresholds]` — warn and
    /// critical percentages for pve's root fs, the local-lvm thin pool, every
    /// ZFS pool, pve's journald and Prometheus' TSDB. Absent = the audit's
    /// own defaults (`HostCapacityThresholds::default()`).
    pub(crate) capacity_thresholds: Option<homelab_core::ops::fleetcheck::HostCapacityThresholds>,
    /// rule-20: the configured `--storage.tsdb.retention.size` of whatever
    /// runs Prometheus, in MiB — Kenny's own number, matching what the
    /// stack's compose file declares; core has no business knowing it.
    /// Absent (with `prometheus_url`) = the question is not asked, same as
    /// every other Prometheus-backed reading in this daemon.
    pub(crate) tsdb_retention_size_mib: Option<u64>,
}

/// Y1: how many backups actually run at once, given what the config says.
///
/// Zero is the case worth guarding. `backup_concurrency = 0` in host.toml
/// reads like "no limit" to the person typing it and means "run none" to the
/// stream that consumes it — so a typo meant to go faster would silently
/// produce a night with no backups at all, and the only sign would be
/// `last_backup` never moving. Clamped to at least one: slow is recoverable,
/// silent is not.
pub(crate) fn effective_concurrency(configured: usize) -> usize {
    configured.max(1)
}

/// fix-240: ten minutes (`homelab_core::ask::DEFAULT_WAIT_S`). Two minutes
/// ran out before the person the question was for had seen it.
pub(crate) fn default_ask_timeout_s() -> u64 {
    homelab_core::ask::DEFAULT_WAIT_S
}

/// Kenny's choice (form Y4, 2026-09-02): three at a time. About a third of
/// the wait, and never more than three services briefly quiet at 04:00.
pub(crate) fn default_backup_concurrency() -> usize {
    3
}

/// A day. The coverage question is asked nightly and on demand, so a stack
/// that has shipped nothing in twenty-four hours has genuinely stopped —
/// while an hour of quiet is normal for a service nobody browsed.
pub(crate) fn default_logs_window() -> String {
    "24h".into()
}

/// Only `<digits><unit>` reaches the query string. The window is pasted into
/// a hand-built URL, and a value out of a config file is not a value to trust
/// with that: anything else falls back to the default rather than producing a
/// query that silently means something other than it says.
pub(crate) fn sane_window(w: &str) -> String {
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
