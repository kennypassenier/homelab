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
//! * a policy the token must not be able to loosen (`exec_enabled`, the
//!   no-touch list, the privileged-container and data-mount lists) stays
//!   ssh-only, as it always was;
//! * a key that can take the dashboard's own route or the backups down is
//!   editable only with a second, typed confirmation.
//!
//! Zero I/O: values travel as JSON, the TOML side is the host's.

use serde::Serialize;

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
    /// Never shown, never sent: only whether it is set.
    Secret,
}

impl Access {
    pub fn editable(self) -> bool {
        matches!(self, Access::Browser | Access::Confirm)
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
    k("token", "Access to the host", "Legacy token", "The single legacy bearer token (scope all). Changed over ssh only.", "none: HOMELAB_TOKEN", Kind::Text, Access::Secret, Apply::Restart),
    k("tokens", "Access to the host", "Scoped tokens", "One entry per machine: name, scope and the SHA-256 of its token. The dashboard's own token is one of them, so it is never changed from here (arch-self).", "none", Kind::Table, Access::Locked, Apply::Restart),
    k("listen", "Access to the host", "Listen address", "Where the host accepts the line. Changing it cuts the dashboard off (arch-self).", "0.0.0.0:8443", Kind::Text, Access::Locked, Apply::Restart),
    k("state_dir", "Access to the host", "State directory", "Where the host keeps state, vault and TLS. Changing it cuts the dashboard off (arch-self).", "/var/lib/homelab", Kind::Text, Access::Locked, Apply::Restart),
    k("exec_enabled", "Access to the host", "Remote exec", "Whether `homelab exec` may run commands in containers. A policy the token must not loosen: ssh only.", "false", Kind::Bool, Access::SshOnly, Apply::Restart),
    // ── Nightly round and backups ───────────────────────────────────────
    k("backup_hour", "Nightly round and backups", "Nightly hour", "Hour (0-23, host time) of the nightly backup and update round; empty turns it off.", "off", Kind::Int { min: 0, max: 23 }, Access::Browser, Apply::Live),
    k("retention", "Nightly round and backups", "Snapshot retention", "Tiers of snapshots kept, e.g. daily for a week, then every two weeks for two months.", "1 day for 7 days, 14 days for 60 days, 60 days for ever", Kind::Table, Access::Browser, Apply::Live),
    k("backup_concurrency", "Nightly round and backups", "Backups at once", "How many stack backups the nightly round runs at the same time; each pauses its containers.", "3", Kind::Int { min: 1, max: 16 }, Access::Browser, Apply::Restart),
    k("restic_base", "Nightly round and backups", "Backup target", "Where restic writes every repository. A wrong value stops every backup.", "rclone:gdrive:homelab-backups", Kind::Text, Access::Confirm, Apply::Restart),
    k("restic_password_file", "Nightly round and backups", "Restic password file", "Path on pve of the file holding the restic password. A wrong value stops every backup.", "/var/lib/homelab/secrets/restic.pw", Kind::Text, Access::Confirm, Apply::Restart),
    k("restic_snapshot_timeout_s", "Nightly round and backups", "Snapshot time limit", "Seconds one snapshot may take.", "14400 (4 h)", Kind::Int { min: 60, max: U32 }, Access::Browser, Apply::Restart),
    k("restic_restore_timeout_s", "Nightly round and backups", "Restore time limit", "Seconds one restore may take.", "14400 (4 h)", Kind::Int { min: 60, max: U32 }, Access::Browser, Apply::Restart),
    k("second_copy_dataset", "Nightly round and backups", "Second copy dataset", "ZFS dataset holding the second repository set, e.g. HDD4TB/restic; empty: no second copy.", "none", Kind::Text, Access::Browser, Apply::Restart),
    k("integrity_data_read_interval_s", "Nightly round and backups", "Data read interval", "How often a repository's check also reads a slice of its data, in seconds.", "2592000 (30 days)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("restore_drill_interval_s", "Nightly round and backups", "Restore drill interval", "How long a passed restore drill counts for, in seconds.", "72000 (20 h)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("zfs_jobs", "Nightly round and backups", "ZFS replication jobs", "Snapshot and replication jobs: source and target dataset each.", "none", Kind::Table, Access::Browser, Apply::Restart),
    k("device_backups", "Nightly round and backups", "Device backups", "Devices that hand over their own configuration once a night.", "none", Kind::Table, Access::Browser, Apply::Restart),
    k("watched_backups", "Nightly round and backups", "Watched backups", "Backups other devices make that the host watches for age.", "none", Kind::Table, Access::Browser, Apply::Restart),
    k("mirror_remote", "Nightly round and backups", "Intent mirror remote", "Git remote the host mirrors its intent repository to; empty: off.", "off", Kind::Text, Access::Browser, Apply::Restart),
    // ── Notifications ───────────────────────────────────────────────────
    k("notify_webhook", "Notifications", "Notification webhook", "Where the host posts a notification after each operation; empty: off.", "off", Kind::Url, Access::Browser, Apply::Live),
    k("notify_auth_bearer", "Notifications", "Webhook token", "The bearer token sent with it. A secret: changed over ssh only.", "none", Kind::Text, Access::Secret, Apply::Restart),
    k("notify_fallback_webhook", "Notifications", "Fallback webhook", "The second route, tried when the first does not answer 2xx.", "none", Kind::Url, Access::Browser, Apply::Restart),
    k("notify_fallback_auth_bearer", "Notifications", "Fallback token", "The bearer token of the second route. A secret: changed over ssh only.", "none", Kind::Text, Access::Secret, Apply::Restart),
    k("dashboard_url", "Notifications", "Dashboard address", "The dashboard's public address; a push links to its page there (click_url).", "none: a push carries no link", Kind::Url, Access::Browser, Apply::Restart),
    k("watch_url", "Notifications", "Dashboard health address", "The dashboard's health address on the house network; the host asks it every minute and sends an urgent notice after five minutes without an answer.", "none: the dashboard is not watched", Kind::Url, Access::Browser, Apply::Restart),
    k("watch_interval_s", "Notifications", "Watch interval", "How often the host asks the dashboard's health address, and the fleet default for how often the dashboard's own minute watch asks a tile (a tile may set its own in its stack file).", "60", Kind::Int { min: 10, max: 86_400 }, Access::Browser, Apply::Restart),
    k("watch_down_after_s", "Notifications", "Watch down-after", "How long the dashboard's health address (or a tile, by fleet default) may fail before it counts as down.", "300", Kind::Int { min: 10, max: 86_400 }, Access::Browser, Apply::Restart),
    k("ask_timeout_s", "Notifications", "Question wait", "Seconds a suspended step waits for an answer before it gives up.", "120", Kind::Int { min: 10, max: 86_400 }, Access::Browser, Apply::Restart),
    // ── Coverage and monitoring ─────────────────────────────────────────
    k("prometheus_url", "Coverage and monitoring", "Prometheus", "Where the coverage check asks whether a stack is measured; empty: not asked.", "not asked", Kind::Url, Access::Browser, Apply::Restart),
    k("loki_url", "Coverage and monitoring", "Loki", "Where the coverage check asks whether a stack's logs arrive; empty: not asked.", "not asked", Kind::Url, Access::Browser, Apply::Restart),
    k("loki_vmid", "Coverage and monitoring", "Loki container", "The container Loki runs in; the log question is asked from inside it.", "asked at the Loki address from pve", Kind::Vmid, Access::Browser, Apply::Restart),
    k("logs_window", "Coverage and monitoring", "Log window", "How far back the log-coverage question looks, e.g. 24h.", "24h", Kind::Window, Access::Browser, Apply::Restart),
    k("status_interval_s", "Coverage and monitoring", "Status interval", "Seconds between two readings of every container's real status.", "60", Kind::Int { min: 10, max: 86_400 }, Access::Browser, Apply::Restart),
    k("recent_lines", "Coverage and monitoring", "Lines kept", "How many of the newest operation lines the host keeps for a client that connects mid-operation.", "2000", Kind::Int { min: 100, max: 100_000 }, Access::Browser, Apply::Restart),
    k("history_days", "Coverage and monitoring", "History days", "Days of operation history kept.", "90", Kind::Int { min: 1, max: 3650 }, Access::Browser, Apply::Restart),
    k("history_max_mib", "Coverage and monitoring", "History size", "Largest size of the history file in MiB; past it the oldest half goes.", "16", Kind::Int { min: 1, max: 1024 }, Access::Browser, Apply::Restart),
    k("metrics_targets_dir", "Coverage and monitoring", "Prometheus targets directory", "Where per-stack scrape targets are written; empty: off.", "off", Kind::Text, Access::Browser, Apply::Restart),
    k("tile_watch_source", "Coverage and monitoring", "Tile watch source address", "The dashboard's own address, from which its once-a-minute tile watch reaches every stack; a deploy derives an inbound firewall rule per stack from its tiles for this source. Empty: no rule is derived.", "off", Kind::Text, Access::Browser, Apply::Restart),
    // ── Gateway and network ─────────────────────────────────────────────
    k("gateway_vmid", "Gateway and network", "Gateway container", "The container Traefik runs in. The dashboard's own route lives there: a wrong value cuts it off from the browser.", "104", Kind::Vmid, Access::Confirm, Apply::Restart),
    k("gateway_routes_dir", "Gateway and network", "Gateway routes directory", "Where route files are written in the gateway. A wrong value drops the dashboard's own route.", "/opt/traefik-config/routes", Kind::Text, Access::Confirm, Apply::Restart),
    k("registry_cache", "Gateway and network", "Registry cache", "The pull-through image cache and its upstreams; empty: images come from their own registry.", "none", Kind::Table, Access::Browser, Apply::Restart),
    // ── Thresholds and limits ───────────────────────────────────────────
    k("patch_threshold_s", "Thresholds and limits", "Patch threshold", "How long a container's updates may stand before `homelab check` says so.", "604800 (7 days)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("backup_max_age_s", "Thresholds and limits", "Backup max age", "How stale a stack's backup may be before it counts as a finding.", "172800 (48 h)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("host_meta_max_age_s", "Thresholds and limits", "Host-meta backup max age", "How old the daemon's own state backup (vault, state.json, TLS, host.toml) may be before it counts as a finding.", "172800 (48 h)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("nightly_report_repeat_s", "Thresholds and limits", "Nightly report repeat", "How often the nightly report repeats an unchanged set of findings.", "604800 (7 days)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    k("incident_bundle_max_age_days", "Thresholds and limits", "Incident bundle max age", "Incident bundles older than this many days are pruned.", "90", Kind::Int { min: 1, max: 3650 }, Access::Browser, Apply::Restart),
    k("incident_bundle_max_count", "Thresholds and limits", "Incident bundle max count", "At most this many incident bundles are kept, newest first.", "200", Kind::Int { min: 1, max: 100_000 }, Access::Browser, Apply::Restart),
    k("upstream_max_age_s", "Thresholds and limits", "Upstream release check age", "How long one GitHub answer about a pinned upstream stands before it is asked again.", "72000 (20 h)", Kind::Int { min: 3600, max: U32 }, Access::Browser, Apply::Restart),
    // ── Safety (ssh only) ───────────────────────────────────────────────
    k("no_touch", "Safety", "No-touch list", "Guests the host never touches; the file can only add to the compiled list. ssh only.", "100, 101, 102, 103", Kind::VmidList, Access::SshOnly, Apply::Restart),
    k("privileged_vmids", "Safety", "Privileged containers", "Containers a deploy may make privileged. ssh only.", "the fleet's own", Kind::VmidList, Access::SshOnly, Apply::Restart),
    k("data_mount_roots", "Safety", "Data mount roots", "Host directories a stack's data mounts may borrow. ssh only.", "the fleet's own", Kind::TextList, Access::SshOnly, Apply::Restart),
];

/// The row of one key.
pub fn key_info(key: &str) -> Option<&'static KeyInfo> {
    KEYS.iter().find(|k| k.key == key)
}

/// Keys whose value is never sent anywhere.
pub fn is_secret(key: &str) -> bool {
    key_info(key).is_some_and(|k| k.access == Access::Secret)
}

/// Why `value` is not a value for `key`, or Ok. `null` is always allowed
/// for an editable key: it removes the key, and the host takes its default.
/// Tables are checked by the host against its own types.
pub fn check_value(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(info) = key_info(key) else {
        return Err(format!("{key} is not a setting the host reads"));
    };
    if !info.access.editable() {
        return Err(match info.access {
            Access::Locked => format!(
                "{key} can cut the dashboard off from the host and is changed over ssh only (arch-self)"
            ),
            Access::SshOnly => format!("{key} is a safety policy and is changed over ssh only"),
            _ => format!("{key} is a secret and is changed over ssh only"),
        });
    }
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
    if key == "tokens" {
        if let Some(list) = value.as_array() {
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
    }
    value.clone()
}
