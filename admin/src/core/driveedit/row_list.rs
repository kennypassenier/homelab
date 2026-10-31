//! `homelab ui row …` on the row lists of Apps, Latch, Checks, Tiles and
//! the settings-ext form.

use super::*;

/// What the forms of a list's rows are built from.
struct Lists {
    apps: Vec<String>,
    natives: Vec<String>,
    groups: Vec<String>,
}

impl Lists {
    fn of(form: &mut OpenForm) -> Lists {
        let manifest = &edit(form).data["manifest"];
        let groups: BTreeSet<String> = manifest["tiles"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(_, t)| t["group"].as_str().map(str::to_string))
            .collect();
        Lists {
            apps: strings(&manifest["apps"]),
            natives: strings(&manifest["natives"]),
            groups: groups.into_iter().collect(),
        }
    }

    fn fields(&self, list: &str, row: Option<&Value>) -> Vec<Field> {
        match list {
            "storage" => storage_fields(&self.apps, row),
            "data_mounts" => data_mount_fields(row),
            "log_files" => log_file_fields(row),
            "latch_files" => latch_file_fields(&self.natives, row),
            "checks" => check_row_fields(row),
            "manual" => manual_row_fields(row),
            "probes" => probe_row_fields(row),
            "retention" => retention_row_fields(row),
            _ => tile_row_fields(&self.groups, row),
        }
    }
}

fn summary_of(list: &str, r: &Value) -> String {
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
}

fn allowed_lists(kind: EditKind) -> &'static [&'static str] {
    match kind {
        EditKind::SettingsExt => &["retention"],
        EditKind::Apps => &["storage", "data_mounts", "log_files"],
        EditKind::Checks => &["checks", "manual", "probes"],
        EditKind::Tiles => &["tiles"],
        _ => &["latch_files"],
    }
}

fn row_index(step: &UiStep, list: &str, n: Option<&str>, count: usize) -> Result<usize, Refusal> {
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
}

/// The step the form's rows are on: settings-ext's own step grouping
/// predates `kind.slug()`'s hyphen, "settings-ext", which only ever named
/// the `homelab ui open` verb.
fn rows_step(kind: EditKind) -> &'static str {
    if kind == EditKind::SettingsExt {
        "settings_ext"
    } else {
        kind.slug()
    }
}

/// The list the target names, and the row number after the colon.
fn split_list<'a>(
    step: &UiStep,
    kind: EditKind,
    raw_target: &'a str,
) -> Result<(&'a str, Option<&'a str>), Refusal> {
    let (list, n) = match raw_target.split_once(':') {
        Some((l, n)) => (l, Some(n)),
        None => (raw_target, None),
    };
    let allowed = allowed_lists(kind);
    if !allowed.contains(&list) {
        return Err(refused(
            step,
            format!("{raw_target:?} does not name a list of this form"),
            format!("the lists are: {}", allowed.join(", ")),
        ));
    }
    Ok((list, n))
}

fn list_dialog(
    step: &UiStep,
    lists: &Lists,
    op: &str,
    list: &str,
    (raw_target, n): (&str, Option<&str>),
    rows: &[Value],
) -> Result<Sub, Refusal> {
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
        let i = row_index(step, list, n, rows.len())?;
        (json!(i), Some(rows[i].clone()))
    };
    let title = match &row {
        Some(r) => format!("Edit · {}", summary_of(list, r)),
        None => format!("Add a {} row", list.trim_end_matches('s')),
    };
    Ok(sub_of(
        list,
        title,
        sub_target,
        lists.fields(list, row.as_ref()),
    ))
}

fn unknown_op(step: &UiStep, list: &str, other: &str) -> Refusal {
    refused(
        step,
        format!("there is no row step {other}"),
        format!("row add {list}, or row edit|up|down|delete {list}:<number>"),
    )
}

pub(super) fn row_list(
    form: &mut OpenForm,
    step: &UiStep,
    kind: EditKind,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    if form.step != rows_step(kind) {
        return Err(refused(
            step,
            format!("the rows are not on screen on the step {}", form.step),
            "press back",
        ));
    }
    let raw_target = target.unwrap_or("");
    let (list, n) = split_list(step, kind, raw_target)?;
    let model = edit(form).model.clone().unwrap_or(Value::Null);
    let mut rows = model[list].as_array().cloned().unwrap_or_default();
    let count = rows.len();
    match op {
        "add" | "edit" => {
            let lists = Lists::of(form);
            edit(form).sub = Some(list_dialog(step, &lists, op, list, (raw_target, n), &rows)?);
        }
        "up" | "down" => {
            let i = row_index(step, list, n, count)?;
            set_model_list(form, list, moved(step, "row", (op, i), rows)?);
        }
        "delete" => {
            rows.remove(row_index(step, list, n, count)?);
            set_model_list(form, list, rows);
        }
        other => return Err(unknown_op(step, list, other)),
    }
    Ok(plain())
}
