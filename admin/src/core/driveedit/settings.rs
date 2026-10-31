//! Edit forms: the stack settings form: fields, tiles and body.

use super::*;

/// `settingsForm`'s fields for one stack's manifest (the edit read's
/// `manifest`) and its images.
pub fn settings_fields(m: &Value, images: &Value) -> Vec<Field> {
    let num = |v: &Value| match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let r = &m["resources"];
    let mut out: Vec<Field> = es()
        .settings
        .iter()
        .map(|d| {
            let mut f = d.field();
            f.current = Some(match d.name.as_str() {
                "onboot" => json!(m["boot"]["onboot"].as_bool().unwrap_or(false)),
                "protection" => json!(m["protection"].as_bool().unwrap_or(false)),
                "order" => json!(num(&m["boot"]["order"])),
                k => json!(num(&r[k])),
            });
            f
        })
        .collect();
    if let Some(map) = images.as_object() {
        for (key, image) in map {
            out.push(image_field(key, image.as_str().unwrap_or("")));
        }
    }
    if let Some(map) = m["tiles"].as_object() {
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for key in keys {
            out.extend(tile_fields(key, &map[key]));
        }
    }
    out
}

/// `imageField`.
pub fn image_field(key: &str, image: &str) -> Field {
    let slug = regex::Regex::new("[^a-zA-Z0-9]+")
        .expect("reads")
        .replace_all(key, "-")
        .to_string();
    let mut f = es().image.filled(&[("key", key), ("slug", &slug)]);
    f.current = Some(json!(image));
    f
}

/// `tileFields`: one tile's two watch fields (owner remark 2026-09-30).
pub fn tile_fields(key: &str, tile: &Value) -> Vec<Field> {
    let slug = regex::Regex::new("[^a-zA-Z0-9]+")
        .expect("reads")
        .replace_all(key, "-")
        .to_string();
    let words = [("key", key), ("slug", &slug)];
    let mut watch = es().tile_watch.filled(&words);
    watch.current = Some(json!(num_text(&tile["watch_every"])));
    let mut down = es().tile_down.filled(&words);
    down.current = Some(json!(num_text(&tile["down_after"])));
    vec![watch, down]
}

pub(super) fn num_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `settingsBody`: only what differs from now.
pub fn settings_body(fields: &[Field], values: &Values) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("settings"));
    let mut images = Map::new();
    let mut tiles = Map::new();
    for f in fields {
        let v = values.get(&f.name);
        if f.kind == FieldKind::Check {
            if v != f.current.as_ref() {
                out.insert(f.name.clone(), json!(v == Some(&Value::Bool(true))));
            }
            continue;
        }
        let Some(Value::String(t)) = v else { continue };
        let t = t.trim();
        let now = text_of(f.current.as_ref());
        if t == now {
            continue;
        }
        if let Some(key) = f.name.strip_prefix("image:") {
            images.insert(key.to_string(), json!(t));
        } else if let Some(key) = f.name.strip_prefix("tile_watch_every:") {
            if !t.is_empty() {
                tile_entry(&mut tiles, key).insert("watch_every".into(), js_number(t));
            }
        } else if let Some(key) = f.name.strip_prefix("tile_down_after:") {
            if !t.is_empty() {
                tile_entry(&mut tiles, key).insert("down_after".into(), js_number(t));
            }
        } else if !t.is_empty() {
            out.insert(f.name.clone(), js_number(t));
        }
    }
    if !images.is_empty() {
        out.insert("images".into(), Value::Object(images));
    }
    if !tiles.is_empty() {
        out.insert("tiles".into(), Value::Object(tiles));
    }
    Value::Object(out)
}

/// The tile's map within `tiles`, made fresh the first time it is touched.
pub(super) fn tile_entry<'a>(
    tiles: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    tiles
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("just inserted as an object")
}

/// `tileProblems`: down after must be at least check every, when both are
/// typed.
pub fn tile_problems(values: &Values) -> BTreeMap<String, String> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for name in values.keys() {
        if let Some(k) = name.strip_prefix("tile_watch_every:") {
            keys.insert(k.to_string());
        } else if let Some(k) = name.strip_prefix("tile_down_after:") {
            keys.insert(k.to_string());
        }
    }
    let mut out = BTreeMap::new();
    for key in keys {
        let w = text_of(values.get(&format!("tile_watch_every:{key}")));
        let d = text_of(values.get(&format!("tile_down_after:{key}")));
        let (w, d) = (w.trim(), d.trim());
        if w.is_empty() || d.is_empty() {
            continue;
        }
        if let (Ok(wn), Ok(dn)) = (w.parse::<i64>(), d.parse::<i64>())
            && dn < wn
        {
            out.insert(
                format!("tile_down_after:{key}"),
                say(
                    "tile_down_low",
                    &[
                        ("key", &key),
                        ("watch", &wn.to_string()),
                        ("down", &dn.to_string()),
                    ],
                ),
            );
        }
    }
    out
}

/// `Number(text)` as JSON.
pub(super) fn js_number(t: &str) -> Value {
    let t = t.trim();
    if t.is_empty() {
        return json!(0);
    }
    if let Ok(n) = t.parse::<u64>() {
        return json!(n);
    }
    t.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| json!(n))
        .unwrap_or(Value::Null)
}
