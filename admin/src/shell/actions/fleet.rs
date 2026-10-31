//! What the newest fleet reading says about a stack.

use super::Actions;

impl Actions {
    /// The host's record of whether the stack runs adopted natives.
    pub(super) fn fleet_native(&self, stack: &str) -> Option<bool> {
        let s = self.inner.shared.try_read().ok()?;
        s.fleet
            .as_ref()?
            .stacks
            .iter()
            .find(|x| x.name == stack)
            .map(|x| x.native)
    }

    pub(super) fn fleet_stack(&self, stack: &str) -> Option<(u16, Option<String>)> {
        let s = self.inner.shared.try_read().ok()?;
        s.fleet
            .as_ref()?
            .stacks
            .iter()
            .find(|x| x.name == stack)
            .map(|x| (x.vmid, x.applied_source.clone()))
    }

    /// The host's applied hashes, from the newest fleet reading.
    pub(super) fn host_pairs(&self) -> Option<Vec<(String, String)>> {
        let s = self.inner.shared.try_read().ok()?;
        Some(
            s.fleet
                .as_ref()?
                .stacks
                .iter()
                .map(|x| (x.name.clone(), x.applied_hash.clone()))
                .collect(),
        )
    }

    /// fix-192: the host's applied component digests, alongside
    /// `host_pairs` — a stack applied by a host from before this field
    /// existed carries the all-empty default, which `redeploy_reason` reads
    /// as "no component digests recorded" rather than a real comparison.
    pub(super) fn host_digests(
        &self,
    ) -> Option<Vec<(String, homelab_core::manifest::ComponentDigests)>> {
        let s = self.inner.shared.try_read().ok()?;
        Some(
            s.fleet
                .as_ref()?
                .stacks
                .iter()
                .map(|x| (x.name.clone(), x.component_digests.clone()))
                .collect(),
        )
    }
}
