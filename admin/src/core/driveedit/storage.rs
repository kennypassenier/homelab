//! Edit forms: the Apps tab's row dialogs: storage, data mounts, log files.

use super::*;

/// `storageFields`.
pub fn storage_fields(apps: &[String], row: Option<&Value>) -> Vec<Field> {
    es().storage_entry
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "no_data" => json!(r["no_data"].as_bool().unwrap_or(false)),
                    "host_owner_uid" => json!(num_text(&r["host_owner_uid"])),
                    k => r[k].clone(),
                });
            }
            if d.name == "app" {
                let mut c = vec![Choice {
                    value: String::new(),
                    label: "(the stack itself)".into(),
                }];
                c.extend(apps.iter().map(|a| Choice {
                    value: a.clone(),
                    label: a.clone(),
                }));
                f.choices = Some(c);
            }
            f
        })
        .collect()
}

/// `storageFromValues` (no `origin`: the caller adds it).
pub fn storage_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert(
        "host_path".into(),
        json!(text_of(v.get("host_path")).trim()),
    );
    m.insert(
        "mount_point".into(),
        json!(text_of(v.get("mount_point")).trim()),
    );
    m.insert(
        "no_data".into(),
        json!(v.get("no_data") == Some(&Value::Bool(true))),
    );
    let app = text_of(v.get("app")).trim().to_string();
    if !app.is_empty() {
        m.insert("app".into(), json!(app));
    }
    let no_backup = text_of(v.get("no_backup")).trim().to_string();
    if !no_backup.is_empty() {
        m.insert("no_backup".into(), json!(no_backup));
    }
    let uid = text_of(v.get("host_owner_uid")).trim().to_string();
    if !uid.is_empty() {
        m.insert("host_owner_uid".into(), js_number(&uid));
    }
    Value::Object(m)
}

/// `storageSummary`.
pub fn storage_summary(row: &Value) -> String {
    let app = row["app"].as_str().filter(|a| !a.is_empty());
    match app {
        Some(a) => format!(
            "{} → {} ({a})",
            row["host_path"].as_str().unwrap_or(""),
            row["mount_point"].as_str().unwrap_or("")
        ),
        None => format!(
            "{} → {}",
            row["host_path"].as_str().unwrap_or(""),
            row["mount_point"].as_str().unwrap_or("")
        ),
    }
}

/// `dataMountFields`.
pub fn data_mount_fields(row: Option<&Value>) -> Vec<Field> {
    es().data_mount
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "rotate_files" => r["rotate"]["files"].clone(),
                    "rotate_keep" => json!(num_text(&r["rotate"]["keep"])),
                    "rotate_container" => r["rotate"]["reopen"]["container"].clone(),
                    "rotate_signal" => r["rotate"]["reopen"]["signal"].clone(),
                    k => r[k].clone(),
                });
            }
            f
        })
        .collect()
}

/// `dataMountFromValues`.
pub fn data_mount_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert(
        "host_path".into(),
        json!(text_of(v.get("host_path")).trim()),
    );
    m.insert(
        "mount_point".into(),
        json!(text_of(v.get("mount_point")).trim()),
    );
    let note = text_of(v.get("note")).trim().to_string();
    if !note.is_empty() {
        m.insert("note".into(), json!(note));
    }
    let files = text_of(v.get("rotate_files")).trim().to_string();
    if !files.is_empty() {
        let mut r = Map::new();
        r.insert("files".into(), json!(files));
        let keep = text_of(v.get("rotate_keep")).trim().to_string();
        if !keep.is_empty() {
            r.insert("keep".into(), js_number(&keep));
        }
        let container = text_of(v.get("rotate_container")).trim().to_string();
        if !container.is_empty() {
            let mut reopen = Map::new();
            reopen.insert("container".into(), json!(container));
            let signal = text_of(v.get("rotate_signal")).trim().to_string();
            if !signal.is_empty() {
                reopen.insert("signal".into(), json!(signal));
            }
            r.insert("reopen".into(), Value::Object(reopen));
        }
        m.insert("rotate".into(), Value::Object(r));
    }
    Value::Object(m)
}

/// `dataMountProblems`.
pub fn data_mount_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if text_of(v.get("rotate_files")).trim().is_empty() {
        for k in ["rotate_keep", "rotate_container", "rotate_signal"] {
            if !text_of(v.get(k)).trim().is_empty() {
                out.insert(k.into(), "Needs a rotate file name/glob above.".into());
            }
        }
    }
    out
}

/// `dataMountSummary`.
pub fn data_mount_summary(row: &Value) -> String {
    format!(
        "{} → {}",
        row["host_path"].as_str().unwrap_or(""),
        row["mount_point"].as_str().unwrap_or("")
    )
}

/// `logFileFields`.
pub fn log_file_fields(row: Option<&Value>) -> Vec<Field> {
    es().log_file
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(r[&d.name].clone());
            }
            f
        })
        .collect()
}

/// `logFileFromValues`.
pub fn log_file_from_values(v: &Values) -> Value {
    json!({
        "path": text_of(v.get("path")).trim(),
        "job": text_of(v.get("job")).trim(),
    })
}

/// `logFileSummary`.
pub fn log_file_summary(row: &Value) -> String {
    format!(
        "{} ({})",
        row["path"].as_str().unwrap_or(""),
        row["job"].as_str().unwrap_or("")
    )
}
