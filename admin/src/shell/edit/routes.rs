//! The edit surface's routes, mounted with `dashboard_routes`.

use axum::Router;
use axum::routing::{get, post};

use super::EditCtx;
use super::fleet_views::{dependencies, firewall, topology};
use super::host_settings::{host_settings, host_settings_save};
use super::import::{import_commit, import_plan};
use super::new_appdata::new_appdata;
use super::new_stack::{new_commit, new_plan};
use super::preset_edit::{presets_commit, presets_plan};
use super::presets::{preset_edit, presets};
use super::repo::{check_text, repo_status, repo_sync, repo_unpushed};
use super::stack::{stack_commit, stack_plan};
use super::stack_read::stack_edit;
use super::tokens::{tokens_issue, tokens_list, tokens_revoke};

/// Mounted with `dashboard_routes`: the login and both locks stand before
/// every one of them.
pub fn router(ctx: EditCtx) -> Router {
    Router::new()
        .route("/data/repo", get(repo_status))
        .route("/data/repo/sync", post(repo_sync))
        .route("/data/repo/unpushed", post(repo_unpushed))
        .route("/data/check-text", post(check_text))
        .route("/data/stacks/{stack}/edit", get(stack_edit))
        .route("/data/stacks/{stack}/plan", post(stack_plan))
        .route("/data/stacks/{stack}/commit", post(stack_commit))
        .route("/data/presets", get(presets))
        .route("/data/presets/{name}/edit", get(preset_edit))
        .route("/data/presets/plan", post(presets_plan))
        .route("/data/presets/commit", post(presets_commit))
        .route("/data/stacks-new/appdata", post(new_appdata))
        .route("/data/stacks-new/plan", post(new_plan))
        .route("/data/stacks-new/commit", post(new_commit))
        .route("/data/stacks-import/plan", post(import_plan))
        .route("/data/stacks-import/commit", post(import_commit))
        .route("/data/firewall", get(firewall))
        .route("/data/topology", get(topology))
        .route("/data/dependencies", get(dependencies))
        .route(
            "/data/host-settings",
            get(host_settings).put(host_settings_save),
        )
        .route("/data/tokens", get(tokens_list).post(tokens_issue))
        .route("/data/tokens/{name}", axum::routing::delete(tokens_revoke))
        .with_state(ctx)
}
