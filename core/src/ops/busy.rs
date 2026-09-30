//! O10: ask a service whether it is busy before updating it.
//!
//! An update or a nightly backup stops containers; for an app somebody is
//! using, that lands in their evening. The app says how to tell in its own
//! `checks.yml` (app-knowledge, Kenny, 2026-09-30: "Alles verplaatsen"):
//!
//! ```yaml
//! busy_check:
//!   command: |   # nothing on stdout: idle; a line per user: in use
//!     ...
//! ```
//!
//! Until then this module knew one app, Jellyfin, by name, with its key path,
//! its URL and its session format in code; any other app could not ask.
//!
//! **It fails closed.** The v1 version of this check did the opposite, and it
//! is worth writing down why that was worse than having no check at all. Every
//! uncertain path in it — no API key, Jellyfin unreachable, an empty response —
//! exited 0, meaning "safe to update". So the exact conditions under which you
//! cannot tell whether someone is watching were the conditions in which it said
//! go ahead. A command that fails, for any reason, is therefore `Unknown`, and
//! `Unknown` is treated as busy. A command must fail when it cannot tell.

use crate::error::CoreError;
use crate::executor::Executor;

/// What a busy-check concluded. `Unknown` is not `Idle` — that distinction is
/// the whole point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Busy {
    /// Somebody is using it; skip the update.
    Yes(String),
    /// Nobody is; go ahead.
    No,
    /// Could not tell. Treated as busy, and says why.
    Unknown(String),
}

impl Busy {
    /// Fail closed: only a definite "no" allows an update.
    pub fn may_update(&self) -> bool {
        matches!(self, Busy::No)
    }
}

/// What a busy check's run says: a failure is Unknown (fail closed), empty
/// stdout is No, anything printed is Yes with those lines as the reason.
pub fn interpret(success: bool, stdout: &str, stderr: &str) -> Busy {
    if !success {
        let why = stderr.trim();
        return Busy::Unknown(if why.is_empty() {
            "the busy check failed without saying why".into()
        } else {
            why.chars().take(200).collect()
        });
    }
    let who: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if who.is_empty() {
        Busy::No
    } else {
        Busy::Yes(who.join("; "))
    }
}

/// Replace a stack's busy checks with what its deploy declared, keyed
/// `stack/app`.
pub fn register(
    state: &mut crate::state::HostState,
    stack: &str,
    checks: &std::collections::BTreeMap<String, crate::checks::ServiceChecks>,
) {
    let prefix = format!("{}/", stack);
    state.busy_checks.retain(|k, _| !k.starts_with(&prefix));
    for (app, sc) in checks {
        if let Some(b) = &sc.busy_check {
            state
                .busy_checks
                .insert(format!("{}/{}", stack, app), b.command.clone());
        }
    }
}

/// Ask one app whether it is in use. `None` when its checks.yml declares no
/// busy check; otherwise a verdict that fails closed.
///
/// **Pass the plain executor, not the tracing one.** Jellyfin's answer names
/// what the household is watching, and a traced call puts it in the
/// transcript — which goes to Loki — every night, twice. The verdict says
/// everything an operator needs ("kenny is paused on …").
///
/// Both callers go through here on purpose. The update path had this check
/// and the backup path did not, and the backup path is the one that stops
/// containers every single night: at 04:17 on 2026-09-04 it stopped Jellyfin
/// while Kenny was watching, which is the one thing the check was written to
/// prevent (F280). One question, asked from both places, cannot drift apart.
pub async fn app_busy(
    exec: &dyn Executor,
    state_dir: &str,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<Option<Busy>, CoreError> {
    let state = crate::state::StateStore::new(exec, state_dir)
        .load()
        .await?;
    let Some(command) = state.busy_checks.get(&format!("{}/{}", stack, app)) else {
        return Ok(None);
    };
    let out = crate::ops::util_pct_sh(exec, vmid, command, 30).await?;
    Ok(Some(interpret(out.success(), &out.stdout, &out.stderr)))
}

/// The sentence a caller shows when it stands aside. Kept here so the update
/// path and the backup path phrase it identically.
pub fn reason(verdict: &Busy) -> String {
    match verdict {
        Busy::Yes(who) => format!("in use — {}", who),
        Busy::Unknown(why) => format!("could not tell ({}), so treating it as in use", why),
        Busy::No => "not in use".to_string(),
    }
}
