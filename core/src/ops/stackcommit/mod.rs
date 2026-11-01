//! key-move: the host commits and pushes the dashboard's stack edits, so the
//! repository's deploy key lives on one machine (the host) instead of on the
//! dashboard's container too.
//!
//! The dashboard sends `Command::CommitStackFiles` (the files' new text, the
//! commit message, and the commit it planned on); the host checks the request
//! with [`guard::request_problems`], applies it in its own clone of the
//! stacks repository and pushes with the key `[stack_commits]` names. This
//! module holds what both sides agree on and the pure decisions: the wire
//! shapes, the `[stack_commits]` settings and their resolution, and the
//! checks. The git work itself is host I/O (`host/src/stack_commit/`).

pub mod guard;

use serde::{Deserialize, Serialize};

/// One file of a `CommitStackFiles` request: its path from the repository
/// root (`stacks/media/lxc-compose.yml`) and its whole new text, or `None`
/// to delete it. Stack files are text; the dashboard reads and writes them
/// as UTF-8, so no binary form is needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackFileChange {
    pub path: String,
    #[serde(default)]
    pub content: Option<String>,
}

/// What a `CommitStackFiles` answers on success (JSON in `RpcResponse.message`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackCommitDone {
    /// The new commit's full hash.
    pub commit: String,
    pub subject: String,
    /// The remote has the commit. False only for a dry run (`push: false`),
    /// where the commit was made and undone again on the host.
    pub pushed: bool,
    /// The push reported an error and `ls-remote` showed it landed anyway.
    pub landed_despite_error: bool,
}

/// The remote the dashboard's working copy already defaults to.
pub const DEFAULT_REMOTE: &str = "git@github.com:kennypassenier/homelab.git";
pub const DEFAULT_BRANCH: &str = "main";
pub const DEFAULT_AUTHOR: &str = "homelab-host <homelab-host@users.noreply.github.com>";
/// The three places the dashboard commits to today: stack directories, the
/// preset catalog, and the declarative host settings file.
pub const DEFAULT_ROOTS: &[&str] = &["stacks/", "presets/", "config/host.toml"];
/// The most file text (plus message) one request may carry: 2 MiB. The
/// dashboard's editor refuses files above `RAW_MAX` (256 KiB) each.
pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// How long one git command (clone, fetch, push) may take.
pub const DEFAULT_TIMEOUT_S: u64 = 120;

/// GitHub's ssh host keys, as GitHub publishes them
/// (docs.github.com, "GitHub's SSH key fingerprints", and `api.github.com/meta`
/// `ssh_keys`; compared 2026-09-28). One `known_hosts` line each. Pinned
/// here, never fetched with `ssh-keyscan` (which trusts whoever answers
/// first); both the host and the dashboard write them when their
/// known-hosts file is missing.
pub const GITHUB_KNOWN_HOSTS: &str = "\
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
github.com ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=
github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk=
";

/// `[stack_commits]` in host.toml, as written. The table's presence turns
/// the feature on; `key` is the only field without a default. Changed over
/// ssh only (`hostconfig::KEYS`): it names a credential.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackCommitSettings {
    /// The stacks repository's remote; default [`DEFAULT_REMOTE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Path of the deploy key's private half (mode 0600). Required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The remote's ssh host keys; default `<state_dir>/stack-commits/known_hosts`,
    /// written from [`GITHUB_KNOWN_HOSTS`] when missing and the remote is GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_hosts: Option<String>,
    /// The host's own clone; default `<state_dir>/stack-commits/repo`. Never
    /// the intent repository (`<state_dir>/repo`): that one holds the host's
    /// deploy history, not the stacks repository's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// `Name <email>` of the host's commits; default [`DEFAULT_AUTHOR`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Path prefixes a request may touch (`dir/`), or exact files; default
    /// [`DEFAULT_ROOTS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u64>,
}

/// The settings with every default filled in, as the host runs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackCommitConfig {
    pub remote: String,
    pub branch: String,
    pub key: String,
    pub known_hosts: String,
    pub dir: String,
    pub author_name: String,
    pub author_email: String,
    pub roots: Vec<String>,
    pub max_bytes: u64,
    pub timeout_s: u64,
}

/// `Name <email>`.
pub fn parse_author(s: &str) -> Option<(String, String)> {
    let (name, rest) = s.split_once('<')?;
    let email = rest.strip_suffix('>')?.trim();
    let name = name.trim();
    (!name.is_empty() && email.contains('@') && !email.contains(char::is_whitespace))
        .then(|| (name.to_string(), email.to_string()))
}

fn text(v: &Option<String>) -> Option<String> {
    v.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl StackCommitSettings {
    /// Every default filled in, or every reason the table cannot run, in
    /// one message. Pure: whether the key file exists is the host's check
    /// at request time, so a key placed later needs no restart.
    pub fn resolve(&self, state_dir: &str) -> Result<StackCommitConfig, String> {
        let mut why = Vec::new();
        let remote = text(&self.remote).unwrap_or_else(|| DEFAULT_REMOTE.into());
        if let Some(p) = guard::remote_problem(&remote) {
            why.push(p);
        }
        let branch = text(&self.branch).unwrap_or_else(|| DEFAULT_BRANCH.into());
        if !guard::is_branch_name(&branch) {
            why.push(format!(
                "stack_commits.branch = {branch:?} is not a branch name"
            ));
        }
        let key = text(&self.key).unwrap_or_default();
        if key.is_empty() {
            why.push("stack_commits.key is not set: name the deploy key's file".into());
        }
        let author = text(&self.author).unwrap_or_else(|| DEFAULT_AUTHOR.into());
        let (author_name, author_email) = parse_author(&author).unwrap_or_else(|| {
            why.push(format!(
                "stack_commits.author = {author:?} must be `Name <email>`"
            ));
            Default::default()
        });
        let roots: Vec<String> = match &self.roots {
            Some(r) => r.clone(),
            None => DEFAULT_ROOTS.iter().map(|s| s.to_string()).collect(),
        };
        if roots.is_empty() || roots.iter().any(|r| guard::path_problem(r).is_some()) {
            why.push(format!(
                "stack_commits.roots = {roots:?} must be plain repository paths"
            ));
        }
        let base = format!("{}/stack-commits", state_dir.trim_end_matches('/'));
        let cfg = StackCommitConfig {
            remote,
            branch,
            key,
            known_hosts: text(&self.known_hosts).unwrap_or_else(|| format!("{base}/known_hosts")),
            dir: text(&self.dir).unwrap_or_else(|| format!("{base}/repo")),
            author_name,
            author_email,
            roots,
            max_bytes: self.max_bytes.unwrap_or(DEFAULT_MAX_BYTES).max(1),
            timeout_s: self.timeout_s.unwrap_or(DEFAULT_TIMEOUT_S).max(5),
        };
        if why.is_empty() {
            Ok(cfg)
        } else {
            Err(why.join("; "))
        }
    }
}

/// Whether a remote reaches GitHub over ssh (only then are GitHub's host
/// keys the ones ssh needs).
pub fn is_github_ssh(remote: &str) -> bool {
    remote.starts_with("git@github.com:") || remote.starts_with("ssh://git@github.com/")
}

/// Whether a remote is a local path (tests, a workstation): no ssh, no key.
pub fn is_local(remote: &str) -> bool {
    remote.starts_with('/')
        || remote.starts_with("file://")
        || remote.starts_with("./")
        || remote.starts_with("../")
}
