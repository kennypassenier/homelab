//! Reading a native service's recorded state.

use crate::*;

/// C7: look up an adopted native stack's manifest in state. Error strings
/// carry the remedy, per standing rule 11.
pub(crate) async fn native_from_state(
    state_dir: &str,
    stack: &str,
) -> Result<(Vec<homelab_core::native::NativeServiceManifest>, u64), String> {
    let store = homelab_core::state::StateStore::new(&RealExecutor, state_dir);
    let snapshot = store
        .load()
        .await
        .map_err(|e| format!("state unreadable: {}", e))?;
    match snapshot.stacks.get(stack) {
        // The timestamp rides along so a skip can say how old the copy it
        // read is — the difference between a decision and a stale field.
        Some(st) if st.is_native() => Ok((st.natives.clone(), st.applied_at)),
        Some(_) => Err(format!(
            "stack '{}' is a compose stack, not a native service :: use the regular backup/update verbs",
            stack
        )),
        None => Err(format!(
            "stack '{}' is not in host state :: adopt it first (homelab adopt stacks/{})",
            stack, stack
        )),
    }
}
