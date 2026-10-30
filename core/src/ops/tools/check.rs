//! Reading a tool's installed version and the newest signed release, and what
//! the follow rule says to do about the difference.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::ops::guards::fetch;
use crate::ops::native::{ReleaseRefs, newest_signed_release};
use crate::tools::policy::{Follow, Verdict, decide};
use crate::tools::spec::ToolSpec;
use crate::tools::version::Version;

/// How many releases to look back through for the newest signed one.
const RELEASE_LIST_LEN: u32 = 10;

/// What one tool looks like right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    pub name: String,
    pub installed: Option<Version>,
    pub newest: Version,
    pub verdict: Verdict,
}

/// The last whitespace-separated word of `<tool> --version`, e.g.
/// `latch 2.7.0` gives `2.7.0`. `Ok(None)` when the program is not there.
pub async fn installed_version(
    exec: &dyn Executor,
    spec: &ToolSpec,
) -> Result<Option<Version>, CoreError> {
    let out = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                &format!("test -x {p} && {p} --version", p = spec.path),
            ],
            30,
        ))
        .await?;
    if !out.success() {
        return Ok(None);
    }
    let word = out.stdout.split_whitespace().last().unwrap_or_default();
    Version::parse(word).map(Some).map_err(|e| {
        CoreError::Other(format!(
            "{} --version answered '{}' :: {e}",
            spec.path,
            out.stdout.trim()
        ))
    })
}

/// The newest signed release that carries this tool's asset.
pub async fn newest_release(
    exec: &dyn Executor,
    spec: &ToolSpec,
) -> Result<(Version, ReleaseRefs), CoreError> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page={RELEASE_LIST_LEN}",
        spec.release_repo
    );
    let list = fetch(exec, &url, "the release list").await?;
    let refs = newest_signed_release(&list, &spec.asset).map_err(CoreError::Other)?;
    let version = Version::parse(&refs.tag).map_err(CoreError::Other)?;
    Ok((version, refs))
}

/// Compare what is installed with the newest release under the follow rule.
/// A tool that is not installed at all is installed, whatever its rule says:
/// there is nothing a major could break.
pub fn judge(follow: Follow, installed: Option<Version>, newest: Version) -> Verdict {
    match installed {
        None => Verdict::Install(newest),
        Some(have) => decide(follow, have, newest),
    }
}

pub async fn status(exec: &dyn Executor, spec: &ToolSpec) -> Result<ToolStatus, CoreError> {
    let installed = installed_version(exec, spec).await?;
    let (newest, _) = newest_release(exec, spec).await?;
    Ok(ToolStatus {
        name: spec.name.clone(),
        installed,
        newest,
        verdict: judge(spec.follow, installed, newest),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(t: &str) -> Version {
        Version::parse(t).unwrap()
    }

    #[test]
    fn a_missing_tool_is_installed_whatever_its_rule() {
        assert_eq!(
            judge(Follow::Pinned, None, v("2.7.0")),
            Verdict::Install(v("2.7.0"))
        );
        assert_eq!(
            judge(Follow::Latest, None, v("3.0.0")),
            Verdict::Install(v("3.0.0"))
        );
    }

    #[test]
    fn an_installed_tool_follows_the_rule() {
        assert_eq!(
            judge(Follow::Latest, Some(v("2.7.0")), v("3.0.0")),
            Verdict::Hold {
                version: v("3.0.0"),
                major: true
            }
        );
        assert_eq!(
            judge(Follow::Latest, Some(v("2.7.0")), v("2.7.0")),
            Verdict::Current
        );
    }
}
