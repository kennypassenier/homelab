//! The stack editor's first read: the stack's files, its parsed settings and
//! firewall, its images, and the presets an app can come from.

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::EditCtx;
use super::http::{answer, blocking, check_stack_name};
use crate::core::actions::{Refusal, SELF_STACK};
use crate::core::stackedit::{self, MANIFEST, StackTexts};
use crate::core::stackedit_checks;
use crate::core::stackedit_files;
use crate::core::stackedit_latch;
use crate::core::stackedit_native;

pub(super) async fn stack_edit(
    State(c): State<EditCtx>,
    UrlPath(stack): UrlPath<String>,
) -> Response {
    answer(read_stack_edit(&c, &stack).await)
}

/// The editor's first read, for the route and a driven `open`.
pub async fn read_stack_edit(
    c: &EditCtx,
    stack: &str,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    check_stack_name(stack, "the editor")?;
    let wc = c.wc.clone();
    let s2 = stack.to_string();
    let read = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        let texts = wc.stack_texts(&s2)?;
        let presets = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"));
        let head = wc.status().head;
        Ok::<_, Refusal>((texts, presets, head, sync_error))
    })
    .await
    .and_then(|r| r);
    let (texts, presets, head, sync_error) = read.map_err(|r| (StatusCode::CONFLICT, r))?;
    let (manifest_json, manifest_error, native_details) = manifest_view(stack, &texts);
    Ok(serde_json::json!({
        "stack": stack,
        "head": head,
        "sync_error": sync_error,
        "texts": texts,
        "manifest": manifest_json,
        "manifest_error": manifest_error,
        "images": stackedit::images(&texts),
        "file_templates": stackedit_files::templates(),
        "self_stack": SELF_STACK,
        "natives": native_details,
        "presets": presets.iter().filter(|p| p.dir.is_some() && !p.apps.is_empty()).map(|p| serde_json::json!({
            "name": p.name, "description": p.meta.description, "apps": p.apps,
        })).collect::<Vec<_>>(),
    }))
}

/// The parsed manifest as the forms read it, its parse error, and each
/// native unit's own `service.yml` (feat-native-1), parsed so the form can
/// prefill without a YAML parser in the browser.
fn manifest_view(
    stack: &str,
    texts: &StackTexts,
) -> (
    serde_json::Value,
    Option<String>,
    Vec<stackedit_native::NativeView>,
) {
    match texts.get(MANIFEST).map(|t| stackedit::parse_manifest(t)) {
        Some(Ok(m)) => (
            manifest_json(texts, &m),
            None,
            stackedit_native::native_views(texts, &m.natives),
        ),
        Some(Err(e)) => (serde_json::Value::Null, Some(e), Vec::new()),
        None => (
            serde_json::Value::Null,
            Some(format!("stacks/{stack} has no {MANIFEST}")),
            Vec::new(),
        ),
    }
}

fn manifest_json(texts: &StackTexts, m: &homelab_proto::StackManifest) -> serde_json::Value {
    serde_json::json!({
        "vmid": m.vmid,
        "hostname": m.hostname,
        "ip": m.network.ip,
        "resources": m.resources,
        "boot": m.boot,
        "protection": m.lxc.protection,
        "apps": m.apps,
        "natives": m.natives,
        "native_only": m.native_only,
        "firewall": m.firewall,
        "tiles": m.tiles,
        // feat-stacks-9/10/11 (Area C): the settings-extension,
        // apps/storage and latch forms' own read side.
        "network": m.network,
        "lxc": m.lxc,
        "on_demand": m.on_demand,
        "storage": m.storage,
        "data_mounts": m.data_mounts,
        "log_files": m.log_files,
        "retention": m.retention,
        "latch": texts.get(MANIFEST).map(|t| stackedit_latch::current(t)).unwrap_or_default(),
        // feat-checks-1/feat-tiles-1 (Area B): the checks and
        // tiles forms' own read side. Tiles already reads from
        // `m.tiles` above; each app's checks.yml is per-app, not
        // part of the manifest, so it needs its own read.
        "checks": stackedit_checks::checks_by_app(texts, &m.apps),
    })
}
