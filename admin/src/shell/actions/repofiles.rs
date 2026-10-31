//! [`RepoFiles`] as [`StackFiles`]: each method hands over to the reader
//! that knows it (`repo_git`, `repo_stacks`).

use homelab_core::ops::deployguard::Ancestry;

use super::{CommitInfo, LocalStacks, RepoFiles, StackFiles};
use crate::core::actions::{ActionKind, Material, Refusal};

impl StackFiles for RepoFiles {
    fn present(&self) -> bool {
        self.repo.join("stacks").is_dir()
    }

    fn read(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal> {
        self.read_stack(stack, kind, commit)
    }

    fn ancestry(&self, commit: &str) -> Ancestry {
        self.ancestry_of(commit)
    }

    fn commits(&self, stack: &str, limit: usize) -> Result<Vec<CommitInfo>, Refusal> {
        self.commits_of(stack, limit)
    }

    fn native_units(&self, stack: &str) -> Vec<String> {
        homelab_client::spec::native_services(&self.repo.join("stacks").join(stack))
            .into_iter()
            .map(|(m, _)| m.unit)
            .collect()
    }

    fn native_release(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<(homelab_proto::NativeServiceManifest, String, String), Refusal> {
        self.native_release_of(stack, unit)
    }

    fn local_stacks(&self, with_specs: bool) -> Result<LocalStacks, Refusal> {
        self.local_stacks_of(with_specs)
    }

    fn stack_files(&self, stack: &str) -> Result<Vec<homelab_proto::FileBlob>, Refusal> {
        self.stack_files_of(stack)
    }
}
