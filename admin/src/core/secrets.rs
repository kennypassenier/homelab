//! redesign-3.71 secrets (Kenny, 2026-10-03, approved demo `secrets.html`
//! plus "Alle drie": hide after 30 s, Copy, every reveal and copy in the
//! audit trail). The pure half of the Secrets page's server side: what one
//! stack declares (or why that cannot be read), who a reveal is recorded
//! for, and how a host too old for the audit field is recognised. Pure.

use serde::Serialize;

use super::stackedit_latch::{CurrentLatch, CurrentLatchFile};

/// One stack's row in the page's left pane: the declared names (never a
/// value), or why the stack file could not be read with what to do.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Declared {
    pub secrets: Vec<String>,
    pub files: Vec<CurrentLatchFile>,
    /// "why :: fix" when the stack's file does not read; the page shows the
    /// warning chip and this text, split at " :: ".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
}

/// What `lxc-compose.yml` declares under `latch_secrets:` and
/// `latch_files:`. `None` (no file: the stack's directory is gone from the
/// repository, or it has no manifest) declares nothing; a file that does
/// not parse is unreadable, with the parser's own words — never silently
/// "declares none", which would hide a broken file behind an empty list.
pub fn declared(manifest: Option<&str>) -> Declared {
    let Some(text) = manifest else {
        return Declared::default();
    };
    let value = match serde_yaml::from_str::<serde_yaml::Value>(text) {
        Ok(v) => v,
        Err(e) => {
            return Declared {
                unreadable: Some(format!(
                    "lxc-compose.yml does not parse: {e} :: fix the file (the stack's Settings tab, \
                     or the repository) and read again"
                )),
                ..Declared::default()
            };
        }
    };
    match serde_yaml::from_value::<CurrentLatch>(value) {
        Ok(c) => Declared {
            secrets: c.latch_secrets,
            files: c.latch_files,
            unreadable: None,
        },
        Err(e) => Declared {
            unreadable: Some(format!(
                "latch_secrets or latch_files does not read: {e} :: fix the list in the stack's \
                 latch form (Settings tab) and read again"
            )),
            ..Declared::default()
        },
    }
}

/// Who a reveal or copy is recorded for, as Activity names them: Claude
/// when the click came from Live view while Live view was driving, else the
/// person — the configured viewer name (`HOMELAB_ADMIN_VIEWER_NAME`), else
/// the Cloudflare Access login, else "a viewer". A label, never a
/// credential.
pub fn actor(
    driven: bool,
    live_view_active: bool,
    viewer_name: Option<&str>,
    access_email: Option<&str>,
) -> String {
    if driven && live_view_active {
        return "Claude (Live view)".into();
    }
    viewer_name
        .filter(|n| !n.trim().is_empty())
        .or(access_email.filter(|e| !e.trim().is_empty()))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "a viewer".into())
}

/// fix-211: a host before 3.71.0 refuses a mutating command carrying a
/// field it does not know. Seen on a reveal, the dashboard asks again
/// without the audit field (the host still writes its own audit.log line),
/// so a version difference never blocks a reveal (invariant 14).
pub fn refused_for_audit_field(message: &str) -> bool {
    message.contains("this host's build does not know") && message.contains("audit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redesign_371_declared_reads_the_names_and_never_hides_a_broken_file() {
        let ok = declared(Some(
            "stack_name: gateway\nlatch_secrets: [traefik, cloudflared]\n",
        ));
        assert_eq!(ok.secrets, vec!["traefik", "cloudflared"]);
        assert!(ok.unreadable.is_none());
        assert_eq!(declared(None), Declared::default());
        let broken = declared(Some("stack_name: [unclosed\n"));
        assert!(broken.secrets.is_empty());
        let why = broken.unreadable.expect("a broken file is unreadable");
        assert!(why.contains(" :: "), "why :: fix, {why}");
        let wrong = declared(Some("latch_secrets: 5\n"));
        assert!(wrong.unreadable.is_some());
    }

    #[test]
    fn redesign_371_actor_names_claude_only_for_a_driven_click_during_live_view() {
        assert_eq!(actor(true, true, Some("Kenny"), None), "Claude (Live view)");
        // A script's click while nobody drives is the person's own.
        assert_eq!(actor(true, false, Some("Kenny"), None), "Kenny");
        assert_eq!(actor(false, true, Some("Kenny"), None), "Kenny");
        assert_eq!(
            actor(false, false, None, Some("someone@example.com")),
            "someone@example.com"
        );
        assert_eq!(actor(false, false, Some(" "), None), "a viewer");
    }

    #[test]
    fn redesign_371_an_old_hosts_refusal_of_the_audit_field_is_recognised() {
        assert!(refused_for_audit_field(
            "refused: this host's build does not know audit — update the host ('homelab release-update') and try again"
        ));
        assert!(!refused_for_audit_field("no sealed copy of this secret"));
    }
}
