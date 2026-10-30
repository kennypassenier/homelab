//! What the nightly round does about a newer release of a managed tool.
//! Pure: the decision, not the installing.

use super::version::{Step, Version, step};

/// How a tool follows its releases (Kenny, 2026-10-30: "nieuwste, major
/// wacht"). `Latest` keeps minor and patch current by itself and holds a
/// major for a person; `Pinned` never moves by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    Latest,
    Pinned,
}

/// What to do about the newest signed release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Installed is current (or newer than anything released).
    Current,
    /// Install it now, with the checks and the rollback.
    Install(Version),
    /// A newer release exists and waits for a person to press the button;
    /// `major` says whether it may carry breaking changes.
    Hold { version: Version, major: bool },
}

pub fn decide(follow: Follow, installed: Version, newest: Version) -> Verdict {
    match (step(installed, newest), follow) {
        (Step::None, _) => Verdict::Current,
        (Step::Major, _) => Verdict::Hold {
            version: newest,
            major: true,
        },
        (_, Follow::Pinned) => Verdict::Hold {
            version: newest,
            major: false,
        },
        (_, Follow::Latest) => Verdict::Install(newest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(t: &str) -> Version {
        Version::parse(t).unwrap()
    }

    #[test]
    fn latest_installs_minor_and_patch_by_itself() {
        assert_eq!(
            decide(Follow::Latest, v("2.7.0"), v("2.7.1")),
            Verdict::Install(v("2.7.1"))
        );
        assert_eq!(
            decide(Follow::Latest, v("2.7.0"), v("2.8.0")),
            Verdict::Install(v("2.8.0"))
        );
    }

    #[test]
    fn a_major_always_waits_for_a_person() {
        for follow in [Follow::Latest, Follow::Pinned] {
            assert_eq!(
                decide(follow, v("2.7.0"), v("3.0.0")),
                Verdict::Hold {
                    version: v("3.0.0"),
                    major: true
                }
            );
        }
    }

    #[test]
    fn pinned_reports_a_newer_release_without_installing_it() {
        assert_eq!(
            decide(Follow::Pinned, v("2.7.0"), v("2.7.1")),
            Verdict::Hold {
                version: v("2.7.1"),
                major: false
            }
        );
    }

    #[test]
    fn nothing_newer_is_current() {
        assert_eq!(
            decide(Follow::Latest, v("2.7.0"), v("2.7.0")),
            Verdict::Current
        );
        assert_eq!(
            decide(Follow::Latest, v("2.7.0"), v("2.6.0")),
            Verdict::Current
        );
    }
}
