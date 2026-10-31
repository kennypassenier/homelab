//! feat-platform-10 (milestone follow), the edit forms: Claude drives the
//! stack settings form and its raw editor, "add an app", the firewall with
//! its rule dialog, the new-stack wizard, the host settings, the batch
//! dialog and the roll back dialog, the way `drive` drives an action's
//! dialog.
//!
//! The fields' words come from `formspec.json`'s `edit` section, the same
//! description `editforms.js` draws the browser's forms from; the checks
//! here are the browser's (`checkFields`, `ruleProblems`, `checkNewStep`,
//! `parseKey`), held equal by the cases file both suites read. The final
//! press (the commit and push, host.toml, the batch) is an `EditCall` the
//! shell runs through the very function a click's route runs. Pure.

mod apps;
mod check_rows;
mod checks;
mod firewall;
mod keys;
mod kind;
mod latch;
mod native;
mod newstack;
mod settings;
mod settings_ext;
mod spec;
mod state;
mod storage;
mod tiles;

pub use self::apps::*;
pub use self::check_rows::*;
pub use self::checks::*;
pub use self::firewall::*;
pub use self::keys::*;
pub use self::kind::*;
pub use self::latch::*;
pub use self::native::*;
pub use self::newstack::*;
pub use self::settings::*;
pub use self::settings_ext::*;
pub use self::spec::*;
pub use self::state::*;
pub use self::storage::*;
pub use self::tiles::*;

use std::collections::{BTreeMap, BTreeSet};

use homelab_proto::{Scope, UiStep};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::core::actions::{ActionKind, Arg, Refusal, SELF_STACK};
use crate::core::drive::{
    self, Applied, Choice, Ctx, DriveState, Effect, Family, Field, FieldKind, FormStep, OpenForm,
    Sources, Values, fill, refused, spec,
};
use crate::core::presetedit;

// ── the description ─────────────────────────────────────────────────────

fn plain() -> Applied {
    Applied {
        effect: Effect::None,
        held: None,
    }
}

fn held_for(form: &OpenForm, what: &str, errors: &BTreeMap<String, String>) -> Refusal {
    let all: Vec<Field> = form
        .desc
        .fields()
        .cloned()
        .chain(
            form.edit
                .as_ref()
                .and_then(|e| e.sub.as_ref())
                .map(|s| s.fields.clone())
                .unwrap_or_default(),
        )
        .collect();
    let list: Vec<String> = errors
        .iter()
        .map(|(name, e)| {
            let id = all
                .iter()
                .find(|f| &f.name == name)
                .map_or(name.as_str(), |f| f.id.as_str());
            format!("{id}: {e}")
        })
        .collect();
    Refusal::new(
        format!("ui press {what}"),
        format!("the form holds: {}", list.join("; ")),
        "correct the named fields and press again",
    )
}

fn held_msg(what: &str, why: &str) -> Applied {
    Applied {
        effect: Effect::None,
        held: Some(Refusal::new(
            format!("ui press {what}"),
            format!("the form holds: {why}"),
            "change something first, or homelab ui close",
        )),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn edit(form: &mut OpenForm) -> &mut EditState {
    form.edit.as_mut().expect("an edit form has its edit state")
}

/// `homelab ui open` of an edit form.
pub fn open(
    st: &mut DriveState,
    step: &UiStep,
    kind: EditKind,
    form: &str,
    target: Option<&str>,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    if let Some(f) = &st.form {
        return Err(refused(
            step,
            format!("the dialog {} is open already", f.title),
            "homelab ui close first",
        ));
    }
    if kind.scope() > cx.scope {
        return Err(refused(
            step,
            format!(
                "{} needs scope {:?}; the token \"{}\" has {:?}",
                kind.slug(),
                kind.scope(),
                cx.by,
                cx.scope
            ),
            "drive it with a token of that scope, or leave it to Kenny",
        ));
    }
    let stack_named = |t: Option<&str>| -> Result<String, Refusal> {
        let Some(t) = t else {
            return Err(refused(
                step,
                format!("{} acts on one stack and none was named", kind.slug()),
                format!("homelab ui open {}", kind.usage()),
            ));
        };
        if !cx.stacks.iter().any(|s| s == t) {
            return Err(refused(
                step,
                format!("the fleet has no stack {t}"),
                format!("the stacks are: {}", cx.stacks.join(", ")),
            ));
        }
        Ok(t.to_string())
    };
    let no_target = |t: Option<&str>| -> Result<(), Refusal> {
        match t {
            Some(t) => Err(refused(
                step,
                format!("{} takes no stack, not {t}", kind.slug()),
                format!("homelab ui open {}", kind.usage()),
            )),
            None => Ok(()),
        }
    };
    let data = cx.sources.edit.clone();
    let unreadable = |what: &str| -> Result<(), Refusal> {
        if let Some(why) = data["error"].as_object() {
            return Err(Refusal::new(
                format!("ui open {}", kind.slug()),
                format!(
                    "{what} could not be read: {}",
                    why.get("why").and_then(Value::as_str).unwrap_or("unknown")
                ),
                why.get("fix")
                    .and_then(Value::as_str)
                    .unwrap_or("homelab ui state, then try again")
                    .to_string(),
            ));
        }
        Ok(())
    };
    let mut es_ = EditState::new(kind, data.clone());
    let (id, stack, title, steps, page) = match kind {
        EditKind::Settings
        | EditKind::Raw
        | EditKind::AddApp
        | EditKind::Firewall
        | EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Tiles
        | EditKind::AddNative => {
            let stack = stack_named(target)?;
            unreadable(&format!("the files of {stack}"))?;
            let m = &data["manifest"];
            let need_manifest = kind != EditKind::Raw;
            if need_manifest && m.is_null() {
                return Err(refused(
                    step,
                    format!(
                        "the {} form needs a readable lxc-compose.yml: {}",
                        kind.slug(),
                        data["manifest_error"].as_str().unwrap_or("none")
                    ),
                    format!("homelab ui open raw {stack}"),
                ));
            }
            let tab = if kind == EditKind::Firewall {
                "firewall"
            } else {
                "settings"
            };
            let page = format!("/stacks/{stack}/{tab}");
            let plan_steps = |first: FormStep| {
                vec![
                    first,
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ]
            };
            match kind {
                EditKind::Settings => (
                    format!("edit:settings:{stack}"),
                    stack.clone(),
                    format!("Settings · {stack}"),
                    plan_steps(FormStep {
                        id: "settings".into(),
                        label: "Settings".into(),
                        fields: settings_fields(m, &data["images"]),
                    }),
                    page,
                ),
                EditKind::Raw => {
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
                                f.choices = Some(
                                    files
                                        .iter()
                                        .map(|x| Choice {
                                            value: x.clone(),
                                            label: x.clone(),
                                        })
                                        .collect(),
                                );
                                f.current = Some(json!(first));
                            }
                            "text" => {
                                f.current = Some(texts.get(&first).cloned().unwrap_or(json!("")));
                            }
                            // "op" starts at "edit" (its choices are the
                            // fixed list formspec.json gives it); the two
                            // path fields (create/rename) start empty.
                            "op" => f.current = Some(json!("edit")),
                            _ => {}
                        }
                    }
                    (
                        format!("edit:raw:{stack}"),
                        stack.clone(),
                        format!("Edit a file · {stack}"),
                        plan_steps(FormStep {
                            id: "file".into(),
                            label: "File".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::AddApp => {
                    let presets = data["presets"].as_array().cloned().unwrap_or_default();
                    if presets.is_empty() {
                        return Err(refused(
                            step,
                            "the repository holds no preset with an app",
                            "add one under presets/ first",
                        ));
                    }
                    let mut preset_field = es().add_app[0].field();
                    preset_field.choices = Some(
                        presets
                            .iter()
                            .map(|p| {
                                let n = p["name"].as_str().unwrap_or("").to_string();
                                Choice {
                                    label: format!(
                                        "{n} · {} ({})",
                                        p["description"].as_str().unwrap_or(""),
                                        strings(&p["apps"]).join(", ")
                                    ),
                                    value: n,
                                }
                            })
                            .collect(),
                    );
                    let first_preset = presets[0]["name"].as_str().unwrap_or("").to_string();
                    preset_field.current = Some(json!(first_preset));
                    // feat-tiles-3: the tile fields of the preset already
                    // selected by default, the same as the browser's own
                    // `renderTiles` on first render.
                    let fields = add_app_step_fields(preset_field, &data, &first_preset);
                    (
                        format!("edit:add-app:{stack}"),
                        stack.clone(),
                        format!("Add an app · {stack}"),
                        plan_steps(FormStep {
                            id: "app".into(),
                            label: "Add an app".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Firewall => {
                    let model = firewall_model(&m["firewall"]);
                    let mut fields: Vec<Field> =
                        es().firewall.iter().map(EditFieldDef::field).collect();
                    for f in &mut fields {
                        f.current = Some(model[&f.name].clone());
                    }
                    es_.model = Some(model);
                    (
                        format!("edit:firewall:{stack}"),
                        stack.clone(),
                        format!("Firewall · {stack}"),
                        plan_steps(FormStep {
                            id: "rules".into(),
                            label: "Rules".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::SettingsExt => {
                    // W2: retention is a row table like storage/data_mounts
                    // (full end-state list, origin tracks the old row), not
                    // a plain field — `es_.model` carries it the same way
                    // Apps' three lists ride along beside their fields.
                    es_.model = Some(json!({ "retention": with_origin(&m["retention"]) }));
                    (
                        format!("edit:settings-ext:{stack}"),
                        stack.clone(),
                        format!("Network & hardware · {stack}"),
                        plan_steps(FormStep {
                            id: "settings_ext".into(),
                            label: "Network & hardware".into(),
                            fields: settings_ext_fields(m),
                        }),
                        page,
                    )
                }
                EditKind::Apps => {
                    let apps = strings(&m["apps"]);
                    let mut fields: Vec<Field> =
                        apps.iter().map(|a| apps_remove_field(a)).collect();
                    fields.push(apps_add_blank_field());
                    // storage/data_mounts/log_files live on the manifest
                    // (`m`), not on the edit-read envelope (`data`) itself
                    // — `data["storage"]` was always null, so the row
                    // table opened empty no matter what the stack held.
                    es_.model = Some(json!({
                        "storage": with_origin(&m["storage"]),
                        "data_mounts": with_origin(&m["data_mounts"]),
                        "log_files": with_origin(&m["log_files"]),
                    }));
                    (
                        format!("edit:apps:{stack}"),
                        stack.clone(),
                        format!("Apps & storage · {stack}"),
                        plan_steps(FormStep {
                            id: "apps".into(),
                            label: "Apps & storage".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Latch => {
                    let apps = strings(&m["apps"]);
                    // `latch` lives on the manifest (`m`) too, the same
                    // fix as storage/data_mounts/log_files above.
                    let secrets = strings(&m["latch"]["latch_secrets"]);
                    let mut fields: Vec<Field> = apps
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
                    es_.model = Some(json!({
                        "latch_files": with_origin(&m["latch"]["latch_files"]),
                    }));
                    (
                        format!("edit:latch:{stack}"),
                        stack.clone(),
                        format!("Latch · {stack}"),
                        plan_steps(FormStep {
                            id: "latch".into(),
                            label: "Latch".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::AddNative => {
                    let fields: Vec<Field> =
                        es().add_native.iter().map(EditFieldDef::field).collect();
                    (
                        format!("edit:add-native:{stack}"),
                        stack.clone(),
                        format!("Add a native unit · {stack}"),
                        plan_steps(FormStep {
                            id: "add_native".into(),
                            label: "Add a native unit".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Tiles => {
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
                    (
                        format!("edit:tiles:{stack}"),
                        stack.clone(),
                        format!("Tiles · {stack}"),
                        plan_steps(FormStep {
                            id: "tiles".into(),
                            label: "Tiles".into(),
                            fields: Vec::new(),
                        }),
                        page,
                    )
                }
                _ => unreachable!("the outer match restricts kind to this bucket's 9 variants"),
            }
        }
        EditKind::Native => {
            // `<stack>[/<unit>]` (`stack_named` reads a plain stack name,
            // so the unit is split off first — `rollback-native`'s own
            // `<stack>[/<unit>]` convention, not a second CLI argument).
            let (stack_part, unit_part) = match target {
                Some(t) => match t.split_once('/') {
                    Some((s, u)) => (s, Some(u)),
                    None => (t, None),
                },
                None => {
                    return Err(refused(
                        step,
                        format!("{} acts on one stack and none was named", kind.slug()),
                        format!("homelab ui open {}", kind.usage()),
                    ));
                }
            };
            let stack = stack_named(Some(stack_part))?;
            unreadable(&format!("the files of {stack}"))?;
            let natives = data["natives"].as_array().cloned().unwrap_or_default();
            if natives.is_empty() {
                return Err(refused(
                    step,
                    format!("{stack} has no native units"),
                    format!("homelab ui open add-native {stack}"),
                ));
            }
            let chosen = match unit_part {
                Some(u) => {
                    if !natives.iter().any(|n| n["unit"].as_str() == Some(u)) {
                        return Err(refused(
                            step,
                            format!("{stack} has no native unit called {u}"),
                            format!(
                                "its native units are: {}",
                                natives
                                    .iter()
                                    .filter_map(|n| n["unit"].as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                    u.to_string()
                }
                None => natives[0]["unit"].as_str().unwrap_or_default().to_string(),
            };
            let m = natives
                .iter()
                .find(|n| n["unit"].as_str() == Some(chosen.as_str()))
                .map(|n| n["manifest"].clone())
                .unwrap_or(Value::Null);
            let mut fields: Vec<Field> = es().native.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                if f.name == "unit" {
                    f.choices = Some(
                        natives
                            .iter()
                            .filter_map(|n| n["unit"].as_str())
                            .map(|u| Choice {
                                value: u.to_string(),
                                label: u.to_string(),
                            })
                            .collect(),
                    );
                    f.current = Some(json!(chosen));
                } else {
                    f.current = Some(native_current(&f.name, &m));
                }
            }
            (
                format!("edit:native:{stack}"),
                stack.clone(),
                format!("Native services · {stack}"),
                vec![
                    FormStep {
                        id: "native".into(),
                        label: "Native services".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                format!("/stacks/{stack}/settings"),
            )
        }
        EditKind::Checks | EditKind::PublishApp => {
            let Some(raw) = target else {
                return Err(refused(
                    step,
                    format!("{} acts on one app and none was named", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            let Some((stack_name, app)) = raw.split_once('/') else {
                return Err(refused(
                    step,
                    format!("{} takes <stack>/<app>, not {raw:?}", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            let stack = stack_named(Some(stack_name))?;
            unreadable(&format!("the files of {stack}"))?;
            let m = &data["manifest"];
            if m.is_null() {
                return Err(refused(
                    step,
                    format!(
                        "the {} form needs a readable lxc-compose.yml: {}",
                        kind.slug(),
                        data["manifest_error"].as_str().unwrap_or("none")
                    ),
                    format!("homelab ui open raw {stack}"),
                ));
            }
            let apps = strings(&m["apps"]);
            if !apps.iter().any(|a| a == app) {
                return Err(refused(
                    step,
                    format!("{app} is not an app of {stack}"),
                    format!("the apps are: {}", apps.join(", ")),
                ));
            }
            let plan_steps = |first: FormStep| {
                vec![
                    first,
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ]
            };
            match kind {
                EditKind::Checks => {
                    // `checks` lives on the manifest (`m`), not the
                    // envelope (`data`) — same fix as Apps/Latch above.
                    let orig = &m["checks"][app];
                    let checks_rows = with_origin(orig.get("checks").unwrap_or(&Value::Null));
                    let manual_rows = with_origin(&manual_as_objects(
                        orig.get("manual").unwrap_or(&Value::Null),
                    ));
                    let probes_rows = with_origin(orig.get("probes").unwrap_or(&Value::Null));
                    let busy = orig
                        .get("busy_check")
                        .and_then(|b| b["command"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = orig
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    es_.model = Some(json!({
                        "app": app,
                        "checks": checks_rows,
                        "manual": manual_rows,
                        "probes": probes_rows,
                    }));
                    let mut fields: Vec<Field> =
                        es().checks_scalar.iter().map(EditFieldDef::field).collect();
                    for f in &mut fields {
                        match f.name.as_str() {
                            "app" => {
                                f.choices = Some(
                                    apps.iter()
                                        .map(|a| Choice {
                                            value: a.clone(),
                                            label: a.clone(),
                                        })
                                        .collect(),
                                );
                                f.current = Some(json!(app));
                            }
                            "busy_check" => f.current = Some(json!(busy)),
                            "url" => f.current = Some(json!(url)),
                            _ => {}
                        }
                    }
                    (
                        format!("edit:checks:{stack}:{app}"),
                        stack.clone(),
                        format!("checks.yml · {app} · {stack}"),
                        plan_steps(FormStep {
                            id: "checks".into(),
                            label: "Checks".into(),
                            fields,
                        }),
                        format!("/stacks/{stack}/checks"),
                    )
                }
                _ => {
                    let fields: Vec<Field> = es().publish.iter().map(EditFieldDef::field).collect();
                    es_.model = Some(json!({ "app": app }));
                    (
                        format!("edit:publish:{stack}:{app}"),
                        stack.clone(),
                        format!("Publish {app} · {stack}"),
                        plan_steps(FormStep {
                            id: "publish".into(),
                            label: "Publish".into(),
                            fields,
                        }),
                        format!("/stacks/{stack}/apps"),
                    )
                }
            }
        }
        EditKind::NewStack => {
            no_target(target)?;
            unreadable("the presets")?;
            let presets = data["presets"].as_array().cloned().unwrap_or_default();
            if presets.is_empty() {
                return Err(refused(
                    step,
                    match data["sync_error"].as_str() {
                        Some(e) => format!("the working copy is not there: {e}"),
                        None => "the repository holds no presets".into(),
                    },
                    "the settings page shows the working copy's state",
                ));
            }
            (
                "new-stack".to_string(),
                String::new(),
                "New stack".to_string(),
                new_stack_steps(&presets, data["suggest_vmid"].as_u64()),
                st.page.clone(),
            )
        }
        EditKind::HostSettings => {
            no_target(target)?;
            unreadable("host.toml")?;
            if data["page"]["fields"].as_array().is_none() {
                return Err(refused(
                    step,
                    "host.toml was not read",
                    "the settings page says why; try again once the host answers",
                ));
            }
            (
                "edit:host-settings".to_string(),
                crate::core::actions::HOST_TARGET.to_string(),
                "Host settings".to_string(),
                vec![
                    FormStep {
                        id: "keys".into(),
                        label: "Keys".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "review".into(),
                        label: "Write host.toml".into(),
                        fields: Vec::new(),
                    },
                ],
                "/settings".to_string(),
            )
        }
        EditKind::Batch => {
            let action = form.strip_prefix("batch:").unwrap_or("");
            let entry = ActionKind::from_slug(action).filter(|k| {
                !k.host_wide()
                    && !k
                        .args()
                        .iter()
                        .any(|a| matches!(a, Arg::Unit | Arg::Commit))
            });
            let Some(ak) = entry else {
                let all: Vec<&str> = ActionKind::ALL
                    .iter()
                    .filter(|k| {
                        !k.host_wide()
                            && !k
                                .args()
                                .iter()
                                .any(|a| matches!(a, Arg::Unit | Arg::Commit))
                    })
                    .map(|k| k.slug())
                    .collect();
                return Err(refused(
                    step,
                    format!("{action:?} is not an action several stacks can be given at once"),
                    format!(
                        "homelab ui open batch <action> <stack>,<stack> with one of: {}",
                        all.join(", ")
                    ),
                ));
            };
            // Owner decision 2026-09-30: no stacks named opens the batch
            // dialog from the Overview table's own ticked selection
            // (`homelab ui select` first), exactly as the page's "Run on
            // the selected…" button does.
            let list: Vec<String> = match target {
                Some(t) => t
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                None => st.selected.clone(),
            };
            if list.is_empty() {
                return Err(refused(
                    step,
                    "a batch needs its stacks: none were named and the fleet table's selection is empty",
                    format!(
                        "homelab ui select <stack>,<stack> then homelab ui open batch {action}, \
                         or homelab ui open batch {action} <stack>,<stack>"
                    ),
                ));
            }
            for s in &list {
                stack_named(Some(s))?;
            }
            if ak.refused_for_self() && list.iter().any(|s| s == SELF_STACK) {
                return Err(refused(
                    step,
                    format!(
                        "The dashboard never does this to its own stack ({SELF_STACK}); leave it out of the selection."
                    ),
                    "name the other stacks only",
                ));
            }
            if ak.scope() > cx.scope {
                return Err(refused(
                    step,
                    format!(
                        "{action} needs scope {:?}; the token \"{}\" has {:?}",
                        ak.scope(),
                        cx.by,
                        cx.scope
                    ),
                    "drive it with a token of that scope, or leave it to Kenny",
                ));
            }
            es_.batch_action = Some(action.to_string());
            es_.stacks = list.clone();
            es_.guarded = data["guarded"].as_u64().unwrap_or(0) as usize;
            let n = list.len();
            (
                format!("batch:{action}"),
                list.join(","),
                format!(
                    "{} · {n} {}",
                    ak.label(),
                    if n == 1 { "stack" } else { "stacks" }
                ),
                vec![FormStep {
                    id: "review".into(),
                    label: "Review and run".into(),
                    fields: batch_fields(ak, &list),
                }],
                // A batch action is opened from the Overview page's own
                // selection (drive.rs's Select step requires it).
                "/overview".to_string(),
            )
        }
        EditKind::Import => {
            no_target(target)?;
            unreadable("the taken names and numbers")?;
            if data["working_copy"] == json!(false) {
                return Err(refused(
                    step,
                    "the dashboard has no working copy to commit the new stack to",
                    "the settings page shows the working copy's state",
                ));
            }
            (
                "import".to_string(),
                String::new(),
                "Import a stack".to_string(),
                vec![
                    FormStep {
                        id: "bundle".into(),
                        label: "Bundle".into(),
                        fields: es().import.iter().map(EditFieldDef::field).collect(),
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                st.page.clone(),
            )
        }
        EditKind::Preset => {
            let Some(name) = target else {
                return Err(refused(
                    step,
                    format!("{} acts on one preset and none was named", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            if presetedit::valid_name(name).is_err() {
                return Err(refused(
                    step,
                    format!("{name:?} is not a preset name"),
                    "lowercase letters, digits and '-', not starting with '_'",
                ));
            }
            unreadable(&format!("the {name} preset"))?;
            if data["exists"] != json!(true) {
                return Err(refused(
                    step,
                    format!("there is no preset called {name} yet"),
                    format!("homelab ui open new-preset, then name it {name}"),
                ));
            }
            let meta = &data["meta"];
            let mut fields: Vec<Field> = es().preset_meta.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                f.current = Some(preset_meta_current(&f.name, meta));
            }
            // feat-preset-1: the Files card's own fields and "Remove this
            // preset…" are only ever on screen for a preset that already
            // exists — the same `if (e.exists)` gate the browser's own
            // dialog uses.
            fields.extend(preset_file_fields(&data));
            (
                format!("edit:preset:{name}"),
                name.to_string(),
                format!("{name} · preset.yml"),
                vec![
                    FormStep {
                        id: "meta".into(),
                        label: "preset.yml".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                "/presets".to_string(),
            )
        }
        EditKind::NewPreset => {
            no_target(target)?;
            let mut fields: Vec<Field> = vec![es().new_preset_name.field()];
            for d in &es().preset_meta {
                let mut f = d.field();
                f.current = Some(preset_meta_current(&f.name, &Value::Null));
                fields.push(f);
            }
            (
                "new-preset".to_string(),
                String::new(),
                "New preset".to_string(),
                vec![
                    FormStep {
                        id: "meta".into(),
                        label: "New preset".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                "/presets".to_string(),
            )
        }
        EditKind::Rollback => {
            let stack = stack_named(target)?;
            unreadable("the roll-back options")?;
            let none = Choice {
                value: String::new(),
                label: "None".into(),
            };
            let mut fields: Vec<Field> = es().rollback.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                let mut c = vec![none.clone()];
                if f.name == "commit" {
                    for x in data["commits"].as_array().into_iter().flatten() {
                        let commit = x["commit"].as_str().unwrap_or("").to_string();
                        let short: String = commit.chars().take(10).collect();
                        c.push(Choice {
                            label: format!("{short} · {}", x["subject"].as_str().unwrap_or("")),
                            value: commit,
                        });
                    }
                } else {
                    for u in strings(&data["native_units"]) {
                        c.push(Choice {
                            value: u.clone(),
                            label: u,
                        });
                    }
                }
                f.choices = Some(c);
                f.current = Some(json!(""));
            }
            (
                format!("rollback:{stack}"),
                stack.clone(),
                format!("Roll back · {stack}"),
                vec![FormStep {
                    id: "choose".into(),
                    label: "Choose".into(),
                    fields,
                }],
                format!("/stacks/{stack}"),
            )
        }
    };
    st.page = page;
    st.form = Some(OpenForm::new_edit(id, stack, title, steps, es_));
    Ok(plain())
}

/// What else changes when one field changes, as the page's own handlers
/// do it: the preset's size, the raw file's text, the firewall's model.
pub fn after_set(form: &mut OpenForm, name: &str, in_sub: bool) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    if in_sub {
        return;
    }
    let value = form.values.get(name).cloned().unwrap_or(Value::Null);
    match (kind, name) {
        (EditKind::NewStack, "preset") => {
            let p = edit(form).data["presets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == value)
                .cloned();
            if let Some(p) = p {
                for k in ["ram_mb", "cores", "disk_gb"] {
                    let v = text_of(Some(&p[k]));
                    form.values.insert(k.into(), json!(v));
                }
            }
        }
        (EditKind::Raw, "file") => {
            let t = edit(form).data["texts"][value.as_str().unwrap_or("")].clone();
            form.values
                .insert("text".into(), if t.is_null() { json!("") } else { t });
        }
        (EditKind::Preset, "file") => {
            // Mirrors `filesCard`'s own `show()`: picking a file fills
            // its path (then locked to it in the real form) and its
            // current text; picking "(new file)" clears both.
            let f = value.as_str().unwrap_or("").to_string();
            form.values.insert("path".into(), json!(f));
            let content = if f.is_empty() {
                String::new()
            } else {
                text_of(edit(form).data["files"].get(f.as_str()))
            };
            form.values.insert("text".into(), json!(content));
        }
        (EditKind::Preset, "template") => {
            // Mirrors `templateSel`'s own change handler: only fills the
            // text when a NEW file is being made (no file picked).
            if text_of(form.values.get("file")).trim().is_empty() {
                let t = value.as_str().unwrap_or("").to_string();
                let content = if t.is_empty() {
                    String::new()
                } else {
                    text_of(edit(form).data["file_templates"].get(t.as_str()))
                };
                form.values.insert("text".into(), json!(content));
            }
        }
        (EditKind::AddApp, "preset") => {
            let preset_name = value.as_str().unwrap_or("").to_string();
            let data = edit(form).data.clone();
            if let Some(step) = form.desc.steps.first_mut() {
                let preset_field = step
                    .fields
                    .iter()
                    .find(|f| f.name == "preset")
                    .cloned()
                    .unwrap_or_else(|| es().add_app[0].field());
                step.fields = add_app_step_fields(preset_field, &data, &preset_name);
            }
            // A tile value typed for an app the new preset does not bring
            // in is stale; drop it rather than silently keep sending it.
            let keep: std::collections::BTreeSet<String> = form
                .desc
                .steps
                .first()
                .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
                .unwrap_or_default();
            form.values
                .retain(|k, _| !k.starts_with("add-app-tile:") || keep.contains(k));
        }
        (EditKind::Firewall, _) => {
            if let Some(m) = edit(form).model.as_mut()
                && m.get(name).is_some()
            {
                m[name] = value;
            }
        }
        (EditKind::Rollback, "commit") if value != json!("") => {
            form.values.insert("unit".into(), json!(""));
        }
        (EditKind::Rollback, "unit") if value != json!("") => {
            form.values.insert("commit".into(), json!(""));
        }
        (EditKind::Native, "unit") => {
            let natives = edit(form).data["natives"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let m = natives
                .iter()
                .find(|n| n["unit"] == value)
                .map(|n| n["manifest"].clone())
                .unwrap_or(Value::Null);
            for field in [
                "binary",
                "env_file",
                "data_dirs",
                "update_cmd",
                "stateless",
                "restore_note",
                "release_repo",
                "release_asset",
                "backup_from_newest",
                "backup_pause",
                "update_policy",
                "metrics",
            ] {
                form.values.insert(field.into(), native_current(field, &m));
            }
        }
        _ => {}
    }
}

/// The buttons on screen and the table in words.
pub fn refresh(form: &mut OpenForm) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    let step = form.step.clone();
    let first = form.step_index == 0;
    let last = form.step_index == form.last();
    let e = form.edit.as_ref().expect("an edit form has its edit state");
    let mut b: Vec<&str> = Vec::new();
    if e.result.is_some() || form.job.is_some() {
        b.push("close");
    } else if let Some(sub) = &e.sub {
        b.push("save");
        if sub.kind == "key" {
            b.push("default");
        }
        b.push("cancel");
    } else {
        let valid = e.plan.as_ref().is_some_and(|p| p["valid"] == true);
        if !first {
            b.push("back");
        }
        match kind {
            _ if kind.stack_edit() || kind == EditKind::Import => match step.as_str() {
                "plan" => {
                    if valid {
                        b.push("next");
                    }
                }
                "commit" => b.push("confirm"),
                _ => {
                    let changed = match (kind, &e.model) {
                        (EditKind::Firewall, Some(m)) => {
                            firewall_changed(m, &e.data["manifest"]["firewall"])
                        }
                        _ => true,
                    };
                    if changed {
                        b.push("next");
                    }
                    // feat-native-1: a unit can be removed straight from
                    // the native step, the same "remove_native" the click
                    // path's own `#native-remove` button sends — a second
                    // button alongside "next", not its own `EditKind`.
                    if kind == EditKind::Native {
                        b.push("remove");
                    }
                }
            },
            _ if kind.preset_edit() => match step.as_str() {
                "plan" => {
                    if valid {
                        b.push("next");
                    }
                }
                "commit" => b.push("confirm"),
                _ => {
                    b.push("next");
                    // feat-preset-1: the Files card's own three buttons
                    // and "Remove this preset…" — only drawn for a preset
                    // that already exists, matching `preset_file_fields`.
                    if kind == EditKind::Preset {
                        b.push("save-file");
                        b.push("rename-file");
                        b.push("delete-file");
                        b.push("remove-preset");
                    }
                }
            },
            EditKind::NewStack => {
                if !last {
                    b.push("next");
                } else if valid {
                    b.push("confirm");
                }
            }
            EditKind::HostSettings => {
                if step == "review" {
                    b.push("confirm");
                } else if !e.staged.is_empty() {
                    b.push("next");
                }
            }
            EditKind::Batch => b.push("confirm"),
            _ => b.push("next"),
        }
        b.push("close");
    }
    form.buttons = b.into_iter().map(str::to_string).collect();
    let rows = match kind {
        EditKind::Firewall => e
            .model
            .as_ref()
            .and_then(|m| m["rules"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("{}. {}{new}", i + 1, rule_summary(&r["rule"]))
            })
            .collect(),
        EditKind::HostSettings => e.data["page"]["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| matches!(f["access"].as_str(), Some("browser" | "confirm")))
            .map(|f| {
                let key = f["key"].as_str().unwrap_or("");
                match e.staged.get(key) {
                    Some(v) => format!(
                        "{key} → {}",
                        if v.is_null() {
                            "(default)".to_string()
                        } else {
                            text_of(Some(v))
                        }
                    ),
                    None if f["set"] == true => format!("{key} = {}", key_text(f)),
                    None => format!("{key} (default {})", f["default"].as_str().unwrap_or("")),
                }
            })
            .collect(),
        EditKind::SettingsExt => e
            .model
            .as_ref()
            .and_then(|m| m["retention"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("retention {}. {}{new}", i + 1, retention_row_summary(r))
            })
            .collect(),
        EditKind::Apps => ["storage", "data_mounts", "log_files"]
            .iter()
            .flat_map(|list| {
                let summary_of: fn(&Value) -> String = match *list {
                    "storage" => storage_summary,
                    "data_mounts" => data_mount_summary,
                    _ => log_file_summary,
                };
                e.model
                    .as_ref()
                    .and_then(|m| m[*list].as_array())
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(move |(i, r)| {
                        let new = if r["origin"].is_null() { " (new)" } else { "" };
                        format!("{list} {}. {}{new}", i + 1, summary_of(r))
                    })
            })
            .collect(),
        EditKind::Latch => e
            .model
            .as_ref()
            .and_then(|m| m["latch_files"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("latch_files {}. {}{new}", i + 1, latch_file_summary(r))
            })
            .collect(),
        EditKind::Checks => ["checks", "manual", "probes"]
            .iter()
            .flat_map(|list| {
                let summary_of: fn(&Value) -> String = match *list {
                    "checks" => check_row_summary,
                    "manual" => manual_row_summary,
                    _ => probe_row_summary,
                };
                e.model
                    .as_ref()
                    .and_then(|m| m[*list].as_array())
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(move |(i, r)| {
                        let new = if r["origin"].is_null() { " (new)" } else { "" };
                        format!("{list} {}. {}{new}", i + 1, summary_of(r))
                    })
            })
            .collect(),
        EditKind::Tiles => e
            .model
            .as_ref()
            .and_then(|m| m["tiles"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("tiles {}. {}{new}", i + 1, tile_row_summary(r))
            })
            .collect(),
        _ => Vec::new(),
    };
    edit(form).rows = rows;
    form.fields = form.field_states();
}

/// `homelab ui row …`: the firewall's rules, host.toml's keys.
pub fn row(
    form: &mut OpenForm,
    step: &UiStep,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    let Family::Edit(kind) = form.desc.family else {
        return Err(refused(
            step,
            format!("the form {} has no table", form.title),
            "row steps are for the firewall and the host settings",
        ));
    };
    if form.sent() {
        return Err(refused(
            step,
            "the form was sent",
            "homelab ui close, then open it again",
        ));
    }
    if let Some(sub) = form.edit.as_ref().and_then(|e| e.sub.as_ref()) {
        return Err(refused(
            step,
            format!("the dialog {} is open", sub.title),
            "press save or cancel first",
        ));
    }
    match kind {
        EditKind::Firewall => {
            if form.step != "rules" {
                return Err(refused(
                    step,
                    format!("the rules are not on screen on the step {}", form.step),
                    "press back to the rules",
                ));
            }
            let model = edit(form).model.clone().unwrap_or(Value::Null);
            let rules = model["rules"].as_array().cloned().unwrap_or_default();
            let n = rules.len();
            let index = || -> Result<usize, Refusal> {
                let t = target.unwrap_or("");
                match t.parse::<usize>() {
                    Ok(i) if (1..=n).contains(&i) => Ok(i - 1),
                    _ => Err(refused(
                        step,
                        format!("there is no rule {t:?}"),
                        if n == 0 {
                            "there are no rules yet; homelab ui row add".to_string()
                        } else {
                            format!("the rules are numbered 1 to {n}, top to bottom")
                        },
                    )),
                }
            };
            let set_rules = |form: &mut OpenForm, rules: Vec<Value>| {
                if let Some(m) = edit(form).model.as_mut() {
                    m["rules"] = Value::Array(rules);
                }
            };
            match op {
                "add" | "edit" => {
                    let (target, rule) = if op == "add" {
                        if let Some(t) = target {
                            return Err(refused(
                                step,
                                format!("row add takes no number, not {t}"),
                                "homelab ui row add",
                            ));
                        }
                        (Value::Null, None)
                    } else {
                        let i = index()?;
                        (json!(i), Some(rules[i]["rule"].clone()))
                    };
                    let fields = rule_fields(rule.as_ref());
                    let values = fields
                        .iter()
                        .map(|f| (f.name.clone(), drive::start_value(f)))
                        .collect();
                    let title = match &rule {
                        Some(r) => format!("Edit rule · {}", rule_summary(r)),
                        None => "Add a rule".to_string(),
                    };
                    edit(form).sub = Some(Sub {
                        kind: "rule".into(),
                        title,
                        target,
                        fields,
                        values,
                        errors: BTreeMap::new(),
                    });
                }
                "up" | "down" => {
                    let i = index()?;
                    let j = if op == "up" {
                        i.checked_sub(1)
                    } else {
                        Some(i + 1).filter(|j| *j < n)
                    };
                    let Some(j) = j else {
                        return Err(refused(
                            step,
                            format!(
                                "rule {} is the {} already",
                                i + 1,
                                if op == "up" { "first" } else { "last" }
                            ),
                            "move another rule",
                        ));
                    };
                    let mut rules = rules;
                    rules.swap(i, j);
                    set_rules(form, rules);
                }
                "delete" => {
                    let i = index()?;
                    let mut rules = rules;
                    rules.remove(i);
                    set_rules(form, rules);
                }
                other => {
                    return Err(refused(
                        step,
                        format!("there is no row step {other}"),
                        "row add, or row edit|up|down|delete <number>",
                    ));
                }
            }
            Ok(plain())
        }
        EditKind::HostSettings => {
            if op != "edit" {
                return Err(refused(
                    step,
                    format!("a host.toml key has no row step {op}"),
                    "homelab ui row edit <key>",
                ));
            }
            if form.step != "keys" {
                return Err(refused(
                    step,
                    format!("the keys are not on screen on the step {}", form.step),
                    "press back to the keys",
                ));
            }
            let key = target.unwrap_or("");
            let e = edit(form);
            let fields = e.data["page"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let Some(f) = fields.iter().find(|f| f["key"] == key).cloned() else {
                return Err(refused(
                    step,
                    format!("host.toml has no key {key:?}"),
                    "the keys are the rows of homelab ui state",
                ));
            };
            let access = f["access"].as_str().unwrap_or("");
            if !matches!(access, "browser" | "confirm") {
                let who = match access {
                    "locked" => "ssh only (can cut the dashboard off)",
                    "ssh_only" => "ssh only (safety policy)",
                    "host_held" => "ssh only (generated and kept by the host itself)",
                    _ => "ssh only (secret)",
                };
                return Err(refused(
                    step,
                    format!("{key} is changed {who}"),
                    "change it over ssh on pve, not from the dashboard",
                ));
            }
            let ty = f["kind"]["type"].as_str().unwrap_or("text");
            let slug = key.replace('_', "-");
            let ids = &es().host_key;
            let pending = e.staged.get(key);
            let start = match (pending, ty) {
                (Some(v), "bool") => json!(*v == Value::Bool(true)),
                (Some(v), _) => json!(if v.is_null() {
                    String::new()
                } else {
                    text_of(Some(v))
                }),
                (None, "bool") => json!(f["value"] == Value::Bool(true)),
                (None, _) => json!(key_text(&f)),
            };
            let mut value = Field {
                name: "value".into(),
                id: fill(&ids.id, &[("slug", &slug)]),
                kind: match ty {
                    "bool" => FieldKind::Check,
                    "table" => FieldKind::Textarea,
                    _ => FieldKind::Text,
                },
                label: f["label"].as_str().unwrap_or(key).to_string(),
                help: f["help"].as_str().unwrap_or("").to_string(),
                required: false,
                pattern: None,
                placeholder: None,
                source: None,
                empty: None,
                danger: false,
                when: None,
                expect: None,
                min: None,
                max: None,
                choices: None,
                current: None,
                show_when: None,
                change_when: None,
            };
            value.current = Some(start);
            let mut sub_fields = vec![value];
            if access == "confirm" {
                sub_fields.push(Field {
                    name: "confirm".into(),
                    id: fill(&ids.confirm_id, &[("slug", &slug)]),
                    kind: FieldKind::Typed,
                    label: fill(&ids.confirm_label, &[("key", key)]),
                    help: String::new(),
                    required: true,
                    pattern: None,
                    placeholder: Some(key.to_string()),
                    source: None,
                    empty: None,
                    danger: true,
                    when: None,
                    expect: Some(key.to_string()),
                    min: None,
                    max: None,
                    choices: None,
                    current: None,
                    show_when: None,
                    change_when: None,
                });
            }
            let values = sub_fields
                .iter()
                .map(|f| (f.name.clone(), drive::start_value(f)))
                .collect();
            e.sub = Some(Sub {
                kind: "key".into(),
                title: format!("{} · {key}", f["label"].as_str().unwrap_or(key)),
                target: json!(key),
                fields: sub_fields,
                values,
                errors: BTreeMap::new(),
            });
            Ok(plain())
        }
        EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Checks
        | EditKind::Tiles => {
            // SettingsExt's own step is "settings_ext" (its form's field
            // grouping predates `kind.slug()`'s hyphen, "settings-ext",
            // which only ever named the `homelab ui open` verb).
            let expected_step = if kind == EditKind::SettingsExt {
                "settings_ext"
            } else {
                kind.slug()
            };
            if form.step != expected_step {
                return Err(refused(
                    step,
                    format!("the rows are not on screen on the step {}", form.step),
                    "press back",
                ));
            }
            let raw_target = target.unwrap_or("");
            let (list, n) = match raw_target.split_once(':') {
                Some((l, n)) => (l, Some(n)),
                None => (raw_target, None),
            };
            let allowed: &[&str] = match kind {
                EditKind::SettingsExt => &["retention"],
                EditKind::Apps => &["storage", "data_mounts", "log_files"],
                EditKind::Checks => &["checks", "manual", "probes"],
                EditKind::Tiles => &["tiles"],
                _ => &["latch_files"],
            };
            if !allowed.contains(&list) {
                return Err(refused(
                    step,
                    format!("{raw_target:?} does not name a list of this form"),
                    format!("the lists are: {}", allowed.join(", ")),
                ));
            }
            let model = edit(form).model.clone().unwrap_or(Value::Null);
            let rows = model[list].as_array().cloned().unwrap_or_default();
            let count = rows.len();
            let index = || -> Result<usize, Refusal> {
                let t = n.unwrap_or("");
                match t.parse::<usize>() {
                    Ok(i) if (1..=count).contains(&i) => Ok(i - 1),
                    _ => Err(refused(
                        step,
                        format!("there is no row {t:?} of {list}"),
                        if count == 0 {
                            format!("there are no {list} rows yet; homelab ui row add {list}")
                        } else {
                            format!("{list} rows are numbered 1 to {count}")
                        },
                    )),
                }
            };
            let set_rows = |form: &mut OpenForm, rows: Vec<Value>| {
                if let Some(m) = edit(form).model.as_mut() {
                    m[list] = Value::Array(rows);
                }
            };
            let apps = strings(&edit(form).data["manifest"]["apps"]);
            let natives = strings(&edit(form).data["manifest"]["natives"]);
            let groups: Vec<String> = edit(form).data["manifest"]["tiles"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(_, t)| t["group"].as_str().map(str::to_string))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let fields_for = |row: Option<&Value>| -> Vec<Field> {
                match list {
                    "storage" => storage_fields(&apps, row),
                    "data_mounts" => data_mount_fields(row),
                    "log_files" => log_file_fields(row),
                    "latch_files" => latch_file_fields(&natives, row),
                    "checks" => check_row_fields(row),
                    "manual" => manual_row_fields(row),
                    "probes" => probe_row_fields(row),
                    "retention" => retention_row_fields(row),
                    _ => tile_row_fields(&groups, row),
                }
            };
            let summary_of = |r: &Value| -> String {
                match list {
                    "storage" => storage_summary(r),
                    "data_mounts" => data_mount_summary(r),
                    "log_files" => log_file_summary(r),
                    "latch_files" => latch_file_summary(r),
                    "checks" => check_row_summary(r),
                    "manual" => manual_row_summary(r),
                    "probes" => probe_row_summary(r),
                    "retention" => retention_row_summary(r),
                    _ => tile_row_summary(r),
                }
            };
            match op {
                "add" | "edit" => {
                    let (sub_target, row) = if op == "add" {
                        if n.is_some() {
                            return Err(refused(
                                step,
                                format!("row add takes {list:?}, not {raw_target:?}"),
                                format!("homelab ui row add {list}"),
                            ));
                        }
                        (Value::Null, None)
                    } else {
                        let i = index()?;
                        (json!(i), Some(rows[i].clone()))
                    };
                    let fields = fields_for(row.as_ref());
                    let values = fields
                        .iter()
                        .map(|f| (f.name.clone(), drive::start_value(f)))
                        .collect();
                    let title = match &row {
                        Some(r) => format!("Edit · {}", summary_of(r)),
                        None => format!("Add a {} row", list.trim_end_matches('s')),
                    };
                    edit(form).sub = Some(Sub {
                        kind: list.to_string(),
                        title,
                        target: sub_target,
                        fields,
                        values,
                        errors: BTreeMap::new(),
                    });
                }
                "up" | "down" => {
                    let i = index()?;
                    let j = if op == "up" {
                        i.checked_sub(1)
                    } else {
                        Some(i + 1).filter(|j| *j < count)
                    };
                    let Some(j) = j else {
                        return Err(refused(
                            step,
                            format!(
                                "row {} is the {} already",
                                i + 1,
                                if op == "up" { "first" } else { "last" }
                            ),
                            "move another row",
                        ));
                    };
                    let mut rows = rows;
                    rows.swap(i, j);
                    set_rows(form, rows);
                }
                "delete" => {
                    let i = index()?;
                    let mut rows = rows;
                    rows.remove(i);
                    set_rows(form, rows);
                }
                other => {
                    return Err(refused(
                        step,
                        format!("there is no row step {other}"),
                        format!("row add {list}, or row edit|up|down|delete {list}:<number>"),
                    ));
                }
            }
            Ok(plain())
        }
        _ => Err(refused(
            step,
            format!("the form {} has no table", form.title),
            "row steps are for the firewall and the host settings",
        )),
    }
}

/// A press in an edit form.
pub fn press(
    form: &mut OpenForm,
    step: &UiStep,
    button: &str,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    let Family::Edit(kind) = form.desc.family else {
        return Err(refused(step, "not an edit form", "homelab ui state"));
    };
    if form.sent() {
        return Err(refused(
            step,
            "this form was sent already; one press runs once",
            "homelab ui state shows what it did; close the dialog to start over",
        ));
    }
    if !form.buttons.iter().any(|b| b == button) {
        return Err(refused(
            step,
            format!("{button} is not on screen on the step {}", form.step),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        ));
    }
    if kind.scope() > cx.scope {
        return Err(refused(
            step,
            format!("{} needs scope {:?}", kind.slug(), kind.scope()),
            "leave this press to Kenny",
        ));
    }
    // A dialog on top first.
    if let Some(sub) = form.edit.as_ref().and_then(|e| e.sub.clone()) {
        if button == "cancel" {
            edit(form).sub = None;
            return Ok(plain());
        }
        if sub.kind == "rule" {
            let mut errors = check_fields(&sub.fields, &sub.values);
            errors.extend(rule_problems(&sub.values));
            if !errors.is_empty() {
                let r = held_for(form, "save", &errors);
                if let Some(s) = edit(form).sub.as_mut() {
                    s.errors = errors;
                }
                return Ok(Applied {
                    effect: Effect::None,
                    held: Some(r),
                });
            }
            let rule = rule_from_values(&sub.values);
            let e = edit(form);
            if let Some(m) = e.model.as_mut() {
                let rules = m["rules"].as_array_mut().expect("the model has rules");
                match sub.target.as_u64() {
                    Some(i) => {
                        if let Some(x) = rules.get_mut(i as usize) {
                            x["rule"] = rule;
                        }
                    }
                    None => rules.push(json!({ "origin": null, "rule": rule })),
                }
            }
            e.sub = None;
            return Ok(plain());
        }
        if EditKind::row_list_kind(&sub.kind) {
            let list = sub.kind.as_str();
            let mut errors = check_fields(&sub.fields, &sub.values);
            errors.extend(match list {
                "data_mounts" => data_mount_problems(&sub.values),
                "latch_files" => latch_file_problems(&sub.values),
                "checks" => check_row_problems(&sub.values),
                "probes" => probe_row_problems(&sub.values),
                "tiles" => tile_row_problems(&sub.values),
                "retention" => retention_row_problems(&sub.values),
                _ => BTreeMap::new(),
            });
            if !errors.is_empty() {
                let r = held_for(form, "save", &errors);
                if let Some(s) = edit(form).sub.as_mut() {
                    s.errors = errors;
                }
                return Ok(Applied {
                    effect: Effect::None,
                    held: Some(r),
                });
            }
            let mut row = match list {
                "storage" => storage_from_values(&sub.values),
                "data_mounts" => data_mount_from_values(&sub.values),
                "log_files" => log_file_from_values(&sub.values),
                "latch_files" => latch_file_from_values(&sub.values),
                "checks" => check_row_from_values(&sub.values),
                "manual" => manual_row_from_values(&sub.values),
                "probes" => probe_row_from_values(&sub.values),
                "retention" => retention_row_from_values(&sub.values),
                _ => tile_row_from_values(&sub.values),
            };
            let e = edit(form);
            if let Some(m) = e.model.as_mut() {
                let rows = m[list].as_array_mut().expect("the model has this list");
                match sub.target.as_u64() {
                    Some(i) => {
                        if let Some(old) = rows.get(i as usize) {
                            row["origin"] = old["origin"].clone();
                        }
                        if let Some(x) = rows.get_mut(i as usize) {
                            *x = row;
                        }
                    }
                    None => {
                        row["origin"] = Value::Null;
                        rows.push(row);
                    }
                }
            }
            e.sub = None;
            return Ok(plain());
        }
        // A host.toml key: save or default.
        let key = sub.target.as_str().unwrap_or("").to_string();
        let mut errors = BTreeMap::new();
        let confirm = sub.fields.iter().any(|f| f.name == "confirm");
        if confirm && text_of(sub.values.get("confirm")).trim() != key {
            errors.insert("confirm".to_string(), say("key_confirm", &[("key", &key)]));
        }
        let value = if button == "default" {
            Ok(Value::Null)
        } else {
            let kind = edit(form).data["page"]["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|f| f["key"] == key.as_str())
                .map(|f| f["kind"].clone())
                .unwrap_or(Value::Null);
            parse_key(&kind, sub.values.get("value").unwrap_or(&Value::Null))
        };
        let value = match value {
            Ok(v) if errors.is_empty() => v,
            Ok(_) => Value::Null,
            Err(why) => {
                errors.insert("value".into(), why);
                Value::Null
            }
        };
        if !errors.is_empty() {
            let r = held_for(form, button, &errors);
            if let Some(s) = edit(form).sub.as_mut() {
                s.errors = errors;
            }
            return Ok(Applied {
                effect: Effect::None,
                held: Some(r),
            });
        }
        let e = edit(form);
        if confirm {
            e.confirmed.insert(key.clone());
        }
        e.staged.insert(key, value);
        e.sub = None;
        return Ok(plain());
    }
    if button == "back" {
        form.step_index = form.step_index.saturating_sub(1);
        form.errors.clear();
        form.run_error = None;
        return Ok(plain());
    }
    let step_id = form.step.clone();
    let hold = |form: &mut OpenForm, what: &str, errors: BTreeMap<String, String>| {
        let r = held_for(form, what, &errors);
        form.errors = errors;
        Ok(Applied {
            effect: Effect::None,
            held: Some(r),
        })
    };
    match (kind, step_id.as_str(), button) {
        (EditKind::Import, "plan", "next") => {
            form.step_index += 1;
            Ok(plain())
        }
        (EditKind::Import, "commit", "confirm") => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let fields: Vec<Field> = form.desc.steps[2].fields.clone();
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::ImportCommit { body }),
                held: None,
            })
        }
        (EditKind::Import, "bundle", "next") => {
            let mut at = form.desc.steps[0].clone();
            // The name and the number are checked as the new-stack wizard
            // checks its identity step.
            at.id = "identity".into();
            let names = strings(&edit(form).data["taken"]["names"]);
            let vmids: Vec<i64> = edit(form).data["taken"]["vmids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
            let errors = check_new_step(&at, &form.values, &names, &vmids);
            if !errors.is_empty() {
                return hold(form, "next", errors);
            }
            let body = json!({
                "bundle": text_of(form.values.get("bundle")),
                "name": text_of(form.values.get("name")).trim(),
                "vmid": js_number(&text_of(form.values.get("vmid"))),
            });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::ImportPlan { body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "next") => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let name = form.stack.clone();
            let unprivileged = edit(form).data["meta"]["unprivileged"].clone();
            let meta = preset_meta_body(&fields, &form.values, unprivileged);
            let body = json!({ "kind": "meta", "name": name, "meta": meta });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::NewPreset, "meta", "next") => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let name = text_of(form.values.get("name")).trim().to_string();
            if name.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("name".into(), say("needed", &[("label", "Preset name")]));
                return hold(form, "next", errors);
            }
            let meta = preset_meta_body(&fields, &form.values, Value::Null);
            let body = json!({ "kind": "meta", "name": name, "meta": meta });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "save-file") => {
            let name = form.stack.clone();
            let path = text_of(form.values.get("path")).trim().to_string();
            if path.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("path".into(), say("needed", &[("label", "Path")]));
                return hold(form, "save-file", errors);
            }
            let content = text_of(form.values.get("text"));
            let body = json!({ "kind": "file", "name": name, "path": path, "content": content });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("save-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "rename-file") => {
            let name = form.stack.clone();
            let from = text_of(form.values.get("file")).trim().to_string();
            let to = text_of(form.values.get("rename_to")).trim().to_string();
            let mut errors = BTreeMap::new();
            if from.is_empty() {
                errors.insert("file".into(), "pick the file to rename first".into());
            }
            if to.is_empty() {
                errors.insert("rename_to".into(), say("needed", &[("label", "New path")]));
            }
            if !errors.is_empty() {
                return hold(form, "rename-file", errors);
            }
            let body = json!({ "kind": "rename_file", "name": name, "from": from, "to": to });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("rename-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "delete-file") => {
            let name = form.stack.clone();
            let path = text_of(form.values.get("file")).trim().to_string();
            if path.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("file".into(), "pick the file to delete first".into());
                return hold(form, "delete-file", errors);
            }
            let body = json!({ "kind": "remove_file", "name": name, "path": path });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("delete-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "remove-preset") => {
            let name = form.stack.clone();
            let body = json!({ "kind": "remove", "name": name });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("remove-preset".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (k, "plan", "next") if k.preset_edit() => {
            form.step_index += 1;
            Ok(plain())
        }
        (k, "commit", "confirm") if k.preset_edit() => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let fields: Vec<Field> = form.desc.steps[2].fields.clone();
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetCommit { body }),
                held: None,
            })
        }
        (k, "plan", "next") if k.stack_edit() => {
            form.step_index += 1;
            Ok(plain())
        }
        (k, "commit", "confirm") if k.stack_edit() => {
            let fields: Vec<Field> = form.desc.steps[2].fields.clone();
            if text_of(form.values.get("subject")).trim().is_empty() {
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Commit {
                    stack: form.stack.clone(),
                    body,
                }),
                held: None,
            })
        }
        (EditKind::Native, _, "remove") => {
            let unit = text_of(form.values.get("unit"));
            let body = json!({ "kind": "remove_native", "unit": unit });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Plan {
                    stack: form.stack.clone(),
                    edit: body,
                }),
                held: None,
            })
        }
        (k, _, "next") if k.stack_edit() => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let body = match k {
                EditKind::Settings => {
                    let mut errors = check_fields(&fields, &form.values);
                    errors.extend(tile_problems(&form.values));
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let b = settings_body(&fields, &form.values);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Raw => {
                    let label_of = |name: &str| {
                        fields
                            .iter()
                            .find(|f| f.name == name)
                            .map(|f| f.label.clone())
                            .unwrap_or_else(|| name.to_string())
                    };
                    let op = text_of(form.values.get("op"));
                    match op.as_str() {
                        // feat-stacks-files: create, delete and rename, the
                        // same ops the Files card's buttons send
                        // (stackedit_files::FilesEdit) — "edit" (the
                        // default, unset `op` included) stays the
                        // original raw edit below.
                        "create" => {
                            let path = text_of(form.values.get("new_path"));
                            if path.trim().is_empty() {
                                let mut errors = BTreeMap::new();
                                errors.insert(
                                    "new_path".into(),
                                    say("needed", &[("label", &label_of("new_path"))]),
                                );
                                return hold(form, "next", errors);
                            }
                            let text = text_of(form.values.get("text"));
                            json!({ "kind": "files", "op": "create", "path": path, "content": text })
                        }
                        "delete" => {
                            let file = text_of(form.values.get("file"));
                            json!({ "kind": "files", "op": "delete", "path": file })
                        }
                        "rename" => {
                            let file = text_of(form.values.get("file"));
                            let to = text_of(form.values.get("rename_to"));
                            if to.trim().is_empty() {
                                let mut errors = BTreeMap::new();
                                errors.insert(
                                    "rename_to".into(),
                                    say("needed", &[("label", &label_of("rename_to"))]),
                                );
                                return hold(form, "next", errors);
                            }
                            json!({ "kind": "files", "op": "rename", "from": file, "to": to })
                        }
                        _ => {
                            let file = text_of(form.values.get("file"));
                            let text = text_of(form.values.get("text"));
                            if edit(form).data["texts"][&file] == json!(text) {
                                return Ok(held_msg("next", &say("raw_unchanged", &[])));
                            }
                            json!({ "kind": "raw", "path": file, "content": text })
                        }
                    }
                }
                EditKind::AddApp => {
                    let mut tiles = Map::new();
                    for f in &fields {
                        let Some(app) = f.name.strip_prefix("add-app-tile:") else {
                            continue;
                        };
                        let hostname = text_of(form.values.get(&f.name)).trim().to_string();
                        if !hostname.is_empty() {
                            tiles.insert(app.to_string(), json!(hostname));
                        }
                    }
                    let mut out = Map::new();
                    out.insert("kind".into(), json!("add_app"));
                    out.insert("preset".into(), json!(text_of(form.values.get("preset"))));
                    if !tiles.is_empty() {
                        out.insert("tiles".into(), Value::Object(tiles));
                    }
                    Value::Object(out)
                }
                EditKind::Native => native_body(&fields, &form.values),
                EditKind::AddNative => {
                    let unit = text_of(form.values.get("unit"));
                    let binary = text_of(form.values.get("binary"));
                    if unit.trim().is_empty() || binary.trim().is_empty() {
                        let mut errors = BTreeMap::new();
                        if unit.trim().is_empty() {
                            errors.insert("unit".into(), say("needed", &[("label", "Unit name")]));
                        }
                        if binary.trim().is_empty() {
                            errors.insert("binary".into(), say("needed", &[("label", "Binary")]));
                        }
                        return hold(form, "next", errors);
                    }
                    add_native_body(&fields, &form.values)
                }
                EditKind::SettingsExt => {
                    let errors = check_fields(&fields, &form.values);
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = settings_ext_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Apps => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = apps_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Latch => {
                    if let Some(Err(errors)) = latch_order_of(&fields, &form.values) {
                        return hold(form, "next", errors);
                    }
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = latch_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Checks => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data["manifest"].clone();
                    let app = model["app"].as_str().unwrap_or("").to_string();
                    let b = checks_drive_body(&form.values, &model, &data, &app);
                    if !checks_changed(&b, &data, &app) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Tiles => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = tiles_drive_body(&model, &data);
                    if b["tiles"].as_array().is_none_or(Vec::is_empty) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::PublishApp => {
                    let errors = check_fields(&fields, &form.values);
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let app = edit(form)
                        .model
                        .as_ref()
                        .and_then(|m| m["app"].as_str())
                        .unwrap_or("")
                        .to_string();
                    publish_body(&form.values, &app)
                }
                _ => firewall_body(edit(form).model.as_ref().unwrap_or(&Value::Null)),
            };
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Plan {
                    stack: form.stack.clone(),
                    edit: body,
                }),
                held: None,
            })
        }
        (EditKind::NewStack, "plan", "confirm") => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let follow = match form.values.get("follow") {
                Some(Value::String(s)) if !s.is_empty() => s.clone(),
                _ => "none".into(),
            };
            let mut body = Map::new();
            body.insert("stack".into(), new_stack_body(&form.values));
            body.insert(
                "subject".into(),
                json!(text_of(form.values.get("subject")).trim()),
            );
            let note = text_of(form.values.get("note")).trim().to_string();
            if !note.is_empty() {
                body.insert("note".into(), json!(note));
            }
            if follow != "none" {
                body.insert("follow".into(), json!(follow));
            }
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::NewCommit {
                    body: Value::Object(body),
                }),
                held: None,
            })
        }
        (EditKind::NewStack, s, "next") => {
            let at = form.desc.steps[form.step_index].clone();
            let names = strings(&edit(form).data["taken"]["names"]);
            let vmids: Vec<i64> = edit(form).data["taken"]["vmids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
            let errors = check_new_step(&at, &form.values, &names, &vmids);
            if !errors.is_empty() {
                return hold(form, "next", errors);
            }
            form.errors.clear();
            form.step_index += 1;
            let next = form.desc.steps[form.step_index].id.clone();
            let _ = s;
            let effect = match next.as_str() {
                "data" => Effect::Edit(EditCall::Appdata {
                    body: json!({
                        "preset": text_of(form.values.get("preset")),
                        "name": text_of(form.values.get("name")).trim(),
                        "vmid": js_number(&text_of(form.values.get("vmid"))),
                    }),
                }),
                "plan" => {
                    edit(form).plan = None;
                    Effect::Edit(EditCall::NewPlan {
                        body: new_stack_body(&form.values),
                    })
                }
                _ => Effect::None,
            };
            Ok(Applied { effect, held: None })
        }
        (EditKind::HostSettings, "keys", "next") => {
            form.step_index = 1;
            Ok(plain())
        }
        (EditKind::HostSettings, "review", "confirm") => {
            let e = edit(form);
            let fields = e.data["page"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let body = host_settings_body(
                e.data["page"]["sha256"].as_str().unwrap_or(""),
                &fields,
                &e.staged,
                &e.confirmed,
            );
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::HostWrite { body }),
                held: None,
            })
        }
        (EditKind::Batch, _, "confirm") => {
            let stacks = edit(form).stacks.clone();
            let action = edit(form).batch_action.clone().unwrap_or_default();
            let mut errors = BTreeMap::new();
            let fields: Vec<Field> = form.desc.fields().cloned().collect();
            for f in fields.iter().filter(|f| f.name.starts_with("confirm:")) {
                let s = &f.name["confirm:".len()..];
                if text_of(form.values.get(&f.name)).trim() != s {
                    errors.insert(f.name.clone(), say("batch_confirm", &[("stack", s)]));
                }
            }
            if !errors.is_empty() {
                return hold(form, "confirm", errors);
            }
            let mut args = Map::new();
            let mut confirms = Map::new();
            for f in &fields {
                let v = form.values.get(&f.name);
                if let Some(s) = f.name.strip_prefix("confirm:") {
                    let t = text_of(v).trim().to_string();
                    if !t.is_empty() {
                        confirms.insert(s.to_string(), json!(t));
                    }
                } else if f.kind == FieldKind::Check {
                    if v == Some(&Value::Bool(true)) {
                        args.insert(f.name.clone(), json!(true));
                    }
                } else {
                    let t = text_of(v).trim().to_string();
                    if !t.is_empty() {
                        args.insert(f.name.clone(), json!(t));
                    }
                }
            }
            let mut body = Map::new();
            body.insert("action".into(), json!(action));
            body.insert("stacks".into(), json!(stacks));
            body.insert("args".into(), Value::Object(args));
            if fields.iter().any(|f| f.name.starts_with("confirm:")) {
                body.insert("confirms".into(), Value::Object(confirms));
            }
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Batch {
                    body: Value::Object(body),
                }),
                held: None,
            })
        }
        (EditKind::Rollback, _, "next") => {
            let commit = text_of(form.values.get("commit"));
            let unit = text_of(form.values.get("unit"));
            let (action, preset) = if !commit.is_empty() {
                (ActionKind::DeployCommit, ("commit", commit))
            } else if !unit.is_empty() {
                (ActionKind::RollbackNative, ("unit", unit))
            } else {
                return Ok(held_msg("next", &say("rollback_choose", &[])));
            };
            if action.scope() > cx.scope {
                return Err(refused(
                    step,
                    format!("{} needs scope {:?}", action.slug(), action.scope()),
                    "leave it to Kenny",
                ));
            }
            let data = edit(form).data.clone();
            let sources = Sources {
                units: strings(&data["native_units"]),
                commits: data["commits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["commit"].as_str().map(str::to_string))
                    .collect(),
                ..Sources::default()
            };
            let mut next = OpenForm::new(drive::action_form(action, &form.stack), &sources);
            next.values.insert(preset.0.into(), json!(preset.1));
            next.refresh();
            let effect = if next.step == drive::REVIEW {
                Effect::Preview
            } else {
                Effect::None
            };
            *form = next;
            Ok(Applied { effect, held: None })
        }
        _ => Err(refused(
            step,
            format!("{button} does nothing on the step {step_id}"),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        )),
    }
}

/// The plan in short, for the tabs and `homelab ui state`.
pub fn plan_summary(p: &Value) -> Value {
    let files: Vec<Value> = p["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            json!({ "path": f["path"], "status": f["status"], "added": f["added"], "removed": f["removed"] })
        })
        .collect();
    let effects: Vec<Value> = p["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| e["what"].clone())
        .collect();
    json!({
        "valid": p["valid"],
        "unchanged": p["unchanged"],
        "problems": p["problems"],
        "subject": p["subject"],
        "follow_ups": p["follow_ups"],
        "files": files,
        "effects": effects,
        "restarts_dashboard": p["restarts_dashboard"],
    })
}

/// What the shell's call answered, into the form.
pub fn done(form: &mut OpenForm, call: &EditCall, outcome: Result<Value, Refusal>) {
    match (call, outcome) {
        (
            EditCall::Plan { .. }
            | EditCall::NewPlan { .. }
            | EditCall::ImportPlan { .. }
            | EditCall::PresetPlan { .. },
            Ok(plan),
        ) => {
            let follow_ups = strings(&plan["follow_ups"]);
            let fields = commit_fields(&follow_ups, plan["subject"].as_str().unwrap_or(""));
            // A preset's own steps are the same `plan_steps()` shape a
            // stack edit's are (`[first, "plan", "commit"]`), so its plan
            // lands on "commit" the same way `EditCall::Plan`'s does.
            let at = if matches!(
                call,
                EditCall::Plan { .. } | EditCall::ImportPlan { .. } | EditCall::PresetPlan { .. }
            ) {
                "commit"
            } else {
                "plan"
            };
            for f in &fields {
                form.values.insert(f.name.clone(), drive::start_value(f));
            }
            if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == at) {
                s.fields = fields;
            }
            form.run_error = None;
            edit(form).plan = Some(plan_summary(&plan));
        }
        (
            EditCall::Plan { .. }
            | EditCall::NewPlan { .. }
            | EditCall::ImportPlan { .. }
            | EditCall::PresetPlan { .. },
            Err(r),
        ) => {
            edit(form).plan = None;
            form.run_error = Some(r);
        }
        (EditCall::Appdata { .. }, outcome) => {
            let paths = outcome.map(|v| strings(&v["appdata"])).unwrap_or_default();
            let fields = data_fields(&paths);
            form.values.retain(|k, _| {
                !k.starts_with("nodata:") || paths.iter().any(|p| k == &format!("nodata:{p}"))
            });
            for f in &fields {
                form.values
                    .entry(f.name.clone())
                    .or_insert(Value::Bool(false));
            }
            if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == "data") {
                s.fields = fields;
            }
        }
        (_, Ok(v)) => {
            form.run_error = None;
            edit(form).result = Some(v);
        }
        (_, Err(r)) => form.run_error = Some(r),
    }
    form.refresh();
}
