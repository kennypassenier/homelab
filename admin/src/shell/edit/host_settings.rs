//! feat-settings-1: every host.toml key, read with a command answered to
//! this session only (the TUI's settings screen keeps its unsaved edits);
//! arch-self keys are read-only. The write is `host_save`.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::Response;
use homelab_proto::Command;

use super::EditCtx;
use super::host_save::save_host_settings;
use super::http::{answer, bad_gateway, body, refusal};
use crate::core::actions::Refusal;
use crate::core::hostsettings;
use crate::shell::actions::Origin;

/// `3.62.2` or `v3.62.2-4-gabc` as numbers.
pub use crate::core::hostversion::version_triple;

/// The host release that answers `GetHostConfig` (feat-settings-1). An
/// older host drops a request it cannot parse without a word, so the page
/// asks only a host at least this new: the next release, 3.63.0.
pub const HOST_SETTINGS_SINCE: (u64, u64, u64) = crate::core::hostversion::NEXT_RELEASE;

/// What a person can do while the host is older than
/// [`HOST_SETTINGS_SINCE`]. Not `homelab host apply`: it reads the host's
/// file with `GetHostConfig` first, which such a host does not answer
/// (fix-guards review: the advice sent people to a command that cannot
/// work there). The TUI's own settings screen is gone (tui-host-settings),
/// so the host's file on its own disk is what remains.
pub const OLD_HOST_SETTINGS_ADVICE: &str = "update the host with `homelab release-update`; \
     until then host.toml changes only on the host itself (/etc/homelab/host.toml over ssh), \
     and takes effect with `systemctl restart homelab-host`";

pub(super) async fn host_new_enough(c: &EditCtx) -> Result<(), Refusal> {
    let v = c.shared.read().await.host_version.clone();
    match v.as_deref().and_then(version_triple) {
        Some(t) if t >= HOST_SETTINGS_SINCE => Ok(()),
        Some(_) => Err(Refusal::new(
            "the host settings",
            format!(
                "the host runs {} and answers host.toml only from {}.{}.{} on",
                v.unwrap_or_default(),
                HOST_SETTINGS_SINCE.0,
                HOST_SETTINGS_SINCE.1,
                HOST_SETTINGS_SINCE.2
            ),
            OLD_HOST_SETTINGS_ADVICE,
        )),
        None => Err(Refusal::new(
            "the host settings",
            "the dashboard has not heard the host yet",
            "wait for the link to the host; the top bar shows it",
        )),
    }
}

pub(super) async fn host_settings(State(c): State<EditCtx>) -> Response {
    answer(read_host_settings(&c).await)
}

/// feat-settings-1: host.toml as the page shows it, for the route and a
/// driven `open host-settings`.
pub async fn read_host_settings(c: &EditCtx) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    host_new_enough(c)
        .await
        .map_err(|r| (StatusCode::SERVICE_UNAVAILABLE, r))?;
    let r = c
        .host
        .ask_traced(Command::GetHostConfig, Duration::from_secs(20), None)
        .await;
    match r {
        Ok(r) if r.ok => host_page(&r.message),
        Ok(r) => Err(bad_gateway(
            "the host settings",
            r.message,
            "look at host.toml on pve",
        )),
        Err(e) => Err(bad_gateway(
            "the host settings",
            e,
            "check that the host answers (homelab ping)",
        )),
    }
}

/// The page from the host's `GetHostConfig` answer.
fn host_page(message: &str) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    match serde_json::from_str::<homelab_proto::HostConfigFile>(message) {
        Ok(file) => Ok(
            serde_json::json!({ "page": hostsettings::page(&file), "measured_at": crate::shell::host_link::now_s() }),
        ),
        Err(e) => Err(bad_gateway(
            "the host settings",
            format!("the host's answer did not read: {e}"),
            "update the host and the dashboard to the same release",
        )),
    }
}

pub(super) async fn host_settings_save(
    State(c): State<EditCtx>,
    b: Result<Json<hostsettings::Change>, JsonRejection>,
) -> Response {
    let change = match body(b, "the host settings") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(save_host_settings(&c, change, Origin::Manual).await)
}
