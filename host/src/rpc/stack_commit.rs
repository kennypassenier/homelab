//! RPC handlers: the dashboard's stack edits, committed and pushed by the
//! host (key-move).

use crate::*;
use homelab_core::ops::stackcommit::StackFileChange;
use homelab_core::ops::stackcommit::guard::request_problems;

fn refused(req_id: u64, message: String) -> RpcResponse {
    RpcResponse {
        id: req_id,
        ok: false,
        message,
        deferred: None,
    }
}

/// `CommitStackFiles`: the core guard, then the transaction on a blocking
/// thread (`stack_commit::commit`), then one audit line naming who asked,
/// the paths and how it ended — never the files' text.
pub(crate) async fn commit_stack_files(
    state: &AppState,
    req_id: u64,
    message: String,
    base: String,
    files: Vec<StackFileChange>,
    push: bool,
) -> RpcResponse {
    let cfg = match &state.config.stack_commits {
        None => {
            return refused(
                req_id,
                "stack commits are off on this host :: add a [stack_commits] table with \
                 key = \"<deploy key path>\" to host.toml and restart the host"
                    .into(),
            );
        }
        Some(Err(why)) => {
            return refused(
                req_id,
                format!("[stack_commits] cannot run: {why} :: fix host.toml"),
            );
        }
        Some(Ok(cfg)) => cfg.clone(),
    };
    let problems = request_problems(&message, &base, &files, &cfg.roots, cfg.max_bytes);
    let by = requested_by();
    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let result = if problems.is_empty() {
        let request = crate::stack_commit::Request {
            message,
            base,
            files,
            push,
        };
        let ran = tokio::task::spawn_blocking(move || {
            crate::stack_commit::commit_noticing(&cfg, &request)
        })
        .await;
        match ran {
            Ok((result, unpushed)) => {
                if let Some(found) = unpushed {
                    crate::stack_commit::notice::record(state, &found);
                }
                result
            }
            Err(e) => Err(format!("the commit thread failed: {e} :: report this")),
        }
    } else {
        Err(format!("{} :: nothing was written", problems.join("; ")))
    };
    audit(state, by.as_deref(), &paths, push, &result);
    match result {
        Ok(done) => match serde_json::to_string(&done) {
            Ok(json) => RpcResponse {
                id: req_id,
                ok: true,
                message: json,
                deferred: None,
            },
            Err(e) => refused(req_id, format!("the answer could not be written: {e}")),
        },
        Err(why) => refused(req_id, why),
    }
}

fn audit(
    state: &AppState,
    by: Option<&str>,
    paths: &[String],
    push: bool,
    result: &Result<homelab_core::ops::stackcommit::StackCommitDone, String>,
) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let outcome = match result {
        Ok(d) => format!("commit={} pushed={}", d.commit, d.pushed),
        Err(why) => format!("refused={:?}", why.lines().next().unwrap_or("")),
    };
    let line = format!(
        "{} commit-stack-files by={} push={} paths={} {}\n",
        ts,
        by.unwrap_or("-"),
        push,
        escaped_paths(paths),
        outcome
    );
    if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &line) {
        tracing::warn!("audit.log: could not record the stack commit :: {}", e);
    }
}

/// The paths as one audit field. A refused request may carry any text, so
/// each path is escaped (`{:?}`): a newline in a path cannot start a forged
/// audit line.
fn escaped_paths(paths: &[String]) -> String {
    paths
        .iter()
        .map(|p| format!("{p:?}"))
        .collect::<Vec<_>>()
        .join(",")
}
