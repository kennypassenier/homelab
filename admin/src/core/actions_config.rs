//! arch-config for milestone act: the settings the actions, schedules and
//! notifications need, from the environment (`HOMELAB_ADMIN_<KEY>`), the way
//! CT 120 takes every setting. Kept apart from the `[admin]` table so this
//! milestone adds no key the table would refuse; folding them into it is a
//! later, one-place change.
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `HOMELAB_ADMIN_DATA_DIR` | `HOMELAB_ADMIN_STATE_DIR`, else `/appdata/admin/admin-config` | where schedules.json and notifications.json live (arch-state) |
//! | `HOMELAB_ADMIN_REPO` | `<data dir>/repo` | the homelab working copy the stack files are read from |
//! | `HOMELAB_ADMIN_NOTIFY_URL` | none: no pushes | kyu's publish URL for feat-ops-8 |
//! | `HOMELAB_ADMIN_NOTIFY_TOKEN` | none | kyu's bearer token (a secret: admin.env) |
//! | `HOMELAB_ADMIN_NOTIFY_TLS_CERT` | none | fix-126: path of the LAN hub's self-signed certificate (PEM), required once `HOMELAB_ADMIN_NOTIFY_URL` is `https://` |
//! | `HOMELAB_ADMIN_NOTIFY_TLS_FINGERPRINT` | none | fix-126: that certificate's SHA-256, checked against the file on disk at every start |
//! | `HOMELAB_ADMIN_SCHEDULE_TICK_S` | 20 | how often the scheduler looks |
//! | `HOMELAB_ADMIN_SCHEDULE_GRACE_S` | 300 | how late a slot may still run |
//! | `HOMELAB_ADMIN_INCIDENTS_POLL_S` | 300 | how often the host's incident list is read |
//! | `HOMELAB_ADMIN_HOST_NOTICES_POLL_S` | 60 | how often the host's notices are read into the notification centre (decision notify-routing) |
//! | `HOMELAB_ADMIN_ALERTS_TOKEN` | none: the Alertmanager hook refuses every call | the bearer Alertmanager sends to `/hooks/alertmanager` (a secret: admin.env) |
//! | `HOMELAB_ADMIN_PUBLIC_URL` | none: a push carries no link | the dashboard's public address, which a push links to (`click_url`); chassis reads the same key for passkeys |
//! | `HOMELAB_ADMIN_ACTION_TIMEOUT_S` | 21600 | the longest one action is waited for |
//! | `HOMELAB_ADMIN_GIT_REMOTE` | `git@github.com:kennypassenier/homelab.git` | where the working copy is cloned from and pushed to (arch-edit-txn); a local path works too (tests) |
//! | `HOMELAB_ADMIN_GIT_BRANCH` | `main` | the branch it follows and pushes |
//! | `HOMELAB_ADMIN_GIT_KEY` | `<data dir>/deploy_key` | the deploy key's private half (arch-push-credential); written at start from `HOMELAB_ADMIN_DEPLOY_KEY_B64` when missing |
//! | `HOMELAB_ADMIN_DEPLOY_KEY_B64` | none | the deploy key file, base64, from latch (a secret: admin.env); written to `HOMELAB_ADMIN_GIT_KEY` with mode 0600 when that file is missing, never logged |
//! | `HOMELAB_ADMIN_GIT_KNOWN_HOSTS` | `<data dir>/known_hosts` | the remote's ssh host keys; ssh refuses a host not in it; GitHub's pinned keys are written there when it is missing |
//! | `HOMELAB_ADMIN_GIT_AUTHOR` | `homelab-admin <homelab-admin@users.noreply.github.com>` | the author and committer of the dashboard's commits |
//! | `HOMELAB_ADMIN_LIVE_ANNOUNCE_MS` | 3000 | Live view: how long a driven step is announced ("Next: …" with its countdown) before it is taken; 0 announces nothing, at most 10000 |
//! | `HOMELAB_ADMIN_LIVE_MAX_PAUSE_S` | 1800 | Live view: how long a step a viewer paused waits for Continue before it fails; 60 to 3600 |
//!
//! Decision "23 constants" (Kenny, 2026-09-30): the rest were fixed
//! `const`s; nothing changes until one is set.
//!
//! | Key | Default | Meaning |
//! |---|---|---|
//! | `HOMELAB_ADMIN_KEEP_JOBS` | 200 | jobs kept for `GET /data/actions/jobs` and a reload |
//! | `HOMELAB_ADMIN_HISTORY_WINDOW_S` | 15552000 (180 d) | history read for a job's expected duration |
//! | `HOMELAB_ADMIN_NOTIFY_KEEP` | 500 | newest notices kept; older ones fall off |
//! | `HOMELAB_ADMIN_NOTIFY_MAX_AGE_DAYS` | 180 | notices older than this fall off too, whatever the count (rule-20) |
//! | `HOMELAB_ADMIN_NOTIFY_SNOOZE_MAX_S` | 604800 (7 d) | the longest snooze |
//! | `HOMELAB_ADMIN_NOTIFY_DIGEST_LATE_S` | 10800 (3 h) | how late the daily digest may still go out |
//! | `HOMELAB_ADMIN_HOSTLOG_RING` | 2000 | host-log lines kept for a page that opens mid-operation |
//! | `HOMELAB_ADMIN_RELEASES_WATCH_EVERY_S` | 3600 | how often the "host update available" badge looks for a newer release |
//! | `HOMELAB_ADMIN_DRIFT_REUSE_S` | 300 | how long a drift reading is reused before latch runs again |
//! | `HOMELAB_ADMIN_LIVE_ANNOUNCE_MAX_MS` | 10000 | Live view: the longest announcement `HOMELAB_ADMIN_LIVE_ANNOUNCE_MS` may be set to |
//! | `HOMELAB_ADMIN_LIVE_MAX_PAUSE_MAX_S` | 3600 | Live view: the longest pause `HOMELAB_ADMIN_LIVE_MAX_PAUSE_S` may be set to |
//! | `HOMELAB_ADMIN_DRIVE_IDLE_S` | 600 | a driver who sends nothing for this long no longer holds the tabs |
//! | `HOMELAB_ADMIN_RELEASE_AFTER_JOB_S` | 30 | a confirmed dialog whose job has ended, with no step since, is closed and the tabs given back this long after |

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActConfig {
    pub data_dir: PathBuf,
    pub repo: PathBuf,
    pub notify_url: Option<String>,
    pub notify_token: Option<String>,
    /// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
    /// LAN-only self-signed certificate `notify_url` is pinned to, when it
    /// is `https://` — mirrors the host's `notify_tls_cert`. `None` on a
    /// plain `http://` route (migration is stepwise).
    pub notify_tls_cert: Option<String>,
    /// The SHA-256 of that certificate's DER form, lowercase hex. Checked
    /// against the file on disk at every start, the same as the host.
    pub notify_tls_fingerprint: Option<String>,
    pub schedule_tick_s: u64,
    pub schedule_grace_s: u64,
    pub incidents_poll_s: u64,
    /// Decision notify-routing: how often the host's notices are read.
    pub host_notices_poll_s: u64,
    /// The bearer Alertmanager sends to the hook; None refuses every call.
    pub alerts_token: Option<String>,
    /// The dashboard's public address, for a push's `click_url`.
    pub public_url: String,
    pub action_timeout_s: u64,
    /// Live view: the announcement's countdown, in milliseconds.
    pub live_announce_ms: u64,
    /// Live view: the longest pause, in seconds.
    pub live_max_pause_s: u64,
    /// arch-edit-txn: the working copy's remote, branch and credential.
    pub git: GitConfig,
    /// Decision "23 constants": jobs kept for `GET /data/actions/jobs`.
    pub keep_jobs: usize,
    /// History read for a job's expected duration, in seconds.
    pub history_window_s: i64,
    /// Newest notices kept in the notification centre.
    pub notify_keep: usize,
    /// rule-20 (disk-audit, 2026-10-01): a notice older than this many days
    /// falls off too, whatever `notify_keep` would otherwise still hold —
    /// the store had a count cap and no age cap before this.
    pub notify_max_age_days: i64,
    /// The longest snooze, in seconds.
    pub notify_snooze_max_s: i64,
    /// How late the daily digest may still go out, in seconds.
    pub notify_digest_late_s: i64,
    /// Host-log lines kept for a page that opens mid-operation.
    pub hostlog_ring: usize,
    /// How often the "host update available" badge looks, in seconds.
    pub releases_watch_every_s: u64,
    /// How long a drift reading is reused, in seconds.
    pub drift_reuse_s: u64,
    /// Live view: the longest announcement `live_announce_ms` may be set
    /// to, in milliseconds.
    pub live_announce_max_ms: u64,
    /// Live view: the longest pause `live_max_pause_s` may be set to, in
    /// seconds.
    pub live_max_pause_max_s: u64,
    /// Live view: a driver who sends nothing for this long no longer holds
    /// the tabs (`core::drive::IDLE_S` by default).
    pub drive_idle_s: i64,
    /// Live view: how long a confirmed, finished dialog waits before it is
    /// released (`core::drive::RELEASE_AFTER_JOB_S` by default).
    pub drive_release_after_job_s: i64,
}

/// arch-edit-txn, arch-push-credential: how the working copy reaches its
/// remote. The key is a path: its content never passes through here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    pub remote: String,
    pub branch: String,
    pub key: PathBuf,
    pub known_hosts: PathBuf,
    pub author_name: String,
    pub author_email: String,
}

pub const DEFAULT_REMOTE: &str = "git@github.com:kennypassenier/homelab.git";
pub const DEFAULT_AUTHOR: &str = "homelab-admin <homelab-admin@users.noreply.github.com>";

/// `Name <email>`.
pub fn parse_author(s: &str) -> Option<(String, String)> {
    let (name, rest) = s.split_once('<')?;
    let email = rest.strip_suffix('>')?.trim();
    let name = name.trim();
    (!name.is_empty() && email.contains('@') && !email.contains(char::is_whitespace))
        .then(|| (name.to_string(), email.to_string()))
}

/// A remote the working copy may use: ssh (`git@host:path` or `ssh://`), or
/// a local path or `file://` (tests, a workstation). Never a URL with a
/// credential in it: no token may sit in a remote URL (arch-edit-txn).
pub fn remote_problem(r: &str) -> Option<String> {
    if r.trim().is_empty() {
        return Some("HOMELAB_ADMIN_GIT_REMOTE is empty".into());
    }
    if r.starts_with("http://") || r.starts_with("https://") {
        return Some(
            "HOMELAB_ADMIN_GIT_REMOTE must be an ssh remote (git@github.com:owner/repo.git): \
             the push goes with the deploy key, and a token never sits in a URL"
                .into(),
        );
    }
    if let Some(rest) = r.strip_prefix("ssh://")
        && rest.split('/').next().is_some_and(|auth| {
            auth.contains(':')
                && auth.contains('@')
                && auth.split('@').next().is_some_and(|u| u.contains(':'))
        })
    {
        return Some("HOMELAB_ADMIN_GIT_REMOTE must not carry a password".into());
    }
    None
}

pub const DEFAULT_DATA_DIR: &str = "/appdata/admin/admin-config";

fn number(
    lookup: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: u64,
    min: u64,
    why: &mut Vec<String>,
) -> u64 {
    let name = format!("HOMELAB_ADMIN_{key}");
    match lookup(&name) {
        None => default,
        Some(v) => match v.trim().parse::<u64>() {
            Ok(n) if n >= min => n,
            _ => {
                why.push(format!(
                    "{name} = {v:?} must be a whole number of at least {min}"
                ));
                default
            }
        },
    }
}

/// The settings, or every reason they cannot run, in one message.
pub fn from_env(lookup: &dyn Fn(&str) -> Option<String>) -> Result<ActConfig, String> {
    let mut why = Vec::new();
    let non_empty = |k: &str| lookup(k).filter(|v| !v.trim().is_empty());
    let data_dir = PathBuf::from(
        non_empty("HOMELAB_ADMIN_DATA_DIR")
            .or_else(|| non_empty("HOMELAB_ADMIN_STATE_DIR"))
            .unwrap_or_else(|| DEFAULT_DATA_DIR.into()),
    );
    let repo = non_empty("HOMELAB_ADMIN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("repo"));
    let notify_url = non_empty("HOMELAB_ADMIN_NOTIFY_URL");
    let notify_token = non_empty("HOMELAB_ADMIN_NOTIFY_TOKEN");
    if let Some(u) = &notify_url
        && !(u.starts_with("http://") || u.starts_with("https://"))
    {
        why.push("HOMELAB_ADMIN_NOTIFY_URL must be an http(s) URL".to_string());
    }
    if notify_token.is_some() && notify_url.is_none() {
        why.push("HOMELAB_ADMIN_NOTIFY_TOKEN is set without HOMELAB_ADMIN_NOTIFY_URL".into());
    }
    let notify_tls_cert = non_empty("HOMELAB_ADMIN_NOTIFY_TLS_CERT");
    let notify_tls_fingerprint = non_empty("HOMELAB_ADMIN_NOTIFY_TLS_FINGERPRINT");
    let https_route = notify_url
        .as_deref()
        .is_some_and(|u| u.starts_with("https://"));
    if https_route && (notify_tls_cert.is_none() || notify_tls_fingerprint.is_none()) {
        why.push(
            "HOMELAB_ADMIN_NOTIFY_URL is https:// but HOMELAB_ADMIN_NOTIFY_TLS_CERT or \
             HOMELAB_ADMIN_NOTIFY_TLS_FINGERPRINT is not set — the LAN hub's certificate is \
             self-signed, so nothing is trusted unless it is pinned"
                .into(),
        );
    }
    if !https_route && (notify_tls_cert.is_some() || notify_tls_fingerprint.is_some()) {
        why.push(
            "HOMELAB_ADMIN_NOTIFY_TLS_CERT/_FINGERPRINT are set but HOMELAB_ADMIN_NOTIFY_URL is \
             not https://"
                .into(),
        );
    }
    let cfg = ActConfig {
        data_dir: data_dir.clone(),
        repo,
        notify_url,
        notify_token,
        notify_tls_cert,
        notify_tls_fingerprint,
        schedule_tick_s: number(lookup, "SCHEDULE_TICK_S", 20, 1, &mut why),
        schedule_grace_s: number(lookup, "SCHEDULE_GRACE_S", 300, 1, &mut why),
        incidents_poll_s: number(lookup, "INCIDENTS_POLL_S", 300, 10, &mut why),
        host_notices_poll_s: number(lookup, "HOST_NOTICES_POLL_S", 60, 5, &mut why),
        alerts_token: non_empty("HOMELAB_ADMIN_ALERTS_TOKEN").map(|t| t.trim().to_string()),
        // tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30):
        // `HOMELAB_ADMIN_WATCH_VIA` is retired — the minute watch now asks
        // each tile's own `probe` address directly (see
        // `shell::watch::round`), not through Traefik with a forged Host
        // header, which is what this key used to configure. A value left
        // in admin.env is simply unread from here on; nothing refuses it.
        public_url: {
            let u = non_empty("HOMELAB_ADMIN_PUBLIC_URL").unwrap_or_default();
            if !u.is_empty() && !(u.starts_with("http://") || u.starts_with("https://")) {
                why.push("HOMELAB_ADMIN_PUBLIC_URL must be an http(s) URL".to_string());
            }
            u.trim_end_matches('/').to_string()
        },
        action_timeout_s: number(lookup, "ACTION_TIMEOUT_S", 21_600, 60, &mut why),
        live_announce_ms: number(
            lookup,
            "LIVE_ANNOUNCE_MS",
            crate::core::drivelive::ANNOUNCE_MS,
            0,
            &mut why,
        ),
        live_max_pause_s: number(
            lookup,
            "LIVE_MAX_PAUSE_S",
            crate::core::drivelive::MAX_PAUSE_S,
            60,
            &mut why,
        ),
        keep_jobs: number(lookup, "KEEP_JOBS", 200, 1, &mut why) as usize,
        history_window_s: number(lookup, "HISTORY_WINDOW_S", 180 * 86_400, 1, &mut why) as i64,
        notify_keep: number(
            lookup,
            "NOTIFY_KEEP",
            crate::core::notify::KEEP as u64,
            1,
            &mut why,
        ) as usize,
        notify_max_age_days: number(
            lookup,
            "NOTIFY_MAX_AGE_DAYS",
            crate::core::notify::MAX_AGE_DAYS as u64,
            1,
            &mut why,
        ) as i64,
        notify_snooze_max_s: number(
            lookup,
            "NOTIFY_SNOOZE_MAX_S",
            crate::core::notify::SNOOZE_MAX_S as u64,
            0,
            &mut why,
        ) as i64,
        notify_digest_late_s: number(
            lookup,
            "NOTIFY_DIGEST_LATE_S",
            crate::core::notify::DIGEST_LATE_S as u64,
            0,
            &mut why,
        ) as i64,
        hostlog_ring: number(lookup, "HOSTLOG_RING", 2_000, 1, &mut why) as usize,
        releases_watch_every_s: number(lookup, "RELEASES_WATCH_EVERY_S", 3_600, 1, &mut why),
        drift_reuse_s: number(lookup, "DRIFT_REUSE_S", 300, 0, &mut why),
        live_announce_max_ms: number(
            lookup,
            "LIVE_ANNOUNCE_MAX_MS",
            crate::core::drivelive::ANNOUNCE_MAX_MS,
            0,
            &mut why,
        ),
        live_max_pause_max_s: number(
            lookup,
            "LIVE_MAX_PAUSE_MAX_S",
            crate::core::drivelive::MAX_PAUSE_MAX_S,
            60,
            &mut why,
        ),
        drive_idle_s: number(
            lookup,
            "DRIVE_IDLE_S",
            crate::core::drive::IDLE_S as u64,
            1,
            &mut why,
        ) as i64,
        drive_release_after_job_s: number(
            lookup,
            "RELEASE_AFTER_JOB_S",
            crate::core::drive::RELEASE_AFTER_JOB_S as u64,
            0,
            &mut why,
        ) as i64,
        git: {
            let remote =
                non_empty("HOMELAB_ADMIN_GIT_REMOTE").unwrap_or_else(|| DEFAULT_REMOTE.into());
            if let Some(p) = remote_problem(&remote) {
                why.push(p);
            }
            let branch = non_empty("HOMELAB_ADMIN_GIT_BRANCH").unwrap_or_else(|| "main".into());
            if branch.starts_with('-')
                || branch.contains(char::is_whitespace)
                || branch.contains("..")
            {
                why.push(format!(
                    "HOMELAB_ADMIN_GIT_BRANCH = {branch:?} is not a branch name"
                ));
            }
            let author =
                non_empty("HOMELAB_ADMIN_GIT_AUTHOR").unwrap_or_else(|| DEFAULT_AUTHOR.into());
            let (author_name, author_email) = parse_author(&author).unwrap_or_else(|| {
                why.push(format!(
                    "HOMELAB_ADMIN_GIT_AUTHOR = {author:?} must be `Name <email>`"
                ));
                parse_author(DEFAULT_AUTHOR).unwrap_or_default()
            });
            GitConfig {
                remote,
                branch,
                key: non_empty("HOMELAB_ADMIN_GIT_KEY")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| data_dir.join("deploy_key")),
                known_hosts: non_empty("HOMELAB_ADMIN_GIT_KNOWN_HOSTS")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| data_dir.join("known_hosts")),
                author_name,
                author_email,
            }
        },
    };
    if cfg.live_announce_max_ms > crate::core::drivelive::ANNOUNCE_MAX_MS {
        why.push(format!(
            "HOMELAB_ADMIN_LIVE_ANNOUNCE_MAX_MS must be at most {}: the host waits {} s for a step's answer",
            crate::core::drivelive::ANNOUNCE_MAX_MS,
            homelab_proto::UI_RELAY_WAIT_S
        ));
    }
    if cfg.live_announce_ms > cfg.live_announce_max_ms {
        why.push(format!(
            "HOMELAB_ADMIN_LIVE_ANNOUNCE_MS must be at most HOMELAB_ADMIN_LIVE_ANNOUNCE_MAX_MS ({})",
            cfg.live_announce_max_ms
        ));
    }
    if cfg.live_max_pause_max_s > crate::core::drivelive::MAX_PAUSE_MAX_S {
        why.push(format!(
            "HOMELAB_ADMIN_LIVE_MAX_PAUSE_MAX_S must be at most {}: the host holds a paused step no longer",
            crate::core::drivelive::MAX_PAUSE_MAX_S
        ));
    }
    if cfg.live_max_pause_s > cfg.live_max_pause_max_s {
        why.push(format!(
            "HOMELAB_ADMIN_LIVE_MAX_PAUSE_S must be at most HOMELAB_ADMIN_LIVE_MAX_PAUSE_MAX_S ({})",
            cfg.live_max_pause_max_s
        ));
    }
    if cfg.schedule_grace_s < cfg.schedule_tick_s {
        why.push(
            "HOMELAB_ADMIN_SCHEDULE_GRACE_S must be at least HOMELAB_ADMIN_SCHEDULE_TICK_S, \
             or a slot can pass between two looks and never run"
                .into(),
        );
    }
    if why.is_empty() {
        Ok(cfg)
    } else {
        Err(why.join("; "))
    }
}

impl ActConfig {
    pub fn tick(&self) -> Duration {
        Duration::from_secs(self.schedule_tick_s)
    }
    pub fn action_timeout(&self) -> Duration {
        Duration::from_secs(self.action_timeout_s)
    }
    pub fn schedules_file(&self) -> PathBuf {
        self.data_dir.join("schedules.json")
    }
    pub fn notify_file(&self) -> PathBuf {
        self.data_dir.join("notifications.json")
    }
    /// Where a scratch copy of a stack is validated, and removed again.
    pub fn scratch_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }
}
