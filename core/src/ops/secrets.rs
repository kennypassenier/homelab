//! feat-secrets-1/2: the pure, zero-I/O half of the dashboard's Secrets
//! page (AR1: `homelab-core` does no I/O of its own) — the `SecretRef`
//! wire/domain type, and the two path computations both the host's reveal
//! and the host's latch-write route from. The I/O itself (reading the
//! vault, running `latch put`) lives on the host
//! (`host/src/secrets.rs`), deliberately NOT through
//! `Executor`/`Cmd`/`TracingExecutor`: a reveal is a plain file read, and a
//! write is a plain `std::process::Command` whose stdin (the new value) and
//! stdout (latch's own content on read, not used here) are never captured
//! into a line the executor, a transcript or `audit.log` could echo
//! (fix-39, fix-32's rule, applied to a path those fixes did not yet
//! cover). The caller is still responsible for one audit line that names
//! WHAT was read or written, never the value — see `host/src/main.rs`'s
//! `Rpc::RevealSecret` / `Rpc::SetSecret` handlers.

use serde::{Deserialize, Serialize};

/// feat-secrets-1/2: one secret a stack declares, as `StackFile::latch_secrets`
/// / `StackFile::latch_files` name it (client/src/spec.rs). `Env` is an
/// app's whole `.env`; `File` is one `latch_files` entry, named by its
/// absolute destination path in the container (unique within a stack,
/// unlike its position in the list, which an edit can reorder) and the
/// relative path it reads from latch (needed to write it back with `latch
/// put`, since the host keeps no copy of `latch_files` itself — see
/// `admin::core::stackedit_latch`, the only place that list is parsed).
///
/// Defined here (not in `homelab-proto`) because `homelab-proto` depends on
/// `homelab-core`, not the other way around; `proto` re-exports this type
/// for the wire, same pattern as `manifest`/`native`/`retention`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecretRef {
    Env { app: String },
    File { from: String, dest: String },
}

/// Where one secret lives under a stack's vault
/// (`{state_dir}/secrets/{stack}/…`), relative to that directory. Mirrors
/// exactly what `ops::deploy` sealed there: an app's `.env` under its own
/// name, a `latch_files` entry under `deploy::vault_key(&dest)`.
pub fn vault_rel(secret: &SecretRef) -> String {
    match secret {
        SecretRef::Env { app } => format!("{app}.env"),
        SecretRef::File { dest, .. } => crate::ops::deploy::vault_key(dest),
    }
}

/// feat-secrets-2: the relative path inside the latch project that `latch
/// cat`/`latch put` read and write, exactly mirroring
/// `client/src/spec.rs::fetch_latch_secrets`/`fetch_latch_files`
/// (`<stack>/<app>/.env` or `<stack>/<from>`) — so changing a secret here
/// touches the SAME file the next deploy reads, nothing else.
pub fn latch_rel_path(stack: &str, secret: &SecretRef) -> String {
    match secret {
        SecretRef::Env { app } => format!("{stack}/{app}/.env"),
        SecretRef::File { from, .. } => format!("{stack}/{from}"),
    }
}

/// redesign-3.71 secrets: why a value left the vault — shown on screen, or
/// put on the viewer's clipboard without being shown. Both are reveals as
/// far as the vault is concerned; the audit trail tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevealPurpose {
    #[default]
    Reveal,
    Copy,
}

impl RevealPurpose {
    /// The history label: `reveal-secret` or `copy-secret`.
    pub fn label(self) -> &'static str {
        match self {
            RevealPurpose::Reveal => "reveal-secret",
            RevealPurpose::Copy => "copy-secret",
        }
    }

    /// The verb the Activity page reads after the actor's name.
    pub fn verb(self) -> &'static str {
        match self {
            RevealPurpose::Reveal => "revealed",
            RevealPurpose::Copy => "copied",
        }
    }
}

/// redesign-3.71 secrets: who asked for a value and why, as the dashboard
/// tells the host alongside a `RevealSecret` (the host only knows the
/// dashboard's token; the person or Live view behind it is the dashboard's
/// to name). A label, never a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevealAudit {
    #[serde(default)]
    pub purpose: RevealPurpose,
    /// "Kenny", "Claude (Live view)", an Access login: what Activity shows.
    pub by: String,
}

/// redesign-3.71 secrets: the one history line a reveal or a copy leaves
/// (`<state_dir>/history.jsonl`, which the dashboard's Activity reads), so
/// Activity says "Kenny revealed gateway/traefik/.env". It names the
/// secret by its latch path and the actor — never the value, which this
/// function is not even handed.
pub fn reveal_history(
    at: u64,
    stack: &str,
    secret: &SecretRef,
    purpose: RevealPurpose,
    by: Option<String>,
    req: Option<u64>,
    error: Option<String>,
) -> crate::history::HistoryEntry {
    crate::history::HistoryEntry::Op {
        start: at,
        end: at,
        label: purpose.label().to_string(),
        subject: Some(format!(
            "{} {}",
            purpose.verb(),
            latch_rel_path(stack, secret)
        )),
        req,
        by,
        ok: error.is_none(),
        deferred: None,
        error,
        steps: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// covers: feat-secrets-6 — the history line names the secret
    /// and the actor, and carries nothing that could be the value.
    #[test]
    fn redesign_371_a_reveal_history_line_names_the_secret_never_the_value() {
        let env = SecretRef::Env {
            app: "traefik".into(),
        };
        let e = reveal_history(
            1_790_000_000,
            "gateway",
            &env,
            RevealPurpose::Copy,
            Some("Kenny".into()),
            Some(7),
            None,
        );
        let line = e.to_line();
        assert!(line.contains("\"label\":\"copy-secret\""), "{line}");
        assert!(
            line.contains("\"subject\":\"copied gateway/traefik/.env\""),
            "{line}"
        );
        assert!(line.contains("\"by\":\"Kenny\""), "{line}");
        // An old dashboard sends no audit: the purpose defaults to a reveal.
        let a: RevealAudit = serde_json::from_str(r#"{"by":"x"}"#).unwrap();
        assert_eq!(a.purpose, RevealPurpose::Reveal);
    }

    #[test]
    fn vault_rel_for_env_is_the_app_name() {
        let r = SecretRef::Env {
            app: "jellyfin".into(),
        };
        assert_eq!(vault_rel(&r), "jellyfin.env");
    }

    #[test]
    fn vault_rel_for_file_uses_the_same_key_deploy_seals_under() {
        let r = SecretRef::File {
            from: "unit.env".into(),
            dest: "/etc/kyu/unit.env".into(),
        };
        assert_eq!(
            vault_rel(&r),
            crate::ops::deploy::vault_key("/etc/kyu/unit.env")
        );
    }

    #[test]
    fn latch_rel_path_mirrors_the_client_fetch_paths() {
        let env = SecretRef::Env {
            app: "jellyfin".into(),
        };
        assert_eq!(latch_rel_path("media", &env), "media/jellyfin/.env");
        let file = SecretRef::File {
            from: "unit.env".into(),
            dest: "/etc/kyu/unit.env".into(),
        };
        assert_eq!(latch_rel_path("kyu", &file), "kyu/unit.env");
    }
}
