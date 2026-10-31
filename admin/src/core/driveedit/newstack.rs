//! Edit forms: the new-stack wizard, the commit step, host settings and batch fields.

use super::*;

/// `changesSomething`.
pub fn changes_something(body: &Value) -> bool {
    body.as_object()
        .is_some_and(|m| m.keys().any(|k| k != "kind"))
}

/// `commitFields`: the plan dialog's commit step.
pub fn commit_fields(follow_ups: &[String], subject: &str) -> Vec<Field> {
    let e = es();
    let mut offered = vec!["none".to_string()];
    offered.extend(
        follow_ups
            .iter()
            .filter(|f| e.follow.contains_key(*f))
            .cloned(),
    );
    let mut out: Vec<Field> = e.commit.iter().map(EditFieldDef::field).collect();
    for f in &mut out {
        match f.name.as_str() {
            "subject" => f.current = Some(json!(subject)),
            "follow" => {
                f.choices = Some(
                    offered
                        .iter()
                        .map(|o| Choice {
                            value: o.clone(),
                            label: e.follow[o].clone(),
                        })
                        .collect(),
                );
                let deploy = follow_ups.iter().any(|f| f == "deploy");
                f.current = Some(json!(if deploy { "deploy" } else { "none" }));
            }
            _ => {}
        }
    }
    out
}

/// `commitBody`.
pub fn commit_body(edit: &Value, v: &Values) -> Value {
    let subject = text_of(v.get("subject")).trim().to_string();
    let note = text_of(v.get("note")).trim().to_string();
    let follow = match v.get("follow") {
        Some(Value::String(s)) => s.clone(),
        _ => "none".into(),
    };
    let mut out = Map::new();
    out.insert("edit".into(), edit.clone());
    if !subject.is_empty() {
        out.insert("subject".into(), json!(subject));
    }
    if !note.is_empty() {
        out.insert("note".into(), json!(note));
    }
    if !follow.is_empty() && follow != "none" {
        out.insert("follow".into(), json!(follow));
    }
    Value::Object(out)
}

/// What the wizard's fields start at: the first preset's own sizes.
fn starting_values(presets: &[Value], suggest: Option<u64>) -> BTreeMap<&'static str, String> {
    let d = &es().new_defaults;
    let first = presets.first();
    let pick =
        |k: &str, d: u64| -> String { first.and_then(|p| p[k].as_u64()).unwrap_or(d).to_string() };
    [
        (
            "preset",
            first
                .and_then(|p| p["name"].as_str())
                .unwrap_or("")
                .to_string(),
        ),
        ("vmid", suggest.map(|v| v.to_string()).unwrap_or_default()),
        ("ram_mb", pick("ram_mb", d.ram_mb)),
        ("cores", pick("cores", d.cores)),
        ("disk_gb", pick("disk_gb", d.disk_gb)),
        ("swap_mb", String::new()),
    ]
    .into_iter()
    .collect()
}

fn preset_choices(presets: &[Value]) -> Vec<Choice> {
    presets
        .iter()
        .map(|p| {
            let n = p["name"].as_str().unwrap_or("").to_string();
            Choice {
                label: format!("{n} · {}", p["description"].as_str().unwrap_or("")),
                value: n,
            }
        })
        .collect()
}

/// `newStackWizard`'s steps, for the presets the repository holds.
pub fn new_stack_steps(presets: &[Value], suggest: Option<u64>) -> Vec<FormStep> {
    let now = starting_values(presets, suggest);
    let field_of = |d: &EditFieldDef| {
        let mut f = d.field();
        if d.name == "preset" {
            f.choices = Some(preset_choices(presets));
        }
        if let Some(v) = now.get(d.name.as_str()) {
            f.current = Some(json!(v));
        }
        f
    };
    es().new_stack
        .iter()
        .map(|s| step_of(&s.id, &s.label, s.fields.iter().map(field_of).collect()))
        .collect()
}

/// `dataFields`: one check per `/appdata` folder.
pub fn data_fields(paths: &[String]) -> Vec<Field> {
    paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let i = i.to_string();
            let mut f = es().nodata.filled(&[("path", p), ("i", &i)]);
            f.current = Some(json!(false));
            f
        })
        .collect()
}

fn trimmed(v: &Values, k: &str) -> String {
    text_of(v.get(k)).trim().to_string()
}

/// feat-tiles-3: the wizard's Tile step, folded into this SAME body
/// (`newStackTile` on the browser's side) so `prepare_new` applies it to
/// the staged manifest in the one commit, not a second one after.
fn wizard_tile(v: &Values) -> Option<Value> {
    let t = |k: &str| trimmed(v, k);
    let hostname = t("tile_hostname");
    if hostname.is_empty() {
        return None;
    }
    let mut tile = Map::new();
    tile.insert("hostname".into(), json!(hostname));
    let name = t("tile_name");
    tile.insert(
        "name".into(),
        json!(if name.is_empty() { t("name") } else { name }),
    );
    let group = t("tile_group");
    tile.insert(
        "group".into(),
        json!(if group.is_empty() {
            "Own".into()
        } else {
            group
        }),
    );
    let description = t("tile_description");
    if !description.is_empty() {
        tile.insert("description".into(), json!(description));
    }
    for (field, key) in [
        ("tile_watch_every", "watch_every"),
        ("tile_down_after", "down_after"),
    ] {
        if !t(field).is_empty() {
            tile.insert(key.into(), js_number(&t(field)));
        }
    }
    Some(Value::Object(tile))
}

/// `newStackBody`.
pub fn new_stack_body(v: &Values) -> Value {
    let t = |k: &str| trimmed(v, k);
    let n = |k: &str| js_number(&t(k));
    let mut out = Map::new();
    out.insert("name".into(), json!(t("name")));
    out.insert("vmid".into(), n("vmid"));
    out.insert("preset".into(), json!(text_of(v.get("preset"))));
    out.insert("ram_mb".into(), n("ram_mb"));
    out.insert("cores".into(), n("cores"));
    out.insert("disk_gb".into(), n("disk_gb"));
    if !t("swap_mb").is_empty() {
        out.insert("swap_mb".into(), n("swap_mb"));
    }
    let no_data: Vec<Value> = v
        .iter()
        .filter(|(k, x)| k.starts_with("nodata:") && **x == Value::Bool(true))
        .map(|(k, _)| json!(&k["nodata:".len()..]))
        .collect();
    out.insert("no_data".into(), Value::Array(no_data));
    if let Some(tile) = wizard_tile(v) {
        out.insert("tile".into(), tile);
    }
    Value::Object(out)
}

/// `hostSettingsBody`.
pub fn host_settings_body(
    sha256: &str,
    fields: &[Value],
    staged: &BTreeMap<String, Value>,
    confirmed: &BTreeSet<String>,
) -> Value {
    let mut values = Map::new();
    let mut fragments = Map::new();
    for (key, v) in staged {
        let table = fields
            .iter()
            .any(|f| f["key"] == key.as_str() && f["kind"]["type"] == "table");
        if table {
            fragments.insert(key.clone(), json!(text_of(Some(v))));
        } else {
            values.insert(key.clone(), v.clone());
        }
    }
    let confirms: Vec<&String> = confirmed
        .iter()
        .filter(|k| staged.contains_key(*k))
        .collect();
    json!({ "expect_sha256": sha256, "values": values, "fragments": fragments, "confirms": confirms })
}

/// The batch form (`batchForm`): the action's own fields but an app and
/// the typed name, plus one typed name per stack when the action asks it.
pub fn batch_fields(kind: ActionKind, stacks: &[String]) -> Vec<Field> {
    let mut out: Vec<Field> = kind
        .args()
        .iter()
        .chain(kind.batch_args())
        .filter(|a| !matches!(a, Arg::App | Arg::Confirm))
        .map(|a| drive::arg_field(*a, kind, "each stack"))
        .collect();
    if kind.confirm() {
        for s in stacks {
            let mut f = drive::arg_field(Arg::Confirm, kind, s);
            f.id = format!("act-confirm-{s}");
            f.name = format!("confirm:{s}");
            out.push(f);
        }
    }
    out
}
