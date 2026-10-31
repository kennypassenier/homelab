//! The buttons on screen and the table in words.

use super::*;

/// Where the form stands, for choosing its buttons.
struct Stand<'a> {
    kind: EditKind,
    e: &'a EditState,
    step: &'a str,
    first: bool,
    last: bool,
}

pub fn refresh(form: &mut OpenForm) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    let e = form.edit.as_ref().expect("an edit form has its edit state");
    let finished = e.result.is_some() || form.job.is_some();
    let stand = Stand {
        kind,
        e,
        step: &form.step,
        first: form.step_index == 0,
        last: form.step_index == form.last(),
    };
    let buttons: Vec<String> = buttons(&stand, finished)
        .into_iter()
        .map(str::to_string)
        .collect();
    let rows = table_rows(kind, e);
    form.buttons = buttons;
    edit(form).rows = rows;
    form.fields = form.field_states();
}

fn buttons(s: &Stand, finished: bool) -> Vec<&'static str> {
    let mut b: Vec<&'static str> = Vec::new();
    if finished {
        b.push("close");
    } else if let Some(sub) = &s.e.sub {
        b.push("save");
        if sub.kind == "key" {
            b.push("default");
        }
        b.push("cancel");
    } else {
        if !s.first {
            b.push("back");
        }
        step_buttons(s, &mut b);
        b.push("close");
    }
    b
}

fn step_buttons(s: &Stand, b: &mut Vec<&'static str>) {
    let (kind, e) = (s.kind, s.e);
    let valid = e.plan.as_ref().is_some_and(|p| p["valid"] == true);
    match kind {
        _ if kind.stack_edit() || kind == EditKind::Import => stack_buttons(s, valid, b),
        _ if kind.preset_edit() => preset_buttons(s, valid, b),
        EditKind::NewStack => {
            if !s.last {
                b.push("next");
            } else if valid {
                b.push("confirm");
            }
        }
        EditKind::HostSettings => {
            if s.step == "review" {
                b.push("confirm");
            } else if !e.staged.is_empty() {
                b.push("next");
            }
        }
        EditKind::Batch => b.push("confirm"),
        _ => b.push("next"),
    }
}

fn stack_buttons(s: &Stand, valid: bool, b: &mut Vec<&'static str>) {
    match s.step {
        "plan" => {
            if valid {
                b.push("next");
            }
        }
        "commit" => b.push("confirm"),
        _ => {
            let changed = match (s.kind, &s.e.model) {
                (EditKind::Firewall, Some(m)) => {
                    firewall_changed(m, &s.e.data["manifest"]["firewall"])
                }
                _ => true,
            };
            if changed {
                b.push("next");
            }
            // feat-native-1: a unit can be removed straight from the native
            // step, the same "remove_native" the click path's own
            // `#native-remove` button sends: a second button alongside
            // "next", not its own `EditKind`.
            if s.kind == EditKind::Native {
                b.push("remove");
            }
        }
    }
}

fn preset_buttons(s: &Stand, valid: bool, b: &mut Vec<&'static str>) {
    match s.step {
        "plan" => {
            if valid {
                b.push("next");
            }
        }
        "commit" => b.push("confirm"),
        _ => {
            b.push("next");
            // feat-preset-1: the Files card's own three buttons and "Remove
            // this preset…": only drawn for a preset that already exists,
            // matching `preset_file_fields`.
            if s.kind == EditKind::Preset {
                b.extend(["save-file", "rename-file", "delete-file", "remove-preset"]);
            }
        }
    }
}

/// One list of the model, numbered, `(new)` where it has no origin.
fn model_rows(
    e: &EditState,
    list: &str,
    prefix: &str,
    summary_of: fn(&Value) -> String,
) -> Vec<String> {
    e.model
        .as_ref()
        .and_then(|m| m[list].as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, r)| {
            let new = if r["origin"].is_null() { " (new)" } else { "" };
            format!("{prefix}{}. {}{new}", i + 1, summary_of(r))
        })
        .collect()
}

fn table_rows(kind: EditKind, e: &EditState) -> Vec<String> {
    let lists = |names: [&str; 3], sums: [fn(&Value) -> String; 3]| -> Vec<String> {
        (0..3)
            .flat_map(|i| model_rows(e, names[i], &format!("{} ", names[i]), sums[i]))
            .collect()
    };
    match kind {
        EditKind::Firewall => firewall_rows(e),
        EditKind::HostSettings => host_key_rows(e),
        EditKind::SettingsExt => model_rows(e, "retention", "retention ", retention_row_summary),
        EditKind::Apps => lists(
            ["storage", "data_mounts", "log_files"],
            [storage_summary, data_mount_summary, log_file_summary],
        ),
        EditKind::Latch => model_rows(e, "latch_files", "latch_files ", latch_file_summary),
        EditKind::Checks => lists(
            ["checks", "manual", "probes"],
            [check_row_summary, manual_row_summary, probe_row_summary],
        ),
        EditKind::Tiles => model_rows(e, "tiles", "tiles ", tile_row_summary),
        _ => Vec::new(),
    }
}

fn rule_row_summary(r: &Value) -> String {
    rule_summary(&r["rule"])
}

fn firewall_rows(e: &EditState) -> Vec<String> {
    model_rows(e, "rules", "", rule_row_summary)
}

fn host_key_rows(e: &EditState) -> Vec<String> {
    e.data["page"]["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| matches!(f["access"].as_str(), Some("browser" | "confirm")))
        .map(|f| {
            let key = f["key"].as_str().unwrap_or("");
            match e.staged.get(key) {
                Some(v) => format!("{key} → {}", staged_text(v)),
                None if f["set"] == true => format!("{key} = {}", key_text(f)),
                None => format!("{key} (default {})", f["default"].as_str().unwrap_or("")),
            }
        })
        .collect()
}

fn staged_text(v: &Value) -> String {
    if v.is_null() {
        "(default)".to_string()
    } else {
        text_of(Some(v))
    }
}
