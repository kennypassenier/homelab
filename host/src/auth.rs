//! Who is asking: bearer-token checks, identifying a caller and the audit lines for refused or scoped requests.

use crate::*;

/// fix-120 (api-token-is-root, 2026-09-27): compared as SHA-256 digests with
/// every byte folded in, so neither the length nor the first differing byte
/// shows in the time the answer takes. It was a plain `==` on a formatted
/// string, which stops at the first difference.
pub(crate) fn bearer_ok(header: Option<&str>, token: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Some(given) = header else {
        return false;
    };
    let want = Sha256::digest(format!("Bearer {}", token).as_bytes());
    let got = Sha256::digest(given.as_bytes());
    want.iter()
        .zip(got.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// arch-tokens: who is on the other end. The single legacy `token` is scope
/// `all`; a `[[tokens]]` entry matches on the SHA-256 of the presented token,
/// compared with every byte folded in (fix-120) and every entry tried, so
/// neither which entry matched nor where a guess differed shows in the time.
pub(crate) fn identify(
    header: Option<&str>,
    legacy: &str,
    tokens: &[TokenEntry],
) -> Option<Identity> {
    use sha2::{Digest, Sha256};
    if bearer_ok(header, legacy) {
        return Some(Identity {
            name: "legacy".into(),
            scope: homelab_proto::Scope::All,
        });
    }
    let presented = header?.strip_prefix("Bearer ")?;
    let digest: String = Sha256::digest(presented.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    let mut found = None;
    for t in tokens {
        let same = t
            .sha256
            .bytes()
            .zip(digest.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
            && t.sha256.len() == digest.len();
        if same && found.is_none() {
            found = Some(Identity {
                name: t.name.clone(),
                scope: t.scope,
            });
        }
    }
    found
}

/// arch-tokens: the audit line for a command refused for its scope, or run
/// under scope `all`. The command's name only, never its payload.
pub(crate) fn scope_audit_line(
    ts: u64,
    who: &Identity,
    command: &homelab_proto::Command,
    refused: bool,
) -> String {
    format!(
        "{} {} token={} scope={:?} cmd={} needs={:?}\n",
        ts,
        if refused { "refused" } else { "scope-all" },
        who.name,
        who.scope,
        command.name(),
        command.scope()
    )
}

/// fix-120: say that a connection was refused for its token, and from where.
pub(crate) fn log_refused(state: &AppState, peer: SocketAddr, path: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let n = state.auth_failures.record(&peer.to_string(), now);
    tracing::warn!(
        "401 on {} from {}: missing or wrong bearer token ({} refused since this daemon started)",
        path,
        peer,
        n
    );
}
