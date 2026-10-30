//! Release versions of a managed tool, and what kind of step one is from
//! another. Pure: no I/O, no clock.

use std::cmp::Ordering;

/// `major.minor.patch`, the only shape a managed tool's releases use. A
/// leading `v` is dropped; a pre-release or build suffix (`-rc.1`, `+abc`)
/// makes the tag not a plain release and is refused rather than guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Result<Version, String> {
        let t = text.trim();
        let t = t.strip_prefix('v').unwrap_or(t);
        let mut parts = t.split('.');
        let mut next = |what: &str| -> Result<u64, String> {
            parts
                .next()
                .ok_or_else(|| {
                    format!("'{text}' has no {what} number :: expected major.minor.patch")
                })?
                .parse::<u64>()
                .map_err(|_| format!("'{text}' is not a plain major.minor.patch version"))
        };
        let v = Version {
            major: next("major")?,
            minor: next("minor")?,
            patch: next("patch")?,
        };
        if parts.next().is_some() {
            return Err(format!("'{text}' has more than three numbers"));
        }
        Ok(v)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// How far a candidate is from what is installed. Ordered by how much can
/// break: a `Major` step may carry breaking changes (semver), the rest may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// Same version, or the candidate is older.
    None,
    Patch,
    Minor,
    Major,
}

/// The step from `installed` to `candidate`.
pub fn step(installed: Version, candidate: Version) -> Step {
    match candidate.cmp(&installed) {
        Ordering::Less | Ordering::Equal => Step::None,
        Ordering::Greater if candidate.major != installed.major => Step::Major,
        Ordering::Greater if candidate.minor != installed.minor => Step::Minor,
        Ordering::Greater => Step::Patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(t: &str) -> Version {
        Version::parse(t).unwrap()
    }

    #[test]
    fn parses_a_tag_with_or_without_the_v() {
        assert_eq!(v("v2.7.0"), v("2.7.0"));
        assert_eq!(v("2.7.0").to_string(), "2.7.0");
    }

    #[test]
    fn refuses_what_is_not_a_plain_release() {
        for bad in ["2.7", "2.7.0.1", "2.7.0-rc.1", "latest", "", "1.x.0"] {
            assert!(Version::parse(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn classifies_the_step_by_the_first_number_that_moved() {
        assert_eq!(step(v("2.7.0"), v("2.7.1")), Step::Patch);
        assert_eq!(step(v("2.7.0"), v("2.8.0")), Step::Minor);
        assert_eq!(step(v("2.7.0"), v("3.0.0")), Step::Major);
        assert_eq!(step(v("2.7.5"), v("3.0.0")), Step::Major);
    }

    #[test]
    fn numbers_compare_as_numbers_not_text() {
        assert_eq!(step(v("2.9.0"), v("2.10.0")), Step::Minor);
        assert!(v("1.0.27") < v("2.7.0"));
    }

    #[test]
    fn the_same_or_an_older_release_is_no_step() {
        assert_eq!(step(v("2.7.0"), v("2.7.0")), Step::None);
        assert_eq!(step(v("2.7.0"), v("1.0.27")), Step::None);
    }
}
