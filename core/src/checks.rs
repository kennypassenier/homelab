//! Per-service health checks, compared as a before/after pair (J1-J3).
//!
//! Kenny's objection killed the obvious design. The first proposal was a
//! threshold per service — "Jellyfin reports at least 900 films" — and he
//! asked what happens when he deletes half his library himself. A fixed floor
//! is an assumption about his data that stops being true, and then it alarms
//! about his own housekeeping. That is how a check gets switched off, and a
//! check that is off protects nothing.
//!
//! So there are no thresholds here. A check measures the same thing twice —
//! once before the work, once after — and judges the PAIR. "May rise, never
//! fall" needs no tolerance when the two readings are minutes apart, and it
//! stays true whatever the absolute numbers are.
//!
//! That timing is what makes the rule valid, and it is why a stored baseline
//! is deliberately not supported: over a month "never falls" is simply false,
//! and a check that needs a fudge factor is a check nobody trusts.
//!
//! The checks live beside their service rather than with their stack, because
//! a service moves between stacks and its notion of healthy moves with it
//! (Kenny, form J2). Uptime Kuma left the gateway the same day this was
//! decided, which is the argument in one sentence.

use serde::{Deserialize, Serialize};

/// How a measurement is allowed to change between the two readings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expect {
    /// May grow, never shrink. For anything counted: films, series, indexers,
    /// monitors. The downloader was importing throughout the media rebuild,
    /// so "equal" would have been wrong and "at least N" would have been a
    /// guess.
    NeverDecreases,
    /// Must be identical. For things that describe configuration rather than
    /// content: library paths, the transcoding device, a version.
    MustMatch,
    /// Must be non-empty afterwards, whatever it was before. For a reading
    /// that has no meaningful before — a fresh container has no ffmpeg log.
    MustBePresent,
}

/// One thing worth measuring about a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// Shown to Kenny, so it says what it means: "films in the library",
    /// not "items_count_movies".
    pub name: String,
    /// Run inside the container. Its stdout, trimmed, is the reading.
    pub command: String,
    pub expect: Expect,
    /// What this check does NOT prove, in one line. The restore drill's most
    /// useful section was exactly this, and without it a passing check reads
    /// as a guarantee it is not.
    #[serde(default)]
    pub blind_spot: Option<String>,
}

/// The whole file that sits beside a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServiceChecks {
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Things no measurement can settle: whether a film looks right on the
    /// television, whether the sound is in sync. Kenny asked for these
    /// explicitly — "ik ga het vergeten met mijn adhd" — and they reach him
    /// as a notification he has to acknowledge rather than a page he has to
    /// go and find (form I2).
    #[serde(default)]
    pub manual: Vec<String>,
}

/// One measurement, taken twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub name: String,
    pub before: String,
    pub after: String,
    pub expect: Expect,
    pub blind_spot: Option<String>,
}

/// What the pair says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Unchanged, or changed in the direction that is allowed.
    Ok,
    /// Changed in a way the service said it must not.
    Regressed(String),
    /// The reading could not be taken at all. Not the same as a regression:
    /// a command that fails to run says nothing about the data, and reporting
    /// it as a regression is how a check earns a reputation for crying wolf.
    Unreadable(String),
}

/// Judge one pair. Pure, so the interesting half needs no container.
pub fn judge(r: &Reading) -> Verdict {
    if r.after.trim().is_empty() {
        return Verdict::Unreadable(format!(
            "'{}' could not be read after the work (it read '{}' before)",
            r.name, r.before
        ));
    }
    match r.expect {
        Expect::MustBePresent => Verdict::Ok,
        Expect::MustMatch => {
            if r.before.trim() == r.after.trim() {
                Verdict::Ok
            } else {
                Verdict::Regressed(format!(
                    "'{}' was '{}' and is now '{}'",
                    r.name,
                    r.before.trim(),
                    r.after.trim()
                ))
            }
        }
        Expect::NeverDecreases => {
            // Both sides must parse as numbers. If the BEFORE reading could
            // not be taken — a container that did not exist yet — there is
            // nothing to compare against and nothing to complain about.
            let before: i64 = match r.before.trim().parse() {
                Ok(v) => v,
                Err(_) if r.before.trim().is_empty() => return Verdict::Ok,
                Err(_) => {
                    return Verdict::Unreadable(format!(
                        "'{}' expects a number and read '{}' before",
                        r.name,
                        r.before.trim()
                    ))
                }
            };
            let after: i64 = match r.after.trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    return Verdict::Unreadable(format!(
                        "'{}' expects a number and read '{}' after",
                        r.name,
                        r.after.trim()
                    ))
                }
            };
            if after >= before {
                Verdict::Ok
            } else {
                Verdict::Regressed(format!("'{}' fell from {} to {}", r.name, before, after))
            }
        }
    }
}

/// Judge the lot, and say what the passing ones do not prove.
///
/// The blind spots are returned even when everything passes, because that is
/// the moment they matter: a green report is exactly when someone stops
/// asking what was not checked.
pub fn judge_all(readings: &[Reading]) -> (Vec<Verdict>, Vec<String>) {
    let verdicts: Vec<Verdict> = readings.iter().map(judge).collect();
    let blind = readings
        .iter()
        .filter_map(|r| r.blind_spot.clone())
        .collect();
    (verdicts, blind)
}

/// Did anything actually go backwards? Unreadable is deliberately not a
/// failure here: it is reported, but it does not block, because a command
/// that will not run is a fault in the check rather than in the service.
pub fn regressions(verdicts: &[Verdict]) -> Vec<String> {
    verdicts
        .iter()
        .filter_map(|v| match v {
            Verdict::Regressed(why) => Some(why.clone()),
            _ => None,
        })
        .collect()
}
