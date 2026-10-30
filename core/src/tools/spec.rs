//! One managed tool as `host.toml` declares it. Everything about a tool is
//! data here, so the code holds no knowledge of any particular program.

use super::policy::Follow;
use serde::{Deserialize, Serialize};

/// `[tools.<name>]` in `host.toml`, e.g. latch:
///
/// ```toml
/// [tools.latch]
/// release_repo = "kennypassenier/latch-rs"
/// asset = "latch-x86_64-unknown-linux-gnu"
/// path = "/usr/local/bin/latch"
/// follow = "latest"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ToolSpec {
    /// The key under `[tools]`; filled in by the loader, not written in the file.
    #[serde(default, skip_serializing)]
    pub name: String,
    /// `owner/repo` on GitHub whose signed releases carry the tool.
    pub release_repo: String,
    /// The file name of the release asset for this machine.
    pub asset: String,
    /// Where the program lives; the previous one is kept beside it as `<path>.prev`.
    pub path: String,
    /// `latest` (minor and patch by themselves, a major waits) or `pinned`.
    #[serde(default = "default_follow")]
    pub follow: Follow,
    /// The version a `pinned` tool is held at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

fn default_follow() -> Follow {
    Follow::Latest
}

impl ToolSpec {
    /// Refuse a spec that could not be installed, before anything runs.
    pub fn validate(&self) -> Result<(), String> {
        let plain = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
                && !s.split('/').any(|p| p == "..")
        };
        if self.release_repo.split('/').count() != 2 || !plain(&self.release_repo) {
            return Err(format!(
                "tool '{}': release_repo '{}' must be owner/repo",
                self.name, self.release_repo
            ));
        }
        if !plain(&self.asset) || self.asset.contains('/') {
            return Err(format!(
                "tool '{}': asset '{}' must be a plain file name",
                self.name, self.asset
            ));
        }
        if !self.path.starts_with('/') || !plain(&self.path) {
            return Err(format!(
                "tool '{}': path '{}' must be an absolute path of plain characters",
                self.name, self.path
            ));
        }
        if self.follow == Follow::Pinned && self.version.is_none() {
            return Err(format!(
                "tool '{}': follow = \"pinned\" needs a version",
                self.name
            ));
        }
        Ok(())
    }

    /// Where the previous program is kept for a roll-back.
    pub fn prev_path(&self) -> String {
        format!("{}.prev", self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latch() -> ToolSpec {
        ToolSpec {
            name: "latch".into(),
            release_repo: "kennypassenier/latch-rs".into(),
            asset: "latch-x86_64-unknown-linux-gnu".into(),
            path: "/usr/local/bin/latch".into(),
            follow: Follow::Latest,
            version: None,
        }
    }

    #[test]
    fn the_latch_spec_is_valid() {
        assert_eq!(latch().validate(), Ok(()));
        assert_eq!(latch().prev_path(), "/usr/local/bin/latch.prev");
    }

    #[test]
    fn refuses_what_could_not_be_installed() {
        let mut bad = latch();
        bad.path = "relative/latch".into();
        assert!(bad.validate().is_err());
        let mut bad = latch();
        bad.release_repo = "no-slash".into();
        assert!(bad.validate().is_err());
        let mut bad = latch();
        bad.asset = "../escape".into();
        assert!(bad.validate().is_err());
        let mut bad = latch();
        bad.follow = Follow::Pinned;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn a_missing_follow_means_latest() {
        let spec: ToolSpec =
            serde_json::from_str(r#"{"release_repo":"a/b","asset":"x","path":"/usr/local/bin/x"}"#)
                .unwrap();
        assert_eq!(spec.follow, Follow::Latest);
    }
}
