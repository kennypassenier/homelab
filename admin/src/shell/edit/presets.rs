//! The presets' reads: the new-stack wizard's first read (the repository's
//! presets and what is taken) and the presets editor's first read for one
//! preset (feat-preset-1, D).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::EditCtx;
use super::http::{answer, blocking};
use super::taken::taken;
use crate::core::actions::Refusal;
use crate::core::newstack;
use crate::core::presetedit;
use crate::core::stackedit_files;

pub(super) async fn presets(State(c): State<EditCtx>) -> Response {
    Json(read_presets(&c).await).into_response()
}

/// The new-stack wizard's first read, for the route and a driven `open`.
pub async fn read_presets(c: &EditCtx) -> serde_json::Value {
    let wc = c.wc.clone();
    // Only the repository's presets: without a working copy the scaffold's
    // built-in fallbacks would be offered as if they were Kenny's.
    let (list, sync_error) = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        let list = if wc.present() {
            homelab_client::scaffold::scan_presets(&wc.repo.join("presets"))
                .into_iter()
                .filter(|p| p.dir.is_some())
                .collect()
        } else {
            Vec::new()
        };
        (list, sync_error)
    })
    .await
    .unwrap_or_default();
    let t = taken(c).await;
    let d = homelab_client::scaffold::StackDefaults::default();
    serde_json::json!({
        "presets": list.iter().map(|p| preset_card(p, &d)).collect::<Vec<_>>(),
        "suggest_vmid": newstack::suggest_vmid(&t),
        "taken": { "names": t.names, "vmids": t.vmids },
        "swap": { "divisor": d.swap_divisor, "min_mb": d.swap_min_mb, "max_mb": d.swap_max_mb },
        "working_copy": c.wc.present(),
        "sync_error": sync_error,
    })
}

fn preset_card(
    p: &homelab_client::scaffold::LoadedPreset,
    d: &homelab_client::scaffold::StackDefaults,
) -> serde_json::Value {
    serde_json::json!({
        "name": p.name,
        "description": p.meta.description,
        "ram_mb": p.meta.ram_mb,
        "cores": p.meta.cores.unwrap_or(d.default_cores),
        "disk_gb": p.meta.disk_gb.unwrap_or(d.default_disk_gb),
        // redesign-presets-1: whether the preset itself sets them, so
        // the gallery can say "default" for the fleet's fallback.
        "cores_set": p.meta.cores.is_some(),
        "disk_set": p.meta.disk_gb.is_some(),
        "apps": p.apps,
        "gpu": p.meta.gpu,
        "vpn": p.meta.vpn,
    })
}

pub(super) async fn preset_edit(
    State(c): State<EditCtx>,
    UrlPath(name): UrlPath<String>,
) -> Response {
    answer(read_preset_edit(&c, &name).await)
}

/// The presets editor's first read for one preset: its `preset.yml`
/// (parsed) and every other file it holds. `exists: false` with empty
/// files is a name the working copy does not have yet — the editor's "new
/// preset" case, not an error.
pub async fn read_preset_edit(
    c: &EditCtx,
    name: &str,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let name = presetedit::valid_name(name).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
    let wc = c.wc.clone();
    let (texts, head, sync_error) = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        (wc.preset_texts(), wc.status().head, sync_error)
    })
    .await
    .unwrap_or_default();
    let prefix = format!("{name}/");
    let files: BTreeMap<String, String> = texts
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
        .collect();
    let meta = files
        .get(presetedit::PRESET_META)
        .and_then(|t| serde_yaml::from_str::<homelab_client::scaffold::PresetMeta>(t).ok());
    Ok(serde_json::json!({
        "name": name,
        "head": head,
        "sync_error": sync_error,
        "exists": !files.is_empty(),
        "meta": meta,
        "files": files,
        // Area A's file templates fit here too: a preset's app files are
        // the same shapes a stack's app files are.
        "file_templates": stackedit_files::templates(),
    }))
}
