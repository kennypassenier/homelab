//! feat-settings-1 (homelab-admin, 2026-09-28): every key of `host.toml`,
//! described once, for the dashboard's settings page and for the host that
//! answers it.
//!
//! The host reads the file (`GetHostConfig`) and writes it
//! (`SetHostConfig`); the dashboard draws one field per key. Both sides
//! consult this table, so the host refuses at its trust boundary what the
//! dashboard already refuses in the browser:
//!
//! * a **secret** is never sent, only whether it is set;
//! * a key that can cut the dashboard off from the host (`token`, `tokens`,
//!   `listen`, `state_dir`) is read-only in the browser (arch-self);
//! * `tokens` goes further still: it is generated and kept by the host
//!   itself, so it is also **host-held** (`Access::HostHeld`, fix-170) —
//!   never declared in `config/host.toml`, kept from the host's current
//!   file across a `homelab host apply`, and skipped by the drift check,
//!   the same shape a secret already has for those three things;
//! * a policy the token must not be able to loosen (`exec_enabled`, the
//!   no-touch list, the privileged-container and data-mount lists) stays
//!   ssh-only, as it always was;
//! * a key that can take the dashboard's own route or the backups down is
//!   editable only with a second, typed confirmation.
//!
//! fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01):
//! `Access::DashboardSecret` is the same "never sent, only whether it is
//! set" promise as `Access::Secret`, but the dashboard may WRITE it — the
//! field is a one-way drop box, never a read, the same shape the vault
//! already uses for `latch_secrets`. A key stays plain `Secret` (ssh only,
//! both ways) unless something — here, "the nightly round needs a
//! Cloudflare token but Kenny should not have to ssh in to set one" —
//! argues for the dashboard field specifically.
//!
//! Zero I/O: values travel as JSON, the TOML side is the host's.

use serde::Serialize;
use std::collections::BTreeMap;

/// Who may change a key from the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Edited in the browser like any field.
    Browser,
    /// Edited in the browser after the key's name is typed a second time
    /// (arch-self: it can cut off the dashboard's route or the backups).
    Confirm,
    /// Shown, never changed from the browser: it can cut the dashboard off
    /// from the host (arch-self).
    Locked,
    /// Shown, changed over ssh only: a policy the token must not loosen.
    SshOnly,
    /// Never shown, never sent, never changed from the browser: ssh only,
    /// both ways.
    Secret,
    /// Never shown, never sent — like `Secret` — but WRITABLE from the
    /// browser: a one-way field, set or replaced here, never read back.
    /// fix-143.
    DashboardSecret,
    /// Shown, changed over ssh only — like `Locked` — but, unlike every
    /// other key, generated and kept by the host itself: it must never be
    /// declared in `config/host.toml` at all (fix-170). The drift check
    /// skips it the same way it skips a secret, and `homelab host apply`
    /// refuses a `config/host.toml` that sets it, keeping the host's
    /// current value untouched instead.
    HostHeld,
}

impl Access {
    pub fn editable(self) -> bool {
        matches!(
            self,
            Access::Browser | Access::Confirm | Access::DashboardSecret
        )
    }
}

/// When a change takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Apply {
    /// At once: the host holds these in its live settings (G8).
    Live,
    /// At the host's next start.
    Restart,
}

/// What a value looks like, for the field and for the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Kind {
    /// A whole number between `min` and `max`.
    Int {
        min: u64,
        max: u64,
    },
    Bool,
    /// One line of text.
    Text,
    /// An http(s) address.
    Url,
    /// A vmid (100 to 999999999, as Proxmox allows; the fleet uses 100-999).
    Vmid,
    /// A list of vmids.
    VmidList,
    /// A list of text values.
    TextList,
    /// `<digits><s|m|h|d|w>`, e.g. `24h`.
    Window,
    /// A table or a list of tables, edited as a TOML fragment.
    Table,
}

/// One key of host.toml.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct KeyInfo {
    pub key: &'static str,
    /// A heading on the page.
    pub group: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    /// What the host uses when the key is absent.
    pub default: &'static str,
    pub kind: Kind,
    pub access: Access,
    pub apply: Apply,
}

/// One row of the table, one argument per column.
#[allow(clippy::too_many_arguments)]
const fn k(
    key: &'static str,
    group: &'static str,
    label: &'static str,
    help: &'static str,
    default: &'static str,
    kind: Kind,
    access: Access,
    apply: Apply,
) -> KeyInfo {
    KeyInfo {
        key,
        group,
        label,
        help,
        default,
        kind,
        access,
        apply,
    }
}

const U32: u64 = u32::MAX as u64;

/// Every key the host reads, in the order the page lists them.
pub const KEYS: &[KeyInfo] = &[
    // ── Access to the host ──────────────────────────────────────────────
    k(
        "token",
        "Access to the host",
        "Legacy token",
        "The single legacy bearer token (scope all). Changed over ssh only.",
        "none: HOMELAB_TOKEN",
        Kind::Text,
        Access::Secret,
        Apply::Restart,
    ),
    // arch-self: the dashboard's own token is in this table; fix-170: the
    // host generates it and it never lives in config/host.toml.
    k(
        "tokens",
        "Access to the host",
        "Scoped tokens",
        "One entry per machine: name, scope and the SHA-256 of its token. The dashboard's own token is one of them, so it is never changed from here; the table is generated by the host itself and never lives in `config/host.toml`.",
        "none",
        Kind::Table,
        Access::HostHeld,
        Apply::Restart,
    ),
    // arch-self: a change here cuts the dashboard's route off.
    k(
        "listen",
        "Access to the host",
        "Listen address",
        "Where the host accepts the line. Changing it cuts the dashboard off.",
        "0.0.0.0:8443",
        Kind::Text,
        Access::Locked,
        Apply::Restart,
    ),
    // arch-self: a change here cuts the dashboard's route off.
    k(
        "state_dir",
        "Access to the host",
        "State directory",
        "Where the host keeps state, vault and TLS. Changing it cuts the dashboard off.",
        "/var/lib/homelab",
        Kind::Text,
        Access::Locked,
        Apply::Restart,
    ),
    k(
        "exec_enabled",
        "Access to the host",
        "Remote exec",
        "Whether `homelab exec` may run commands in containers. A policy the token must not loosen: ssh only.",
        "false",
        Kind::Bool,
        Access::SshOnly,
        Apply::Restart,
    ),
    // ── Nightly round and backups ───────────────────────────────────────
    k(
        "backup_hour",
        "Nightly round and backups",
        "Nightly hour",
        "Hour (0-23, host time) of the nightly backup and update round; empty turns it off.",
        "off",
        Kind::Int { min: 0, max: 23 },
        Access::Browser,
        Apply::Live,
    ),
    k(
        "retention",
        "Nightly round and backups",
        "Snapshot retention",
        "Tiers of snapshots kept, e.g. daily for a week, then every two weeks for two months.",
        "1 day for 7 days, 14 days for 60 days, 60 days for ever",
        Kind::Table,
        Access::Browser,
        Apply::Live,
    ),
    k(
        "backup_concurrency",
        "Nightly round and backups",
        "Backups at once",
        "How many stack backups the nightly round runs at the same time; each pauses its containers.",
        "3",
        Kind::Int { min: 1, max: 16 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "restic_base",
        "Nightly round and backups",
        "Backup target",
        "Where restic writes every repository. A wrong value stops every backup.",
        "rclone:gdrive:homelab-backups",
        Kind::Text,
        Access::Confirm,
        Apply::Restart,
    ),
    k(
        "restic_password_file",
        "Nightly round and backups",
        "Restic password file",
        "Path on pve of the file holding the restic password. A wrong value stops every backup.",
        "/var/lib/homelab/secrets/restic.pw",
        Kind::Text,
        Access::Confirm,
        Apply::Restart,
    ),
    k(
        "restic_snapshot_timeout_s",
        "Nightly round and backups",
        "Snapshot time limit",
        "Seconds one snapshot may take.",
        "14400 (4 h)",
        Kind::Int { min: 60, max: U32 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "restic_restore_timeout_s",
        "Nightly round and backups",
        "Restore time limit",
        "Seconds one restore may take.",
        "14400 (4 h)",
        Kind::Int { min: 60, max: U32 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "second_copy_dataset",
        "Nightly round and backups",
        "Second copy dataset",
        "ZFS dataset holding the second repository set, e.g. HDD4TB/restic; empty: no second copy.",
        "none",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "integrity_data_read_interval_s",
        "Nightly round and backups",
        "Data read interval",
        "How often a repository's check also reads a slice of its data, in seconds.",
        "2592000 (30 days)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "restore_drill_interval_s",
        "Nightly round and backups",
        "Restore drill interval",
        "How long a passed restore drill counts for, in seconds.",
        "72000 (20 h)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    // fix-62: a data pool, not the root disk — same reason as the backup
    // staging directory. Story: docs/deployment/REGISTER.md.
    k(
        "restore_drill_scratch_dir",
        "Nightly round and backups",
        "Restore drill scratch directory",
        "Where the nightly restore drill restores a repository to, so it can be judged by what actually comes back. A directory on a data pool, not the root disk — emptied before and after every drill, so nothing it leaves behind can fill the disk.",
        "/appdata/.restore-scratch",
        Kind::Text,
        Access::Browser,
        Apply::Live,
    ),
    k(
        "zfs_jobs",
        "Nightly round and backups",
        "ZFS replication jobs",
        "Snapshot and replication jobs: source and target dataset each.",
        "none",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "device_backups",
        "Nightly round and backups",
        "Device backups",
        "Devices that hand over their own configuration once a night.",
        "none",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "watched_backups",
        "Nightly round and backups",
        "Watched backups",
        "Backups other devices make that the host watches for age.",
        "none",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "mirror_remote",
        "Nightly round and backups",
        "Intent mirror remote",
        "Git remote the host mirrors its intent repository to; empty: off.",
        "off",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    // fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): where a
    // `backup_pause: chassis` native backup stages its local copy.
    k(
        "native_backup_staging_dir",
        "Nightly round and backups",
        "Chassis backup staging directory",
        "Where a chassis-paused native backup (backup_pause: chassis) tars its data locally before restic uploads it, so the write-pause only lasts as long as the local copy, not the upload. A directory on a data pool, not the root disk. \"none\": no staging — chassis-paused backups tar straight to restic under a renewed pause.",
        "/appdata/.backup-staging",
        Kind::Text,
        Access::Browser,
        Apply::Live,
    ),
    k(
        "native_backup_staging_cap_mib",
        "Nightly round and backups",
        "Chassis backup staging cap",
        "Largest one staged tar may be, in MiB, before a 20% safety margin; a copy that would not fit (that margin, or the directory's free space) skips staging for that run and backs up live under the renewed pause instead.",
        "10240 (10 GiB)",
        Kind::Int { min: 1, max: U32 },
        Access::Browser,
        Apply::Live,
    ),
    // ── Notifications ───────────────────────────────────────────────────
    k(
        "notify_webhook",
        "Notifications",
        "Notification webhook",
        "Where the host posts a notification after each operation; empty: off.",
        "off",
        Kind::Url,
        Access::Browser,
        Apply::Live,
    ),
    k(
        "notify_auth_bearer",
        "Notifications",
        "Webhook token",
        "The bearer token sent with it. A secret: changed over ssh only.",
        "none",
        Kind::Text,
        Access::Secret,
        Apply::Restart,
    ),
    // fix-191: a secret, never declared in the public repository.
    k(
        "notify_fallback_webhook",
        "Notifications",
        "Fallback webhook",
        "The second route, tried when the first does not answer 2xx. A secret: \
         its URL carries a Home Assistant webhook id, so it is changed over ssh only and \
         never declared in the public repository.",
        "none",
        Kind::Url,
        Access::Secret,
        Apply::Restart,
    ),
    k(
        "notify_fallback_auth_bearer",
        "Notifications",
        "Fallback token",
        "The bearer token of the second route. A secret: changed over ssh only.",
        "none",
        Kind::Text,
        Access::Secret,
        Apply::Restart,
    ),
    k(
        "notify_tls_cert",
        "Notifications",
        "Pinned hub certificate",
        "Path to the LAN self-signed certificate (PEM) an https:// notify route is pinned to. Not a secret — the fingerprint below is what proves it.",
        "none",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "notify_tls_fingerprint",
        "Notifications",
        "Pinned hub fingerprint",
        "SHA-256 of that certificate. Read it again from the container (openssl x509 -noout -fingerprint -sha256) whenever the certificate is regenerated.",
        "none",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    // ── Nightly checks ──────────────────────────────────────────────────
    k(
        "cloudflare_token",
        "Nightly checks",
        "Cloudflare read-only token",
        "Lets the host's own nightly round compare the Cloudflare edge against captured/gateway/, the same comparison `homelab check` runs from the workstation. Read-only token; set or replaced here (never read back) or over ssh. Absent: the nightly comparison reports \"not configured\", not broken.",
        "none",
        Kind::Text,
        Access::DashboardSecret,
        Apply::Restart,
    ),
    k(
        "dashboard_url",
        "Notifications",
        "Dashboard address",
        "The dashboard's public address; a push links to its page there (click_url).",
        "none: a push carries no link",
        Kind::Url,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "watch_url",
        "Notifications",
        "Dashboard health address",
        "The dashboard's health address on the house network; the host asks it every minute and sends an urgent notice after five minutes without an answer.",
        "none: the dashboard is not watched",
        Kind::Url,
        Access::Browser,
        Apply::Restart,
    ),
    // fix-184: the down notice is held back while the stack is worked on.
    k(
        "watch_stack",
        "Notifications",
        "Stack behind the health address",
        "The stack watch_url answers for. While the host deploys, installs, updates or adopts that stack, the watch's down notice is held back; a self-update or host restart always holds it.",
        "none: only a self-update or host restart holds it back",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "watch_interval_s",
        "Notifications",
        "Watch interval",
        "How often the host asks the dashboard's health address, and the fleet default for how often the dashboard's own minute watch asks a tile (a tile may set its own in its stack file).",
        "60",
        Kind::Int {
            min: 10,
            max: 86_400,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "watch_down_after_s",
        "Notifications",
        "Watch down-after",
        "How long the dashboard's health address (or a tile, by fleet default) may fail before it counts as down.",
        "300",
        Kind::Int {
            min: 10,
            max: 86_400,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "ask_timeout_s",
        "Notifications",
        "Question wait",
        "Seconds a suspended step waits for an answer before it fails; the question is pushed as a notice when it is asked.",
        "600",
        Kind::Int {
            min: 10,
            max: 86_400,
        },
        Access::Browser,
        Apply::Restart,
    ),
    // ── Coverage and monitoring ─────────────────────────────────────────
    k(
        "prometheus_url",
        "Coverage and monitoring",
        "Prometheus",
        "Where the coverage check asks whether a stack is measured; empty: not asked.",
        "not asked",
        Kind::Url,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "loki_url",
        "Coverage and monitoring",
        "Loki",
        "Where the coverage check asks whether a stack's logs arrive; empty: not asked.",
        "not asked",
        Kind::Url,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "loki_vmid",
        "Coverage and monitoring",
        "Loki container",
        "The container Loki runs in; the log question is asked from inside it.",
        "asked at the Loki address from pve",
        Kind::Vmid,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "logs_window",
        "Coverage and monitoring",
        "Log window",
        "How far back the log-coverage question looks, e.g. 24h.",
        "24h",
        Kind::Window,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "status_interval_s",
        "Coverage and monitoring",
        "Status interval",
        "Seconds between two readings of every container's real status.",
        "60",
        Kind::Int {
            min: 10,
            max: 86_400,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "recent_lines",
        "Coverage and monitoring",
        "Lines kept",
        "How many of the newest operation lines the host keeps for a client that connects mid-operation.",
        "2000",
        Kind::Int {
            min: 100,
            max: 100_000,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "history_days",
        "Coverage and monitoring",
        "History days",
        "Days of operation history kept.",
        "90",
        Kind::Int { min: 1, max: 3650 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "history_max_mib",
        "Coverage and monitoring",
        "History size",
        "Largest size of the history file in MiB; past it the oldest half goes.",
        "16",
        Kind::Int { min: 1, max: 1024 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "metrics_targets_dir",
        "Coverage and monitoring",
        "Prometheus targets directory",
        "Where per-stack scrape targets are written; empty: off.",
        "off",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "tile_watch_source",
        "Coverage and monitoring",
        "Tile watch source address",
        "The dashboard's own address, from which its once-a-minute tile watch reaches every stack; a deploy derives an inbound firewall rule per stack from its tiles for this source. Empty: no rule is derived.",
        "off",
        Kind::Text,
        Access::Browser,
        Apply::Restart,
    ),
    // ── Gateway and network ─────────────────────────────────────────────
    k(
        "gateway_vmid",
        "Gateway and network",
        "Gateway container",
        "The container Traefik runs in. The dashboard's own route lives there: a wrong value cuts it off from the browser.",
        "104",
        Kind::Vmid,
        Access::Confirm,
        Apply::Restart,
    ),
    k(
        "gateway_routes_dir",
        "Gateway and network",
        "Gateway routes directory",
        "Where route files are written in the gateway. A wrong value drops the dashboard's own route.",
        "/opt/traefik-config/routes",
        Kind::Text,
        Access::Confirm,
        Apply::Restart,
    ),
    k(
        "registry_cache",
        "Gateway and network",
        "Registry cache",
        "The pull-through image cache and its upstreams; empty: images come from their own registry.",
        "none",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "default_log_rotation",
        "Gateway and network",
        "Default log rotation",
        "rule-20: the fleet default for a data mount that declares no rotate: of its own and does not opt out — files pattern (e.g. *.log), size cap and copies kept. Empty: only a stack's own declared rotate: rotates anything, as before.",
        "*.log, 50M, keep 5",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    // ── Thresholds and limits ───────────────────────────────────────────
    k(
        "patch_threshold_s",
        "Thresholds and limits",
        "Patch threshold",
        "How long a container's updates may stand before `homelab check` says so.",
        "604800 (7 days)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "backup_max_age_s",
        "Thresholds and limits",
        "Backup max age",
        "How stale a stack's backup may be before it counts as a finding.",
        "172800 (48 h)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "host_meta_max_age_s",
        "Thresholds and limits",
        "Host-meta backup max age",
        "How old the daemon's own state backup (vault, state.json, TLS, host.toml) may be before it counts as a finding.",
        "172800 (48 h)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "nightly_report_repeat_s",
        "Thresholds and limits",
        "Nightly report repeat",
        "How often the nightly report repeats an unchanged set of findings.",
        "604800 (7 days)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "incident_bundle_max_age_days",
        "Thresholds and limits",
        "Incident bundle max age",
        "Incident bundles older than this many days are pruned.",
        "90",
        Kind::Int { min: 1, max: 3650 },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "incident_bundle_max_count",
        "Thresholds and limits",
        "Incident bundle max count",
        "At most this many incident bundles are kept, newest first.",
        "200",
        Kind::Int {
            min: 1,
            max: 100_000,
        },
        Access::Browser,
        Apply::Restart,
    ),
    // gap-26: the journal's size cap.
    k(
        "journal_max_bytes",
        "Thresholds and limits",
        "Journal max size",
        "journal.jsonl is cut back to half of this many bytes once it grows past it; the last line of any still-running operation is kept, so the interrupted-operation report reads the same after the cut.",
        "4194304 (4 MiB)",
        Kind::Int {
            min: 65_536,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "upstream_max_age_s",
        "Thresholds and limits",
        "Upstream release check age",
        "How long one GitHub answer about a pinned upstream stands before it is asked again.",
        "72000 (20 h)",
        Kind::Int {
            min: 3600,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "capacity_thresholds",
        "Thresholds and limits",
        "Capacity thresholds",
        "rule-20: warn/critical percentages for pve's own root filesystem, the local-lvm thin pool (data and metadata separately), every ZFS pool, pve's journald against its own cap, Prometheus' TSDB against its retention.size, and the native-backup staging directory against its own cap.",
        "70/85, 70/85, 50/70, 80/90, 80/95, 70/90, 10/50",
        Kind::Table,
        Access::Browser,
        Apply::Restart,
    ),
    k(
        "tsdb_retention_size_mib",
        "Thresholds and limits",
        "Prometheus TSDB cap",
        "rule-20: the --storage.tsdb.retention.size Prometheus is actually configured with, in MiB — match the stack's own compose file. Needs prometheus_url set too. Empty: the TSDB threshold is not asked.",
        "not asked",
        Kind::Int { min: 1, max: U32 },
        Access::Browser,
        Apply::Restart,
    ),
    // ── Logging ──────────────────────────────────────────────────────────
    k(
        "log_level",
        "Logging",
        "Log level",
        "A tracing/EnvFilter directive, e.g. \"info\", \"debug\", or \"homelab_host=debug,info\" for one module only. Applied live to the journal and the JSONL ring below — no restart.",
        "info",
        Kind::Text,
        Access::Browser,
        Apply::Live,
    ),
    k(
        "log_ring_max_bytes",
        "Logging",
        "Log ring max size",
        "logs/host.jsonl, the daemon's own JSONL trace ring, is cut back to half of this many bytes once it grows past it — same shape as journal_max_bytes above, a separate file.",
        "4194304 (4 MiB)",
        Kind::Int {
            min: 65_536,
            max: U32,
        },
        Access::Browser,
        Apply::Restart,
    ),
    // ── Safety (ssh only) ───────────────────────────────────────────────
    k(
        "no_touch",
        "Safety",
        "No-touch list",
        "Guests the host never touches; the file can only add to the compiled list. ssh only.",
        "100, 101, 102, 103",
        Kind::VmidList,
        Access::SshOnly,
        Apply::Restart,
    ),
    k(
        "privileged_vmids",
        "Safety",
        "Privileged containers",
        "Containers a deploy may make privileged. ssh only.",
        "the fleet's own",
        Kind::VmidList,
        Access::SshOnly,
        Apply::Restart,
    ),
    k(
        "data_mount_roots",
        "Safety",
        "Data mount roots",
        "Host directories a stack's data mounts may borrow. ssh only.",
        "the fleet's own",
        Kind::TextList,
        Access::SshOnly,
        Apply::Restart,
    ),
];

/// The row of one key.
pub fn key_info(key: &str) -> Option<&'static KeyInfo> {
    KEYS.iter().find(|k| k.key == key)
}

/// Keys whose value is never sent anywhere.
pub fn is_secret(key: &str) -> bool {
    key_info(key).is_some_and(|k| matches!(k.access, Access::Secret | Access::DashboardSecret))
}

/// Keys generated and kept by the host itself: shown and sent like any
/// other key, but never declared in `config/host.toml` (fix-170).
pub fn is_host_held(key: &str) -> bool {
    key_info(key).is_some_and(|k| matches!(k.access, Access::HostHeld))
}

/// fix-181: the value the host actually runs with when `key` is unset,
/// where that value is unambiguous enough to compare against a declared
/// one — used by `evaluate_host_config_drift` so a key left at its default
/// on one side and spelled out to the same effect on the other reads as
/// "no difference", not as drift.
///
/// One rule per `Kind`, not one case per key (`KEYS` already carries that
/// distinction): a number's default is the leading number in its `default`
/// text (the rest is documentation — "2592000 (30 days)"), a bool's is its
/// literal `"true"`/`"false"`, and a single-word text default is itself
/// ("info", "/var/lib/homelab") — unless that word is a sentinel for "not
/// set" ("none", "off"), which is already what `None` means and gives
/// nothing new to compare against. Anything with more structure than one
/// token (a table, a list, a sentence like "not asked" or "the fleet's
/// own") has no single value worth asserting here, so it is left alone and
/// the comparison falls back to the raw presence check it always had.
pub fn default_effective(key: &str) -> Option<serde_json::Value> {
    let info = key_info(key)?;
    match info.kind {
        Kind::Bool => match info.default {
            "true" => Some(serde_json::Value::Bool(true)),
            "false" => Some(serde_json::Value::Bool(false)),
            _ => None,
        },
        Kind::Int { .. } | Kind::Vmid => {
            let token = info.default.split_whitespace().next()?;
            let n: u64 = token.parse().ok()?;
            Some(serde_json::Value::Number(n.into()))
        }
        Kind::Text | Kind::Url | Kind::Window => {
            let token = info.default.trim();
            if token.is_empty() || token.contains(char::is_whitespace) {
                return None;
            }
            if matches!(token, "none" | "off") {
                return None;
            }
            Some(serde_json::Value::String(token.to_string()))
        }
        _ => None,
    }
}

/// Why `value` is not a value for `key`, or Ok. `null` is always allowed
/// for an editable key: it removes the key, and the host takes its default.
/// Tables are checked by the host against its own types.
///
/// Browser-only gate (`Access::editable`) plus the shape check
/// ([`check_shape`]). A caller whose own path already decided a key may be
/// changed — `homelab host apply` reading `config/host.toml`, or the
/// dashboard's declarative commit of it — checks shape alone, since that
/// whole-file apply is the ssh-equivalent path `Locked`/`SshOnly` keys name
/// as their real route (arch-self; fix-110).
pub fn check_value(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(info) = key_info(key) else {
        return Err(format!("{key} is not a setting the host reads"));
    };
    if !info.access.editable() {
        return Err(match info.access {
            // arch-self: a Locked key is the dashboard's own route.
            Access::Locked => format!(
                "{key} can cut the dashboard off from the host and is changed over ssh only"
            ),
            Access::SshOnly => format!("{key} is a safety policy and is changed over ssh only"),
            Access::HostHeld => format!(
                "{key} is generated and kept by the host itself, never declared in config/host.toml, and is changed over ssh only"
            ),
            _ => format!("{key} is a secret and is changed over ssh only"),
        });
    }
    check_shape(key, value)
}

/// Why `value` is not shaped like `key`'s kind, or Ok — without the access
/// gate `check_value` adds. `null` is always allowed: it removes the key.
pub fn check_shape(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(info) = key_info(key) else {
        return Err(format!("{key} is not a setting the host reads"));
    };
    if value.is_null() {
        return Ok(());
    }
    let whole = |v: &serde_json::Value| v.as_u64();
    match info.kind {
        Kind::Int { min, max } => match whole(value) {
            Some(n) if (min..=max).contains(&n) => Ok(()),
            _ => Err(format!("{key} must be a whole number from {min} to {max}")),
        },
        Kind::Bool => value
            .is_boolean()
            .then_some(())
            .ok_or_else(|| format!("{key} must be true or false")),
        Kind::Text => match value.as_str() {
            Some(s) if !s.trim().is_empty() && !s.contains('\n') => Ok(()),
            _ => Err(format!("{key} must be one line of text (empty: remove it)")),
        },
        Kind::Url => match value.as_str() {
            Some(s)
                if (s.starts_with("http://") || s.starts_with("https://"))
                    && !s.contains(char::is_whitespace) =>
            {
                Ok(())
            }
            _ => Err(format!("{key} must be an http:// or https:// address")),
        },
        Kind::Vmid => match whole(value) {
            Some(n) if (100..=999_999_999).contains(&n) => Ok(()),
            _ => Err(format!("{key} must be a vmid (100 or more)")),
        },
        Kind::VmidList => match value.as_array() {
            Some(a) if a.iter().all(|v| whole(v).is_some_and(|n| n >= 100)) => Ok(()),
            _ => Err(format!("{key} must be a list of vmids")),
        },
        Kind::TextList => match value.as_array() {
            Some(a) if a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty())) => Ok(()),
            _ => Err(format!("{key} must be a list of text values")),
        },
        Kind::Window => match value.as_str() {
            Some(w) if valid_window(w) => Ok(()),
            _ => Err(format!(
                "{key} must be a number and a unit, e.g. 24h (s, m, h, d or w)"
            )),
        },
        Kind::Table => {
            if value.is_object() || value.is_array() {
                Ok(())
            } else {
                Err(format!("{key} must be a table or a list of tables"))
            }
        }
    }
}

/// `<digits><s|m|h|d|w>`, as the host accepts a log window.
pub fn valid_window(w: &str) -> bool {
    w.len() >= 2
        && w.chars().next().is_some_and(|c| c.is_ascii_digit())
        && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())
        && matches!(w.chars().last(), Some('s' | 'm' | 'h' | 'd' | 'w'))
}

/// A key with a value the host must not send: secrets become whether they
/// are set; a scoped token keeps its name and scope, never its hash.
pub fn redact(key: &str, value: &serde_json::Value) -> serde_json::Value {
    if is_secret(key) {
        return serde_json::Value::Null;
    }
    if key == "tokens"
        && let Some(list) = value.as_array()
    {
        return serde_json::Value::Array(
            list.iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.get("name").cloned().unwrap_or_default(),
                        "scope": t.get("scope").cloned().unwrap_or_default(),
                    })
                })
                .collect(),
        );
    }
    value.clone()
}

/// JSON as a TOML value; `null` inside a value has no TOML form (only a
/// whole key may be null, meaning "remove it", which the caller handles
/// before reaching here). Shared by the host's per-key `SetHostConfig` and
/// `config/host.toml`'s whole-file apply (fix-110), so both read the same
/// JSON the dashboard and `homelab host apply` send.
pub fn json_to_toml(v: &serde_json::Value) -> Result<toml::Value, String> {
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

/// fix-110: `raw` (a TOML document) with `changes` applied (a key → null
/// removes it), shape-checked key by key with [`check_shape`] — not
/// [`check_value`]'s access gate, which the caller's own path already
/// decided (see `check_value`'s doc). Returns the new pretty-printed TOML
/// text, or every reason it is refused. Pure: parsing and type-checking
/// only, no file I/O and no `FileConfig`/`startup_problems` validation —
/// the host runs those itself before it writes anything, since only the
/// host knows the full set of fields and their cross-field rules.
pub fn merge_changes(
    raw: &str,
    changes: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<String, String> {
    if changes.is_empty() {
        return Err("no change was sent".into());
    }
    let mut why = Vec::new();
    for (key, value) in changes {
        if let Err(e) = check_shape(key, value) {
            why.push(e);
        }
    }
    if !why.is_empty() {
        return Err(why.join("; "));
    }
    let mut table: toml::Table = if raw.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(raw).map_err(|e| format!("does not parse as TOML: {e}"))?
    };
    for (key, value) in changes {
        if value.is_null() {
            table.remove(key);
        } else {
            let v = json_to_toml(value).map_err(|e| format!("{key}: {e}"))?;
            table.insert(key.clone(), v);
        }
    }
    toml::to_string_pretty(&table).map_err(|e| e.to_string())
}

/// fix-110 (fix-170 extended it to host-held keys): `declared` (the
/// non-secret, non-host-held table `config/host.toml` holds) laid over
/// `current` (the host's own host.toml, parsed), keeping every secret and
/// every host-held key `current` already sets untouched — `declared` must
/// never carry one of either (checked here), since the repository is not
/// where a secret lives, and a host-held key (`tokens`) is not the
/// repository's to declare at all: the host itself generates and keeps it.
/// A key `current` sets that `declared` does not is dropped: the repository
/// is the whole declarative picture for everything but secrets and
/// host-held keys, the same rule `homelab apply` already keeps for a
/// stack's files. Returns the merged table, or why `declared` cannot be
/// applied. Pure.
pub fn apply_declared(
    declared: &toml::Table,
    current: &toml::Table,
) -> Result<toml::Table, String> {
    let secret_in_repo: Vec<&str> = declared
        .keys()
        .map(String::as_str)
        .filter(|k| is_secret(k))
        .collect();
    if !secret_in_repo.is_empty() {
        return Err(format!(
            "config/host.toml sets {} — a secret, which must never be in the repository",
            secret_in_repo.join(", ")
        ));
    }
    let host_held_in_repo: Vec<&str> = declared
        .keys()
        .map(String::as_str)
        .filter(|k| is_host_held(k))
        .collect();
    if !host_held_in_repo.is_empty() {
        return Err(format!(
            "config/host.toml sets {} — generated and kept by the host itself, and must never be declared in the repository",
            host_held_in_repo.join(", ")
        ));
    }
    let mut merged = declared.clone();
    for (key, value) in current {
        if is_secret(key) || is_host_held(key) {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(merged)
}

/// fix-191: one key a whole-file apply would move, for the operator to read
/// before the host writes it. Values as JSON; secrets and host-held keys
/// never appear (they are kept from the host's own file, never applied).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChange {
    pub key: String,
    /// The host's current value; `None`: the host does not set it.
    pub from: Option<serde_json::Value>,
    /// The declared value; `None`: the apply drops the key.
    pub to: Option<serde_json::Value>,
}

/// fix-191: every non-secret, non-host-held key whose value differs between
/// `current` (the host's file, as `GetHostConfig` reports it) and `declared`
/// (`config/host.toml`), sorted by key. Pure.
pub fn declared_changes(
    declared: &BTreeMap<String, serde_json::Value>,
    current: &BTreeMap<String, serde_json::Value>,
) -> Vec<KeyChange> {
    let keys: std::collections::BTreeSet<&String> = declared.keys().chain(current.keys()).collect();
    keys.into_iter()
        .filter(|k| !is_secret(k) && !is_host_held(k))
        .filter(|k| declared.get(*k) != current.get(*k))
        .map(|k| KeyChange {
            key: k.clone(),
            from: current.get(k).cloned(),
            to: declared.get(k).cloned(),
        })
        .collect()
}

/// fix-guards-7: the exit code of `homelab host diff` when the two
/// disagree; `make host-drift` tells it apart from an unreachable host
/// (any other non-zero code), which says nothing about drift.
pub const DRIFT_EXIT_CODE: i32 = 3;

/// fix-guards-7: what a host reports about its own binary for a drift
/// check: every key it reads, and the compiled default of each key that
/// has one (`GetHostConfig`'s `known_keys` and `defaults`). Pure.
pub fn host_facts() -> (Vec<String>, BTreeMap<String, serde_json::Value>) {
    let known = KEYS.iter().map(|k| k.key.to_string()).collect();
    let defaults = KEYS
        .iter()
        .filter_map(|k| default_effective(k.key).map(|v| (k.key.to_string(), v)))
        .collect();
    (known, defaults)
}

/// fix-guards-7: the changes that are real drift — a key whose EFFECTIVE
/// value differs on the RUNNING host. A key the host leaves unset while
/// the repository spells out its default (or the other way round) changes
/// nothing the host does, so it is not drift (the same rule as fix-181's
/// fleet check), and the default is the host binary's own (`host_defaults`)
/// where it reports one. A key the running host does not read
/// (`host_known`, when it reports the list) is a key of a later release:
/// it arrives with that release, not through `host apply`. Pure.
pub fn effective_drift(
    changes: Vec<KeyChange>,
    host_known: &[String],
    host_defaults: &BTreeMap<String, serde_json::Value>,
) -> Vec<KeyChange> {
    changes
        .into_iter()
        .filter(|c| host_known.is_empty() || host_known.iter().any(|k| k == &c.key))
        .filter(|c| {
            let d = host_defaults
                .get(&c.key)
                .cloned()
                .or_else(|| default_effective(&c.key));
            c.from.clone().or_else(|| d.clone()) != c.to.clone().or(d)
        })
        .collect()
}

/// fix-guards-7: one line per drifting key, the host's value first, as
/// `homelab host diff` prints them. Pure.
pub fn drift_lines(drift: &[KeyChange]) -> Vec<String> {
    let show = |v: &Option<serde_json::Value>| match v {
        None => "(not set)".to_string(),
        Some(v) => {
            let t = v.to_string();
            if t.chars().count() > 100 {
                format!("{}…", t.chars().take(100).collect::<String>())
            } else {
                t
            }
        }
    };
    drift
        .iter()
        .map(|c| {
            format!(
                "{}: the host runs {}, config/host.toml says {}",
                c.key,
                show(&c.from),
                show(&c.to)
            )
        })
        .collect()
}

/// fix-191: the keys `merged` would move away from `current` that `allow`
/// does not name — what `ApplyHostConfig` refuses. Pure.
pub fn unannounced_changes(
    merged: &toml::Table,
    current: &toml::Table,
    allow: &[String],
) -> Vec<String> {
    let keys: std::collections::BTreeSet<&String> = merged.keys().chain(current.keys()).collect();
    keys.into_iter()
        .filter(|k| merged.get(*k) != current.get(*k))
        .filter(|k| !allow.iter().any(|a| a == *k))
        .cloned()
        .collect()
}
