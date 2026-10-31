//! feat-settings-1 / fix-110: write host.toml.
//!
//! Declarative like a stack (fix-110, 2026-10-01): the change is committed
//! to `config/host.toml` in the working copy FIRST — the repository is the
//! record of every non-secret host setting, the same as a stack's files —
//! and only then applied to the host, from that exact committed text
//! (`Command::ApplyHostConfig`, not the per-key `SetHostConfig` the TUI's
//! older settings screen still uses). A host and a repository can then
//! never silently disagree about what was asked for: `homelab check`
//! reports it when they do (a push that is refused, or a save made
//! meanwhile over ssh). If the commit lands but the host refuses or does
//! not answer, the change still exists in the repository, and the error
//! says to run `homelab host apply` once the host accepts it — nothing is
//! lost, only not yet applied.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::http::StatusCode;
use homelab_proto::{Command, RpcResponse};

use super::EditCtx;
use super::host_settings::host_new_enough;
use super::http::{bad_gateway, blocking};
use super::repo::publish_repo;
use crate::core::actions::{self, ActionArgs, ActionKind, Refusal};
use crate::core::hostsettings;
use crate::core::stackedit::FileChange;
use crate::shell::actions::Origin;
use crate::shell::workcopy::Committed;

/// feat-settings-1 / fix-110: write host.toml, for the route and a driven
/// final press: the same check, once.
///
/// Owner decision 2026-09-30 (item 2): when any key just written takes
/// effect only at the host's next start (`Apply::Restart`), the answer
/// also carries a `follow` job that restarts `homelab-host.service` — the
/// same shape `stack_commit`'s `follow_up` gives a deploy, so the generic
/// driven-form job wiring in `shell::drive` picks it up unchanged.
pub async fn save_host_settings(
    c: &EditCtx,
    change: hostsettings::Change,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    host_new_enough(c)
        .await
        .map_err(|r| (StatusCode::SERVICE_UNAVAILABLE, r))?;
    let changes = hostsettings::check(change).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
    let keys: Vec<String> = changes.keys().cloned().collect();
    let (fc, new_text) = prepare_toml(c, changes)
        .await
        .map_err(|r| (StatusCode::BAD_REQUEST, r))?;
    let committed = commit_toml(c, fc, &keys)
        .await
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let expect_sha256 = fresh_host_sha(c).await;
    let r = apply_toml(c, new_text, expect_sha256, keys.clone()).await;
    apply_outcome(c, r, committed, keys, origin)
}

/// The merged `config/host.toml` text, as a change against the file now.
async fn prepare_toml(
    c: &EditCtx,
    changes: BTreeMap<String, serde_json::Value>,
) -> Result<(FileChange, String), Refusal> {
    let wc = c.wc.clone();
    blocking(move || -> Result<(FileChange, String), Refusal> {
        let path = "config/host.toml".to_string();
        let old = std::fs::read_to_string(wc.repo.join(&path)).ok();
        let new = homelab_core::hostconfig::merge_changes(old.as_deref().unwrap_or(""), &changes)
            .map_err(|e| Refusal::new("the host settings", e, "correct the marked fields"))?;
        Ok((
            FileChange {
                path,
                old,
                new: Some(new.clone()),
            },
            new,
        ))
    })
    .await
    .and_then(|r| r)
}

async fn commit_toml(c: &EditCtx, fc: FileChange, keys: &[String]) -> Result<Committed, Refusal> {
    let message = format!(
        "chore(host): {} [fix-110]\n\nFrom the dashboard's host settings page.\n",
        keys.join(", ")
    );
    let wc = c.wc.clone();
    let committed = blocking(move || wc.transact_file(&fc, &message, |_| Vec::new()))
        .await
        .and_then(|r| r);
    publish_repo(c).await;
    committed
}

/// The host's current sha256, fresh — not the page's possibly-stale
/// `expect_sha256` — so an edit made meanwhile (over ssh, or a TUI
/// save) is refused rather than silently overwritten.
async fn fresh_host_sha(c: &EditCtx) -> Option<String> {
    c.host
        .ask_traced(Command::GetHostConfig, Duration::from_secs(20), None)
        .await
        .ok()
        .filter(|r| r.ok)
        .and_then(|r| serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message).ok())
        .map(|f| f.sha256)
}

async fn apply_toml(
    c: &EditCtx,
    toml: String,
    expect_sha256: Option<String>,
    keys: Vec<String>,
) -> Result<RpcResponse, String> {
    c.host
        .ask_traced(
            Command::ApplyHostConfig {
                toml,
                expect_sha256,
                // fix-191: only the keys this page changed may move.
                allow: keys,
            },
            Duration::from_secs(30),
            None,
        )
        .await
}

/// The save's answer from the host's: saved, refused, or silent.
fn apply_outcome(
    c: &EditCtx,
    r: Result<RpcResponse, String>,
    committed: Committed,
    keys: Vec<String>,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let short = &committed.commit[..12.min(committed.commit.len())];
    match r {
        Ok(r) if r.ok => {
            let saved: serde_json::Value = serde_json::from_str(&r.message).unwrap_or_default();
            c.publish.publish(
                "host_settings",
                serde_json::json!({ "saved": saved, "keys": keys }),
            );
            let follow = restart_follow_up(c, &keys, origin);
            Ok(serde_json::json!({ "committed": committed, "saved": saved, "follow": follow }))
        }
        Ok(r) => Err(bad_gateway(
            "the host settings",
            format!(
                "committed to the repository as {short}, but the host refused it: {}",
                r.message
            ),
            "the commit is in the repository; `homelab host apply` once the host accepts it",
        )),
        Err(e) => Err(bad_gateway(
            "the host settings",
            format!("committed to the repository as {short}, but the host did not answer: {e}"),
            "the commit is in the repository; `homelab host apply` once the host answers",
        )),
    }
}

/// Owner decision 2026-09-30 (item 2): a key just written whose
/// `Apply::Restart` means the host only reads it at its next start;
/// queues `restart-host` so "Save and restart the host" does both in one
/// press. `null` when nothing written needs it.
fn restart_follow_up(c: &EditCtx, keys: &[String], origin: Origin) -> serde_json::Value {
    use homelab_core::hostconfig::{Apply, key_info};
    let needs_restart = keys
        .iter()
        .any(|k| key_info(k).is_some_and(|i| i.apply == Apply::Restart));
    if !needs_restart {
        return serde_json::Value::Null;
    }
    // bundle-hosttoml-deploys (Kenny, 2026-10-24): once the host is back
    // with its new settings, every stack whose files changed is deployed in
    // the same job, so a host.toml change never waits on a second press.
    let req = match actions::validate(
        actions::HOST_TARGET,
        ActionKind::RestartHost.slug(),
        ActionArgs {
            then_apply: true,
            ..ActionArgs::default()
        },
    ) {
        Ok(r) => r,
        Err(r) => return serde_json::json!({ "refused": r }),
    };
    super::follow::queue(c, req, origin)
}
