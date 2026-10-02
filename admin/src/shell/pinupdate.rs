//! fix-231 (Kenny, 2026-10-02, Dutch: "ik zie die tabel wel, maar er zijn
//! geen actions aan verbonden?"): the Fleet view's stale images get an
//! Update. Moving a pinned app is an edit of its `image:` line followed by
//! a deploy of its stack (docs/deployment/UPDATE_POLICY.md, "Every manual
//! image names its version and digest"); the page runs that through the
//! routes that already exist — a Backup job first, then the stack editor's
//! own commit (`StackEdit::Settings { images }`) with its deploy of that
//! exact commit.
//!
//! The one thing the page cannot work out alone is the new reference: the
//! upstream's release tag turned into the image's own tag, and the digest
//! the image's registry gives for it right now (the repository pins every
//! manual image by digest, fix-82). This route answers that, read-only:
//! `GET /data/stacks/{stack}/pin-target?key=<app>/<service>&latest=<tag>`.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};

use super::workcopy::WorkingCopy;
use crate::core::actions::{Refusal, valid_stack_name};
use crate::core::stackedit::{StackTexts, images};
use crate::core::stale_images::{registry_of, retarget, target_tag};

/// `(registry, repository, tag)` → the digest the registry gives for it;
/// `Ok(None)` when it answered without one. The real one asks the
/// registry (`homelab_client::pinexists::resolve_digest`); the demo host
/// makes one up.
pub type ResolveFn = dyn Fn(&str, &str, &str) -> Result<Option<String>, String> + Send + Sync;
pub type Resolver = Arc<ResolveFn>;

#[derive(Clone)]
pub struct PinCtx {
    pub wc: Arc<WorkingCopy>,
    pub resolve: Resolver,
}

#[derive(Deserialize)]
struct TargetQuery {
    key: String,
    latest: String,
}

/// What the Update dialog shows and then commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    /// `<app>/<service>`, the settings edit's own key.
    pub key: String,
    /// The `image:` line now.
    pub from: String,
    /// The `image:` line after: same name, new tag, its digest.
    pub to: String,
    pub from_version: String,
    pub to_version: String,
    /// The file the line lives in, relative to the repository.
    pub file: String,
}

/// The target for one stack's `key`, moving to the upstream's `latest`.
pub fn plan_target(
    stack: &str,
    texts: &StackTexts,
    key: &str,
    latest: &str,
    resolve: &ResolveFn,
) -> Result<Target, Refusal> {
    let what = format!("the update of {stack}/{key}");
    let latest = latest.trim();
    if latest.is_empty()
        || latest.len() > 128
        || !latest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(Refusal::new(
            what,
            format!("{latest:?} is not a version tag"),
            "update from the Fleet view's stale-image row",
        ));
    }
    let Some(from) = images(texts).get(key).cloned() else {
        return Err(Refusal::new(
            what,
            format!("stacks/{stack} has no service {key} with an image line"),
            "pull the working copy; the row may name a service that was renamed",
        ));
    };
    let Some(from_version) = homelab_core::ops::pins::pinned_version(&from) else {
        return Err(Refusal::new(
            what,
            format!("{from} names no version tag"),
            "pin it to a version first in the stack editor",
        ));
    };
    let tag = target_tag(&from_version, latest);
    if tag == from_version {
        return Err(Refusal::new(
            what,
            format!("the stack file already names {from_version}"),
            "deploy the stack to run it; the fleet check updates the row tonight",
        ));
    }
    let (registry, repository) = registry_of(&from);
    let digest = match resolve(&registry, &repository, &tag) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Err(Refusal::new(
                what,
                format!("{registry} answered for {repository}:{tag} without a digest"),
                "edit the image line by hand in the stack editor, with the digest from the registry",
            ));
        }
        Err(e) => {
            return Err(Refusal::new(
                what,
                format!(
                    "{registry} has no image {repository}:{tag} the dashboard could read ({e})"
                ),
                "the image may be tagged differently from the release; edit the image line by hand in the stack editor",
            ));
        }
    };
    let (app, _) = key.split_once('/').unwrap_or((key, key));
    Ok(Target {
        key: key.to_string(),
        to: retarget(&from, &tag, &digest),
        from,
        from_version,
        to_version: tag,
        file: format!("stacks/{stack}/{app}/docker-compose.yml"),
    })
}

async fn pin_target(
    State(c): State<PinCtx>,
    UrlPath(stack): UrlPath<String>,
    Query(q): Query<TargetQuery>,
) -> Response {
    if !valid_stack_name(&stack) {
        return (
            StatusCode::BAD_REQUEST,
            Json(Refusal::new(
                "the update",
                "not a stack name",
                "use the name the Fleet view shows",
            )),
        )
            .into_response();
    }
    let done = tokio::task::spawn_blocking(move || {
        let texts = c.wc.stack_texts(&stack)?;
        plan_target(&stack, &texts, &q.key, &q.latest, &*c.resolve)
    })
    .await;
    match done {
        Ok(Ok(t)) => Json(t).into_response(),
        Ok(Err(r)) => (StatusCode::CONFLICT, Json(r)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Refusal::new(
                "the update",
                format!("the lookup stopped: {e}"),
                "try again",
            )),
        )
            .into_response(),
    }
}

/// Mounted with `dashboard_routes`: the login and both locks stand before it.
pub fn router(ctx: PinCtx) -> Router {
    Router::new()
        .route("/data/stacks/{stack}/pin-target", get(pin_target))
        .with_state(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> StackTexts {
        [(
            "web/docker-compose.yml".to_string(),
            "services:\n  web:\n    image: example/web:1.4.2@sha256:aa\n".to_string(),
        )]
        .into_iter()
        .collect()
    }

    fn ok(_: &str, _: &str, tag: &str) -> Result<Option<String>, String> {
        Ok(Some(format!("sha256:{tag}")))
    }

    #[test]
    /// covers: fix-231
    fn fix_231_the_target_keeps_the_name_and_pins_the_new_tags_digest() {
        let t = plan_target("demo", &texts(), "web/web", "v2.0.0", &ok).unwrap();
        assert_eq!(t.from, "example/web:1.4.2@sha256:aa");
        assert_eq!(t.to, "example/web:2.0.0@sha256:2.0.0");
        assert_eq!(t.from_version, "1.4.2");
        assert_eq!(t.to_version, "2.0.0");
        assert_eq!(t.file, "stacks/demo/web/docker-compose.yml");
    }

    #[test]
    /// covers: fix-231
    fn fix_231_a_tag_the_registry_lacks_is_refused_with_a_way_out() {
        let missing =
            |_: &str, _: &str, _: &str| -> Result<Option<String>, String> { Err("404".into()) };
        let r = plan_target("demo", &texts(), "web/web", "2.0.0", &missing).unwrap_err();
        assert!(r.why.contains("example/web:2.0.0"), "{}", r.why);
        assert!(r.fix.contains("stack editor"));
    }

    #[test]
    /// covers: fix-231
    fn fix_231_an_unknown_service_or_a_bad_tag_is_refused() {
        assert!(plan_target("demo", &texts(), "web/other", "2.0.0", &ok).is_err());
        assert!(plan_target("demo", &texts(), "web/web", "2.0 ; rm", &ok).is_err());
        // Already there: nothing to move.
        assert!(plan_target("demo", &texts(), "web/web", "v1.4.2", &ok).is_err());
    }
}
