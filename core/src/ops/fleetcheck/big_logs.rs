//! Log files on a data mount that are past any size rotation would allow.

use super::{Finding, Severity};

/// fix-24: one oversized log file on a data mount.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BigLogFact {
    pub path: String,
    pub bytes: u64,
    /// Whether the mount declares `rotate:` — a big file under a declared
    /// rule means the rule is not running, which is a different remedy.
    pub rotated: bool,
}

/// fix-24: 500 MB. Traefik's access log reached 254 MB in 25 unrotated days,
/// and the largest rotated day on record is 84 MB, so a file past this is a
/// rotation that is missing or not running, not a busy day.
pub const BIG_LOG_BYTES: u64 = 500 * 1024 * 1024;

/// fix-24: parse `find -printf '%s %p\n'` output into facts, marking the ones
/// under a mount that declares rotation.
pub fn big_log_facts(transcript: &str, rotated_prefixes: &[String]) -> Vec<BigLogFact> {
    transcript
        .lines()
        .filter_map(|l| {
            let (size, path) = l.trim().split_once(' ')?;
            let bytes = size.parse::<u64>().ok()?;
            (bytes >= BIG_LOG_BYTES).then(|| BigLogFact {
                path: path.to_string(),
                bytes,
                rotated: rotated_prefixes
                    .iter()
                    .any(|p| path.starts_with(&format!("{}/", p.trim_end_matches('/')))),
            })
        })
        .collect()
}

/// fix-24: a Drift finding per oversized log.
pub fn evaluate_big_logs(facts: &[BigLogFact]) -> Vec<Finding> {
    facts
        .iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: f.path.clone(),
            what: format!("log file is {} MB", f.bytes / 1024 / 1024),
            remedy: if f.rotated {
                "the stack declares rotation for it, so the rule is not running — check \
                 `/etc/logrotate.d/homelab-<stack>` inside the container and `systemctl status logrotate.timer`"
                    .into()
            } else {
                "nothing rotates it — declare `rotate:` on this data mount in the stack file \
                 and deploy"
                    .into()
            },
        })
        .collect()
}
