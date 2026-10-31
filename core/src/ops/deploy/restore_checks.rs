//! Remembering which empty directories could not be checked against a backup.

use crate::ops::OpCtx;
use crate::sink::{Level, PipelineEvent};

/// fix-147 (restore-check-failure, 2026-09-27): remember each empty
/// directory or native unit whose backup could not be checked, and forget
/// each one whose check succeeded. Keys are `<stack>:<path or unit>`. A
/// failure to record is said, never fatal: it must not block a deploy either.
pub(crate) async fn record_restore_checks(
    ctx: &OpCtx<'_>,
    stack: &str,
    failed: &[(String, String)],
    ok: &[String],
) {
    if failed.is_empty() && ok.is_empty() {
        return;
    }
    let now = ctx.now_unix;
    let recorded = crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .update(|s| {
            for what in ok {
                s.restore_check_failures
                    .remove(&format!("{}:{}", stack, what));
            }
            for (what, why) in failed {
                s.restore_check_failures.insert(
                    format!("{}:{}", stack, what),
                    crate::state::RestoreCheckFailure {
                        stack: stack.to_string(),
                        what: what.clone(),
                        at: now,
                        why: why.clone(),
                    },
                );
            }
        })
        .await;
    if let Err(e) = recorded {
        ctx.sink.emit(PipelineEvent::Line {
            level: Level::Warn,
            source: "HOST".into(),
            msg: format!(
                "[e3] could not record the backup checks of {} :: {}",
                stack, e
            ),
        });
    }
}
