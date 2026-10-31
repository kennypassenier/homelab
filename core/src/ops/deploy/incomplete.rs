//! Marking a deploy that stopped part-way (S2).

use crate::manifest::StackManifest;
use crate::ops::OpCtx;
use crate::sink::{Level, PipelineEvent};
use crate::state::{StackState, StateStore};

/// Record that a deploy stopped part-way, so the stack is at least visible.
///
/// Deliberately silent about its own failures: this runs while another error
/// is already on its way to the operator, and a second one stacked on top of
/// it helps nobody. The worst case is the state we had before — no record —
/// which is exactly what this exists to avoid, so it is worth attempting and
/// not worth escalating.
pub(super) async fn mark_incomplete(
    exec: &dyn crate::executor::Executor,
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    step: &str,
) {
    // A1 promises that a refused target runs ZERO commands, and D10 promises
    // the same for a manifest that does not validate. Writing a state record
    // is a command. These four steps all run before anything on the machine
    // has been touched, so a failure in them leaves nothing half-done and
    // there is nothing to record — recording anyway would both break the
    // guarantee and invent a stack the operator never deployed.
    const BEFORE_ANYTHING_CHANGES: [&str; 4] = [
        "validate",
        "safety gates",
        "registry cache",
        "hardware readiness",
    ];
    if BEFORE_ANYTHING_CHANGES.contains(&step) {
        return;
    }
    let store = StateStore::new(exec, &ctx.state_dir);
    let Ok(mut state) = store.load().await else {
        return;
    };
    match state.stacks.get_mut(&m.stack_name) {
        // Known already: keep everything, just mark where it stopped. The
        // manifest on record stays the last one that fully applied, because
        // that is what actually ran — not what this attempt wanted.
        Some(existing) => existing.incomplete_step = Some(step.to_string()),
        // Never recorded: write the minimum that makes it exist. Nightly
        // backups key off this entry, and 12 GB of configuration with no
        // backup was how this defect announced itself.
        None => {
            state.stacks.insert(
                m.stack_name.clone(),
                StackState {
                    applied_source: None,
                    vmid: m.vmid,
                    hostname: m.hostname.clone(),
                    apps: m.apps.clone(),
                    applied_at: ctx.now_unix,
                    last_backup: 0,
                    applied_hash: String::new(),
                    manifest: Some(m.clone()),
                    enabled: true,
                    route_file: None,
                    extra_route_files: Vec::new(),
                    natives: Vec::new(),
                    incomplete_step: Some(step.to_string()),
                    pushed_file_hashes: std::collections::BTreeMap::new(),
                    component_digests: Default::default(),
                },
            );
        }
    }
    let _ = store.save(state).await;
    ctx.sink.emit(PipelineEvent::Line {
        level: Level::Warn,
        source: "HOST".into(),
        msg: format!(
            "[state] '{}' recorded as incomplete at step '{}' — it is managed, \
             but what is on the container is not what the stack file says",
            m.stack_name, step
        ),
    });
}
