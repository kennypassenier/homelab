//! Storage directories owned by someone other than their stack file says.

use super::{Finding, Severity};

/// fix-26: one storage directory owned by someone other than its stack file
/// says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnerFact {
    pub path: String,
    pub stack: String,
    pub declared: u32,
    pub actual: u32,
}

/// fix-26: parse `stat -c '%u %n'` lines against the declarations. A path
/// stat could not read produces no line and no fact: a missing directory is
/// the deploy's business, not this check's.
pub fn owner_facts(transcript: &str, declared: &[(String, u32, String)]) -> Vec<OwnerFact> {
    let mut out = Vec::new();
    for line in transcript.lines() {
        let Some((uid, path)) = line.trim().split_once(' ') else {
            continue;
        };
        let Ok(actual) = uid.parse::<u32>() else {
            continue;
        };
        for (p, want, stack) in declared {
            if p == path && *want != actual {
                out.push(OwnerFact {
                    path: path.to_string(),
                    stack: stack.clone(),
                    declared: *want,
                    actual,
                });
            }
        }
    }
    out
}

/// fix-26: Drift per mismatch. Not Broken: the service may run fine today,
/// and the finding is there to be read before a restart surfaces it. Story:
/// `docs/deployment/REGISTER.md`.
pub fn evaluate_owners(facts: &[OwnerFact]) -> Vec<Finding> {
    facts
        .iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: format!("{} ({})", f.path, f.stack),
            what: format!(
                "owned by uid {} on disk, while the stack file declares host_owner_uid {}",
                f.actual, f.declared
            ),
            remedy: "the next deploy chowns it to the declared uid — if the service runs as \
                     its own user, correct host_owner_uid in the stack file first (100000 + \
                     the uid inside an unprivileged container), or it stops at its next restart"
                .into(),
        })
        .collect()
}
