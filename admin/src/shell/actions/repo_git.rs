//! Reading a stack's files from the working copy, or from an earlier
//! commit of it, and the git questions about that copy.

use std::path::Path;

use homelab_core::ops::deployguard::Ancestry;

use super::stackfiles::no_repo;
use super::{CommitInfo, RepoFiles, StackFiles, is_head};
use crate::core::actions::{ActionKind, Material, Needs, Refusal};

impl RepoFiles {
    pub(super) fn git(&self, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .args(args)
            .output()
            .map_err(|e| format!("git did not run: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    /// Take the remote's newest commits (best effort) before a read.
    pub(super) fn sync_copy(&self, before: &str) {
        if let Some(wc) = &self.wc
            && let Err(r) = wc.sync()
        {
            tracing::warn!(why = %r.why, "the working copy was not brought up to date before {before}");
        }
    }

    /// Hold the working copy still while a guard lives, so a read never
    /// sees a stack an edit is writing.
    pub(super) fn hold_copy(&self) -> Option<impl Sized + use<'_>> {
        self.wc.as_ref().map(|wc| wc.hold())
    }

    fn read_dir(
        &self,
        dir: &Path,
        stack: &str,
        kind: ActionKind,
        what: &str,
    ) -> Result<Material, Refusal> {
        let failed = |why: String| {
            Refusal::new(
                what.to_string(),
                why,
                format!(
                    "fix stacks/{stack} in the working copy; `homelab plan {stack}` says the same"
                ),
            )
        };
        match kind.needs() {
            Needs::Nothing
            | Needs::Vmid
            | Needs::HostRelease
            | Needs::NativeRelease
            | Needs::Apply => Ok(Material::None),
            Needs::Manifest => manifest_material(dir, kind).map_err(failed),
            Needs::Spec => spec_material(dir).map_err(failed),
            Needs::NativeManifest => native_material(dir).map_err(failed),
        }
    }

    pub(super) fn read_stack(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal> {
        let what = format!("{} {}", kind.slug(), stack);
        if matches!(
            kind.needs(),
            Needs::Nothing | Needs::Vmid | Needs::HostRelease | Needs::NativeRelease | Needs::Apply
        ) {
            return Ok(Material::None);
        }
        if commit.is_none() {
            self.sync_copy(&what);
        }
        let _held = self.hold_copy();
        if !self.present() {
            return Err(no_repo(&self.repo, what));
        }
        let dir = self.repo.join("stacks").join(stack);
        let Some(commit) = commit else {
            return self.read_dir(&dir, stack, kind, &what);
        };
        // update-flow-latch-1: the commit the working copy already stands on
        // (the Update flow deploys its own commit right after making it) is
        // read from the working copy itself. A checkout under tmp/ is a
        // directory latch has no link for, so every stack with a latch
        // secret was refused there; the files are the same either way.
        if is_head(self.git(&["rev-parse", "HEAD"]).ok().as_deref(), commit) {
            return self.read_dir(&dir, stack, kind, &what);
        }
        self.read_checkout(stack, kind, commit, &what)
    }

    /// feat-stacks-6: a checkout of that commit beside the working copy,
    /// read like the working copy, removed again whatever happened.
    fn read_checkout(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: &str,
        what: &str,
    ) -> Result<Material, Refusal> {
        let _ = std::fs::create_dir_all(&self.scratch);
        let tree = self
            .scratch
            .join(format!("rollback-{}-{}", std::process::id(), commit));
        let tree_s = tree.display().to_string();
        let _ = self.git(&["worktree", "remove", "--force", &tree_s]);
        self.git(&["worktree", "add", "--detach", &tree_s, commit])
            .map_err(|why| {
                Refusal::new(
                    what.to_string(),
                    format!("commit {commit} could not be checked out: {why}"),
                    "pick a commit from the roll-back list; `git fetch` in the working copy first if it is new",
                )
            })?;
        let dir = tree.join("stacks").join(stack);
        let out = if dir.is_dir() {
            self.read_dir(&dir, stack, kind, what)
        } else {
            Err(Refusal::new(
                what.to_string(),
                format!("stacks/{stack} does not exist at commit {commit}"),
                "pick a commit that has the stack",
            ))
        };
        let _ = self.git(&["worktree", "remove", "--force", &tree_s]);
        out
    }

    pub(super) fn ancestry_of(&self, commit: &str) -> Ancestry {
        let object = format!("{commit}^{{commit}}");
        if self.git(&["cat-file", "-e", &object]).is_err() {
            Ancestry::Unknown
        } else if self
            .git(&["merge-base", "--is-ancestor", commit, "HEAD"])
            .is_ok()
        {
            Ancestry::Contained
        } else {
            Ancestry::Diverged
        }
    }

    pub(super) fn commits_of(&self, stack: &str, limit: usize) -> Result<Vec<CommitInfo>, Refusal> {
        if !self.present() {
            return Err(no_repo(&self.repo, format!("roll-back list of {stack}")));
        }
        let n = format!("-n{}", limit.clamp(1, 200));
        let path = format!("stacks/{stack}");
        let out = self
            .git(&["log", &n, "--format=%H%x09%ct%x09%s", "--", &path])
            .map_err(|why| {
                Refusal::new(
                    format!("roll-back list of {stack}"),
                    format!("git log failed: {why}"),
                    "check the working copy",
                )
            })?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut p = l.splitn(3, '\t');
                Some(CommitInfo {
                    commit: p.next()?.to_string(),
                    at: p.next()?.parse().ok()?,
                    subject: p.next().unwrap_or("").to_string(),
                })
            })
            .collect())
    }
}

fn manifest_material(dir: &Path, kind: ActionKind) -> Result<Material, String> {
    if kind == ActionKind::Destroy && !dir.join("lxc-compose.yml").exists() {
        return Ok(Material::None);
    }
    homelab_client::spec::build_manifest(dir).map(|m| Material::Manifest(Box::new(m)))
}

fn spec_material(dir: &Path) -> Result<Material, String> {
    let spec = homelab_client::spec::build_spec(dir)?;
    homelab_core::manifest::validate(&spec).map_err(|e| format!("validation failed: {e}"))?;
    Ok(Material::Spec(Box::new(spec)))
}

fn native_material(dir: &Path) -> Result<Material, String> {
    let path = dir.join("service.yml");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let m: homelab_proto::NativeServiceManifest =
        serde_yaml::from_str(&raw).map_err(|e| format!("service.yml parse: {e}"))?;
    homelab_core::native::validate_native(&m)
        .map_err(|p| format!("service.yml invalid: {}", p.join("; ")))?;
    Ok(Material::Native(Box::new(m)))
}
