//! `homelab ui open` of the forms that edit one stack's files: the first
//! step of each (title head and step).

use super::*;

type Head = (&'static str, FormStep);

pub(super) fn open_settings(m: &Value, data: &Value) -> Head {
    let fields = settings_fields(m, &data["images"]);
    ("Settings", step_of("settings", "Settings", fields))
}

fn choices_of(names: &[String]) -> Vec<Choice> {
    names
        .iter()
        .map(|x| Choice {
            value: x.clone(),
            label: x.clone(),
        })
        .collect()
}

/// The raw editor opens on `lxc-compose.yml`, else on the first file.
pub(super) fn open_raw(data: &Value) -> Head {
    let texts = data["texts"].as_object().cloned().unwrap_or_default();
    let files: Vec<String> = texts.keys().cloned().collect();
    let first = if files.iter().any(|f| f == "lxc-compose.yml") {
        "lxc-compose.yml".to_string()
    } else {
        files.first().cloned().unwrap_or_default()
    };
    let mut fields: Vec<Field> = es().raw.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        match f.name.as_str() {
            "file" => {
                f.choices = Some(choices_of(&files));
                f.current = Some(json!(first));
            }
            "text" => f.current = Some(texts.get(&first).cloned().unwrap_or(json!(""))),
            // "op" starts at "edit" (its choices are the fixed list
            // formspec.json gives it); the two path fields (create/rename)
            // start empty.
            "op" => f.current = Some(json!("edit")),
            _ => {}
        }
    }
    ("Edit a file", step_of("file", "File", fields))
}

fn preset_choice(p: &Value) -> Choice {
    let n = p["name"].as_str().unwrap_or("").to_string();
    Choice {
        label: format!(
            "{n} · {} ({})",
            p["description"].as_str().unwrap_or(""),
            strings(&p["apps"]).join(", ")
        ),
        value: n,
    }
}

pub(super) fn open_add_app(req: &Req) -> Result<Head, Refusal> {
    let data = &req.data;
    let presets = data["presets"].as_array().cloned().unwrap_or_default();
    if presets.is_empty() {
        return Err(req.refuse(
            "the repository holds no preset with an app",
            "add one under presets/ first",
        ));
    }
    let mut preset_field = es().add_app[0].field();
    preset_field.choices = Some(presets.iter().map(preset_choice).collect());
    let first_preset = presets[0]["name"].as_str().unwrap_or("").to_string();
    preset_field.current = Some(json!(first_preset));
    // feat-tiles-3: the tile fields of the preset already selected by
    // default, the same as the browser's own `renderTiles` on first render.
    let fields = add_app_step_fields(preset_field, data, &first_preset);
    Ok(("Add an app", step_of("app", "Add an app", fields)))
}

pub(super) fn open_firewall(m: &Value, es_: &mut EditState) -> Head {
    let model = firewall_model(&m["firewall"]);
    let mut fields: Vec<Field> = es().firewall.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        f.current = Some(model[&f.name].clone());
    }
    es_.model = Some(model);
    ("Firewall", step_of("rules", "Rules", fields))
}

pub(super) fn open_settings_ext(m: &Value, es_: &mut EditState) -> Head {
    // W2: retention is a row table like storage/data_mounts (full end-state
    // list, origin tracks the old row), not a plain field: `es_.model`
    // carries it the same way Apps' three lists ride along beside their
    // fields.
    es_.model = Some(json!({ "retention": with_origin(&m["retention"]) }));
    let fields = settings_ext_fields(m);
    (
        "Network & hardware",
        step_of("settings_ext", "Network & hardware", fields),
    )
}

pub(super) fn open_apps(m: &Value, es_: &mut EditState) -> Head {
    let mut fields: Vec<Field> = strings(&m["apps"])
        .iter()
        .map(|a| apps_remove_field(a))
        .collect();
    fields.push(apps_add_blank_field());
    // storage/data_mounts/log_files live on the manifest (`m`), not on the
    // edit-read envelope (`data`) itself.
    es_.model = Some(json!({
        "storage": with_origin(&m["storage"]),
        "data_mounts": with_origin(&m["data_mounts"]),
        "log_files": with_origin(&m["log_files"]),
    }));
    ("Apps & storage", step_of("apps", "Apps & storage", fields))
}

pub(super) fn open_latch(m: &Value, es_: &mut EditState) -> Head {
    // `latch` lives on the manifest (`m`) too, like storage above.
    let secrets = strings(&m["latch"]["latch_secrets"]);
    let mut fields: Vec<Field> = strings(&m["apps"])
        .iter()
        .map(|a| {
            let mut f = latch_secret_field(a);
            f.current = Some(json!(secrets.contains(a)));
            f
        })
        .collect();
    // gap 74: two or more native units have a restart order.
    let natives = strings(&m["natives"]);
    if natives.len() > 1 {
        fields.extend(
            natives
                .iter()
                .enumerate()
                .map(|(i, u)| latch_order_field(u, i + 1, natives.len())),
        );
    }
    es_.model = Some(json!({ "latch_files": with_origin(&m["latch"]["latch_files"]) }));
    ("Latch", step_of("latch", "Latch", fields))
}

pub(super) fn open_add_native() -> Head {
    let fields: Vec<Field> = es().add_native.iter().map(EditFieldDef::field).collect();
    (
        "Add a native unit",
        step_of("add_native", "Add a native unit", fields),
    )
}

pub(super) fn open_tiles(m: &Value, es_: &mut EditState) -> Head {
    let tiles = m["tiles"].as_object().cloned().unwrap_or_default();
    let mut keys: Vec<&String> = tiles.keys().collect();
    keys.sort();
    let list: Vec<Value> = keys
        .into_iter()
        .map(|key| {
            let mut row = tiles[key].as_object().cloned().unwrap_or_default();
            row.insert("key".into(), json!(key));
            row.insert("origin".into(), json!(key));
            Value::Object(row)
        })
        .collect();
    es_.model = Some(json!({ "tiles": Value::Array(list) }));
    ("Tiles", step_of("tiles", "Tiles", Vec::new()))
}
