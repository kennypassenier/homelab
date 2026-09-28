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
//! | `HOMELAB_ADMIN_SCHEDULE_TICK_S` | 20 | how often the scheduler looks |
//! | `HOMELAB_ADMIN_SCHEDULE_GRACE_S` | 300 | how late a slot may still run |
//! | `HOMELAB_ADMIN_INCIDENTS_POLL_S` | 300 | how often the host's incident list is read |
//! | `HOMELAB_ADMIN_ACTION_TIMEOUT_S` | 21600 | the longest one action is waited for |
//! | `HOMELAB_ADMIN_GIT_REMOTE` | `git@github.com:kennypassenier/homelab.git` | where the working copy is cloned from and pushed to (arch-edit-txn); a local path works too (tests) |
//! | `HOMELAB_ADMIN_GIT_BRANCH` | `main` | the branch it follows and pushes |
//! | `HOMELAB_ADMIN_GIT_KEY` | `<data dir>/deploy_key` | the deploy key's private half (arch-push-credential); written at start from `HOMELAB_ADMIN_DEPLOY_KEY_B64` when missing |
//! | `HOMELAB_ADMIN_DEPLOY_KEY_B64` | none | the deploy key file, base64, from latch (a secret: admin.env); written to `HOMELAB_ADMIN_GIT_KEY` with mode 0600 when that file is missing, never logged |
//! | `HOMELAB_ADMIN_GIT_KNOWN_HOSTS` | `<data dir>/known_hosts` | the remote's ssh host keys; ssh refuses a host not in it; GitHub's pinned keys are written there when it is missing |
//! | `HOMELAB_ADMIN_GIT_AUTHOR` | `homelab-admin <homelab-admin@users.noreply.github.com>` | the author and committer of the dashboard's commits |

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActConfig {
    pub data_dir: PathBuf,
    pub repo: PathBuf,
    pub notify_url: Option<String>,
    pub notify_token: Option<String>,
    pub schedule_tick_s: u64,
    pub schedule_grace_s: u64,
    pub incidents_poll_s: u64,
    pub action_timeout_s: u64,
    /// arch-edit-txn: the working copy's remote, branch and credential.
    pub git: GitConfig,
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
    if let Some(rest) = r.strip_prefix("ssh://") {
        if rest.split('/').next().is_some_and(|auth| {
            auth.contains(':')
                && auth.contains('@')
                && auth.split('@').next().is_some_and(|u| u.contains(':'))
        }) {
            return Some("HOMELAB_ADMIN_GIT_REMOTE must not carry a password".into());
        }
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
    if let Some(u) = &notify_url {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            why.push("HOMELAB_ADMIN_NOTIFY_URL must be an http(s) URL".to_string());
        }
    }
    if notify_token.is_some() && notify_url.is_none() {
        why.push("HOMELAB_ADMIN_NOTIFY_TOKEN is set without HOMELAB_ADMIN_NOTIFY_URL".into());
    }
    let cfg = ActConfig {
        data_dir: data_dir.clone(),
        repo,
        notify_url,
        notify_token,
        schedule_tick_s: number(lookup, "SCHEDULE_TICK_S", 20, 1, &mut why),
        schedule_grace_s: number(lookup, "SCHEDULE_GRACE_S", 300, 1, &mut why),
        incidents_poll_s: number(lookup, "INCIDENTS_POLL_S", 300, 10, &mut why),
        action_timeout_s: number(lookup, "ACTION_TIMEOUT_S", 21_600, 60, &mut why),
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
