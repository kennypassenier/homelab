//! dashboard-latest: the "release tag" dropdown of update-host and
//! install-native, built from GitHub's release list, and how a picked
//! value resolves to the concrete tag a preview and a job carry (never the
//! word "latest" — the review line and the CLI parity line both show what
//! was actually installed).

use homelab_core::ops::native::ReleaseListItem;

/// The dropdown's top value: "the newest release", resolved to a concrete
/// tag before anything is sent to the host.
pub const LATEST: &str = "latest";

/// One option of the "release tag" dropdown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReleaseChoice {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

/// The dropdown: "latest (now vX.Y.Z)" first (always selectable — a
/// resolution, not one release), then every release GitHub lists, newest
/// first as `releases` already gives them; a release without
/// `SHA256SUMS.minisig` (fix-29) is listed disabled, labelled "(unsigned)".
/// Empty when the repository has no release at all.
pub fn dropdown(releases: &[ReleaseListItem]) -> Vec<ReleaseChoice> {
    let Some(newest) = releases.first() else {
        return Vec::new();
    };
    let mut out = vec![ReleaseChoice {
        value: LATEST.to_string(),
        label: format!("latest (now {})", newest.tag),
        disabled: false,
    }];
    out.extend(releases.iter().map(|r| ReleaseChoice {
        value: r.tag.clone(),
        label: if r.signed {
            r.tag.clone()
        } else {
            format!("{} (unsigned)", r.tag)
        },
        disabled: !r.signed,
    }));
    out
}

/// The concrete tag a chosen value resolves to. `latest` and the empty
/// string (a driven `ui type act-tag` left blank, the pre-dropdown
/// meaning) both resolve to the newest release in `releases`; any other
/// value is sent through unchanged — a release the list does not carry
/// (not yet fetched, or unsigned) is still refused where it is installed,
/// not here.
pub fn resolve(chosen: &str, releases: &[ReleaseListItem]) -> Option<String> {
    let chosen = chosen.trim();
    if chosen.is_empty() || chosen == LATEST {
        return releases.first().map(|r| r.tag.clone());
    }
    Some(chosen.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<ReleaseListItem> {
        vec![
            ReleaseListItem {
                tag: "v2.0.0".into(),
                signed: false,
            },
            ReleaseListItem {
                tag: "v1.9.0".into(),
                signed: true,
            },
            ReleaseListItem {
                tag: "v1.8.0".into(),
                signed: true,
            },
        ]
    }

    #[test]
    fn latest_is_first_and_unsigned_releases_are_disabled() {
        let d = dropdown(&items());
        assert_eq!(
            d.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
            vec!["latest", "v2.0.0", "v1.9.0", "v1.8.0"]
        );
        assert_eq!(d[0].label, "latest (now v2.0.0)");
        assert!(!d[0].disabled);
        assert!(d[1].disabled, "an unsigned release is not selectable");
        assert!(d[1].label.contains("unsigned"));
        assert!(!d[2].disabled);
        // The positive twin: a signed release's label is just its tag.
        assert_eq!(d[2].label, "v1.9.0");
        assert!(!d[2].label.contains("unsigned"));
        assert!(!d[3].disabled);
    }

    #[test]
    fn a_repository_with_no_release_has_no_dropdown() {
        assert!(dropdown(&[]).is_empty());
    }

    #[test]
    fn latest_and_empty_resolve_to_the_newest_release_never_the_word_latest() {
        assert_eq!(resolve("latest", &items()).as_deref(), Some("v2.0.0"));
        assert_eq!(resolve("", &items()).as_deref(), Some("v2.0.0"));
        assert_eq!(resolve("  ", &items()).as_deref(), Some("v2.0.0"));
        assert_eq!(resolve("v1.8.0", &items()).as_deref(), Some("v1.8.0"));
        assert_eq!(resolve("latest", &[]), None);
    }
}
