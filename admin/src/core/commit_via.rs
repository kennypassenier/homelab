//! key-move: who commits and pushes the dashboard's edits.
//!
//! `HOMELAB_ADMIN_COMMIT_VIA=dashboard` (the default, today's path): the
//! working copy commits and pushes itself with the deploy key on CT 120.
//! `host`: the working copy validates as before, then sends the files to the
//! host (`Command::CommitStackFiles`), which commits and pushes with the key
//! only it holds; the working copy then fetches the result. With `host`, the
//! working copy only reads the remote, so it may use anonymous https (the
//! repository is public) and needs no key at all — that is the default
//! remote then.

/// The setting.
pub const VIA_ENV: &str = "HOMELAB_ADMIN_COMMIT_VIA";
/// The remote the working copy reads when the host commits: no key needed.
pub const HOST_MODE_REMOTE: &str = "https://github.com/kennypassenier/homelab.git";
/// How long the dashboard waits for the host's commit by default.
pub const DEFAULT_HOST_COMMIT_TIMEOUT_S: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitVia {
    /// The working copy pushes with its own deploy key.
    #[default]
    Dashboard,
    /// The host commits and pushes; the dashboard holds no key.
    Host,
}

impl CommitVia {
    /// The value of [`VIA_ENV`]; unset is `dashboard`.
    pub fn parse(v: Option<&str>) -> Result<CommitVia, String> {
        match v.map(str::trim) {
            None | Some("") | Some("dashboard") => Ok(CommitVia::Dashboard),
            Some("host") => Ok(CommitVia::Host),
            Some(other) => Err(format!(
                "{VIA_ENV} = {other:?} must be `dashboard` or `host`"
            )),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CommitVia::Dashboard => "dashboard",
            CommitVia::Host => "host",
        }
    }
}

/// With `host`, the working copy may also read over https — but never with
/// a credential in the URL (`https://user:token@…`, or any `user@`).
pub fn https_problem(r: &str) -> Option<String> {
    let rest = r
        .strip_prefix("https://")
        .or_else(|| r.strip_prefix("http://"))?;
    if r.starts_with("http://") {
        return Some("HOMELAB_ADMIN_GIT_REMOTE must not be plain http://".into());
    }
    let authority = rest.split('/').next().unwrap_or("");
    authority.contains('@').then(|| {
        "HOMELAB_ADMIN_GIT_REMOTE must not carry a user or token: the host pushes, \
         the dashboard only reads the public repository"
            .into()
    })
}
