//! Edit forms: the checks and tiles bodies, the tile row dialog and the publish body.

use super::*;

/// `checksBody`: only the parts touched. `model` is `{checks, manual,
/// probes}` as the editor holds them now; `data` is the edit read `open`
/// started from.
pub fn checks_drive_body(values: &Values, model: &Value, data: &Value, app: &str) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("checks"));
    out.insert("app".into(), json!(app));
    let orig = &data["checks"][app];
    let orig_checks = with_origin(orig.get("checks").unwrap_or(&Value::Null));
    let orig_manual = with_origin(&manual_as_objects(
        orig.get("manual").unwrap_or(&Value::Null),
    ));
    let orig_probes = with_origin(orig.get("probes").unwrap_or(&Value::Null));
    if model["checks"] != orig_checks {
        out.insert("checks".into(), model["checks"].clone());
    }
    if model["manual"] != orig_manual {
        out.insert("manual".into(), model["manual"].clone());
    }
    if model["probes"] != orig_probes {
        out.insert("probes".into(), model["probes"].clone());
    }
    let busy = text_of(values.get("busy_check")).trim().to_string();
    out.insert(
        "busy_check".into(),
        if busy.is_empty() {
            Value::Null
        } else {
            json!(busy)
        },
    );
    let url = text_of(values.get("url")).trim().to_string();
    out.insert(
        "url".into(),
        if url.is_empty() {
            Value::Null
        } else {
            json!(url)
        },
    );
    Value::Object(out)
}

/// Whether `checks_drive_body`'s result actually changes anything —
/// `changes_something` alone cannot tell, since `busy_check`/`url` are
/// always present (the plain-scalar convention `firewallBody` uses for
/// `management_open`: PUT the whole wanted value, not a patch).
pub fn checks_changed(body: &Value, data: &Value, app: &str) -> bool {
    if body.get("checks").is_some() || body.get("manual").is_some() || body.get("probes").is_some()
    {
        return true;
    }
    let orig = &data["checks"][app];
    let now_busy = orig.get("busy_check").and_then(|b| b["command"].as_str());
    let now_url = orig.get("url").and_then(Value::as_str);
    body["busy_check"].as_str() != now_busy || body["url"].as_str() != now_url
}

/// `tileRowFields`.
pub fn tile_row_fields(groups: &[String], row: Option<&Value>) -> Vec<Field> {
    es().tile_row
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "order" | "watch_every" | "down_after" => json!(num_text(&r[&d.name])),
                    k => r[k].clone(),
                });
            }
            if d.name == "group" {
                f.choices = Some(
                    groups
                        .iter()
                        .map(|g| Choice {
                            value: g.clone(),
                            label: g.clone(),
                        })
                        .collect(),
                );
            }
            f
        })
        .collect()
}

/// `tileRowFromValues` (no `origin`/`key`: the caller sets those).
pub fn tile_row_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), json!(text_of(v.get("key")).trim()));
    m.insert("name".into(), json!(text_of(v.get("name")).trim()));
    m.insert("group".into(), json!(text_of(v.get("group")).trim()));
    let order = text_of(v.get("order")).trim().to_string();
    if !order.is_empty() {
        m.insert("order".into(), js_number(&order));
    }
    for k in ["description", "url", "watch_url", "reading"] {
        let t = text_of(v.get(k)).trim().to_string();
        if !t.is_empty() {
            m.insert(k.into(), json!(t));
        }
    }
    let we = text_of(v.get("watch_every")).trim().to_string();
    if !we.is_empty() {
        m.insert("watch_every".into(), js_number(&we));
    }
    let da = text_of(v.get("down_after")).trim().to_string();
    if !da.is_empty() {
        m.insert("down_after".into(), js_number(&da));
    }
    Value::Object(m)
}

/// `tileRowProblems`.
pub fn tile_row_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let we = text_of(v.get("watch_every")).trim().to_string();
    let da = text_of(v.get("down_after")).trim().to_string();
    if !we.is_empty()
        && !da.is_empty()
        && let (Ok(w), Ok(d)) = (we.parse::<i64>(), da.parse::<i64>())
        && d < w
    {
        out.insert(
            "down_after".into(),
            "Must be at least check every (seconds).".into(),
        );
    }
    out
}

/// `tileRowSummary`.
pub fn tile_row_summary(row: &Value) -> String {
    format!(
        "{} · {} ({})",
        row["key"].as_str().unwrap_or(""),
        row["name"].as_str().unwrap_or(""),
        row["group"].as_str().unwrap_or("")
    )
}

/// `tilesEditBody`/`tilesBody`: `model["tiles"]`'s rows (`origin` the old
/// hostname or null) into `TilesEdit.tiles` — a sparse change list
/// (`stackedit_tiles::TilesEdit` is not a full-list `Seq`, unlike the
/// lists above: a tile not mentioned is left alone, and deleting an
/// EXISTING one needs an explicit `delete: true` tombstone).
pub fn tiles_drive_body(model: &Value, data: &Value) -> Value {
    let originals = data["manifest"]["tiles"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let rows = model["tiles"].as_array().cloned().unwrap_or_default();
    let present: std::collections::BTreeSet<String> = rows
        .iter()
        .filter_map(|r| r["origin"].as_str().map(str::to_string))
        .collect();
    let mut out = Vec::new();
    for row in &rows {
        let key = row["key"].as_str().unwrap_or("").to_string();
        let mut tile = row.as_object().cloned().unwrap_or_default();
        tile.remove("origin");
        tile.remove("key");
        match row["origin"].as_str() {
            None => out.push(json!({ "origin": null, "key": key, "delete": false, "tile": tile })),
            Some(origin) => {
                let was = originals.get(origin);
                let changed = was.map(|w| w.as_object() != Some(&tile)).unwrap_or(true);
                if changed || key != origin {
                    out.push(
                        json!({ "origin": origin, "key": key, "delete": false, "tile": tile }),
                    );
                }
            }
        }
    }
    for key in originals.keys() {
        if !present.contains(key) {
            out.push(json!({ "origin": key, "key": key, "delete": true, "tile": {} }));
        }
    }
    json!({ "kind": "tiles", "tiles": out })
}

/// `openPublishDialog`'s body.
pub fn publish_body(values: &Values, app: &str) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("publish_app"));
    out.insert("app".into(), json!(app));
    out.insert(
        "hostname".into(),
        json!(text_of(values.get("hostname")).trim()),
    );
    out.insert("port".into(), js_number(&text_of(values.get("port"))));
    out.insert(
        "external".into(),
        json!(values.get("external") == Some(&Value::Bool(true))),
    );
    out.insert(
        "separate_file".into(),
        json!(values.get("separate_file") == Some(&Value::Bool(true))),
    );
    if values.get("create_tile") == Some(&Value::Bool(true)) {
        let name = text_of(values.get("tile_name")).trim().to_string();
        let group = text_of(values.get("tile_group")).trim().to_string();
        out.insert(
            "tile".into(),
            json!({
                "name": if name.is_empty() { app.to_string() } else { name },
                "group": if group.is_empty() { "Own".to_string() } else { group },
            }),
        );
    }
    Value::Object(out)
}
