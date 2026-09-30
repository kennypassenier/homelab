//! feat-stacks-10 (Area C, 2026-09-30): the "Apps" tab — remove an app,
//! add a blank one (no preset), and the full storage / data_mounts /
//! log_files lists. The deep rules (a storage path must sit under
//! `/appdata/`, be named `<app>-config`, the uid that matches privileged vs
//! unprivileged, …) are `homelab_core::manifest::validate_manifest`'s job
//! and run on the staged manifest the same way every other edit's do
//! (`edit::check_dir`); this module only builds the ops and the file
//! changes, and catches what would otherwise panic or silently misbehave
//! (an app that is not there, a path that is not a path). Pure.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use super::actions::valid_stack_name;
use super::stackedit::StackTexts;
use super::yamledit::{path, Item, Op};
use homelab_proto::StackManifest;

/// What the Apps tab asks for. Every list is `None` = leave that list
/// alone, `Some(items)` = this is the whole list now (the same "send the
/// end state, origins track the old rows" shape `FirewallEdit.rules` uses).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppsEdit {
    /// Apps to drop: from `apps:`, their directory, and any storage entry
    /// or data_mounts entry `app:` names. Data on the container itself is
    /// untouched — only the repository's declaration and its directory of
    /// compose files go.
    #[serde(default)]
    pub remove: Vec<String>,
    /// New apps with no preset: a directory and a minimal
    /// `docker-compose.yml` the owner fills in.
    #[serde(default)]
    pub add_blank: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<Vec<StorageEntryEdit>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_mounts: Option<Vec<DataMountEdit>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_files: Option<Vec<LogFileEdit>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEntryEdit {
    // NOTE: Clone is load-bearing — `apply` filters a copy of this list.
    #[serde(default)]
    pub origin: Option<usize>,
    pub host_path: String,
    pub mount_point: String,
    #[serde(default)]
    pub no_data: bool,
    #[serde(default)]
    pub no_backup: Option<String>,
    #[serde(default)]
    pub host_owner_uid: Option<u32>,
    #[serde(default)]
    pub app: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DataMountEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub host_path: String,
    pub mount_point: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub rotate: Option<LogRotationEdit>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogRotationEdit {
    pub files: String,
    #[serde(default)]
    pub keep: Option<u32>,
    #[serde(default)]
    pub reopen: Option<ReopenEdit>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReopenEdit {
    pub container: String,
    #[serde(default)]
    pub signal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogFileEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub path: String,
    pub job: String,
}

/// Shape checks this form keeps to before any text is touched. The
/// business rules (paths, ownership, naming) are the staged manifest
/// validator's job — see the module doc.
pub fn problems(m: &StackManifest, e: &AppsEdit) -> Vec<String> {
    let mut out = Vec::new();
    for a in &e.remove {
        if !m.apps.contains(a) {
            out.push(format!("{a} is not an app of this stack"));
        }
    }
    for a in &e.add_blank {
        if !valid_stack_name(a) {
            out.push(format!(
                "{a:?} is not an app name: lowercase letters, digits and '-'"
            ));
        } else if m.apps.contains(a) && !e.remove.contains(a) {
            out.push(format!("{a} is an app of this stack already"));
        }
    }
    {
        let mut seen = std::collections::BTreeSet::new();
        for a in &e.add_blank {
            if !seen.insert(a) {
                out.push(format!("{a} is a blank app twice"));
            }
        }
    }
    let path_ok = |p: &str| p.starts_with('/') && !p.is_empty();
    if let Some(list) = &e.storage {
        for s in list {
            if !path_ok(&s.host_path) || !path_ok(&s.mount_point) {
                out.push(format!(
                    "storage {}: both paths must be absolute",
                    s.host_path
                ));
            }
        }
    }
    if let Some(list) = &e.data_mounts {
        for d in list {
            if !path_ok(&d.host_path) || !path_ok(&d.mount_point) {
                out.push(format!(
                    "data_mounts {}: both paths must be absolute",
                    d.host_path
                ));
            }
            if let Some(r) = &d.rotate {
                if r.files.is_empty() || r.files.contains('/') {
                    out.push(format!(
                        "data_mounts {}: rotate files must be a name or glob, not a path",
                        d.host_path
                    ));
                }
            }
        }
    }
    if let Some(list) = &e.log_files {
        for l in list {
            if !path_ok(&l.path) {
                out.push(format!("log_files {}: path must be absolute", l.path));
            }
            if l.job.is_empty() {
                out.push(format!("log_files {}: job must not be empty", l.path));
            }
        }
    }
    out
}

fn storage_value(s: &StorageEntryEdit) -> Value {
    let mut m = Mapping::new();
    m.insert("host_path".into(), Value::from(s.host_path.as_str()));
    m.insert("mount_point".into(), Value::from(s.mount_point.as_str()));
    if s.no_data {
        m.insert("no_data".into(), Value::from(true));
    }
    if let Some(v) = &s.no_backup {
        m.insert("no_backup".into(), Value::from(v.as_str()));
    }
    if let Some(v) = s.host_owner_uid {
        m.insert("host_owner_uid".into(), Value::from(v));
    }
    if let Some(v) = &s.app {
        m.insert("app".into(), Value::from(v.as_str()));
    }
    Value::Mapping(m)
}

fn data_mount_value(d: &DataMountEdit) -> Value {
    let mut m = Mapping::new();
    m.insert("host_path".into(), Value::from(d.host_path.as_str()));
    m.insert("mount_point".into(), Value::from(d.mount_point.as_str()));
    if let Some(v) = &d.note {
        m.insert("note".into(), Value::from(v.as_str()));
    }
    if let Some(r) = &d.rotate {
        let mut rm = Mapping::new();
        rm.insert("files".into(), Value::from(r.files.as_str()));
        if let Some(k) = r.keep {
            rm.insert("keep".into(), Value::from(k));
        }
        if let Some(o) = &r.reopen {
            let mut om = Mapping::new();
            om.insert("container".into(), Value::from(o.container.as_str()));
            if let Some(sig) = &o.signal {
                om.insert("signal".into(), Value::from(sig.as_str()));
            }
            rm.insert("reopen".into(), Value::Mapping(om));
        }
        m.insert("rotate".into(), Value::Mapping(rm));
    }
    Value::Mapping(m)
}

fn log_file_value(l: &LogFileEdit) -> Value {
    let mut m = Mapping::new();
    m.insert("path".into(), Value::from(l.path.as_str()));
    m.insert("job".into(), Value::from(l.job.as_str()));
    Value::Mapping(m)
}

/// Origin-tracked items → a `Seq` op, or none when the list would come out
/// exactly as it already is.
fn seq_op<T>(
    key: &str,
    old_len: usize,
    edits: &[T],
    origin: impl Fn(&T) -> Option<usize>,
    unchanged: impl Fn(&T, usize) -> bool,
    to_value: impl Fn(&T) -> Value,
) -> Result<Option<Op>, String> {
    let mut used = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for e in edits {
        match origin(e) {
            Some(i) if i >= old_len => {
                return Err(format!(
                    "{key}: entry {} points at row {} of the file, which has {old_len}",
                    items.len() + 1,
                    i + 1
                ))
            }
            Some(i) if used.insert(i) => {
                if unchanged(e, i) {
                    items.push(Item::Keep(i));
                } else {
                    items.push(Item::Retext(i, to_value(e)));
                }
            }
            _ => items.push(Item::New(to_value(e))),
        }
    }
    let same = items.len() == old_len
        && items
            .iter()
            .enumerate()
            .all(|(n, it)| matches!(it, Item::Keep(i) if *i == n));
    if same {
        return Ok(None);
    }
    Ok(Some(Op::Seq {
        path: path(key),
        items,
    }))
}

fn mount_eq(e: &StorageEntryEdit, o: &homelab_core::manifest::MountSpec) -> bool {
    e.host_path == o.host_path
        && e.mount_point == o.mount_point
        && e.no_data == o.no_data
        && e.no_backup == o.no_backup
        && e.host_owner_uid == o.host_owner_uid
        && e.app == o.app
}

fn data_mount_eq(e: &DataMountEdit, o: &homelab_core::manifest::DataMount) -> bool {
    e.host_path == o.host_path
        && e.mount_point == o.mount_point
        && e.note == o.note
        && e.rotate.as_ref().map(|r| {
            (
                r.files.clone(),
                r.keep,
                r.reopen
                    .as_ref()
                    .map(|s| (s.container.clone(), s.signal.clone())),
            )
        }) == o.rotate.as_ref().map(|r| {
            (
                r.files.clone(),
                Some(r.keep),
                r.reopen
                    .as_ref()
                    .map(|s| (s.container.clone(), Some(s.signal.clone()))),
            )
        })
}

fn log_file_eq(e: &LogFileEdit, o: &homelab_core::manifest::LogFile) -> bool {
    e.path == o.path && e.job == o.job
}

/// One file this edit adds or removes, path relative to the stack's
/// directory (`None` content = removed).
pub struct FileWrite {
    pub rel: String,
    pub content: Option<String>,
}

/// The minimal skeleton a blank app gets; the owner fills in image,
/// ports and mounts by hand or in the raw editor afterwards.
pub fn blank_compose(app: &str) -> String {
    format!("services:\n  {app}:\n    image: CHANGE-ME:latest\n    restart: unless-stopped\n")
}

/// The ops on the manifest and the files this edit touches.
pub fn apply(
    m: &StackManifest,
    texts: &StackTexts,
    e: &AppsEdit,
) -> Result<(Vec<Op>, Vec<FileWrite>), String> {
    let mut ops = Vec::new();
    let mut files = Vec::new();

    if !e.remove.is_empty() || !e.add_blank.is_empty() {
        let mut apps: Vec<Item> = m
            .apps
            .iter()
            .enumerate()
            .filter(|(_, a)| !e.remove.contains(a))
            .map(|(i, _)| Item::Keep(i))
            .collect();
        apps.extend(
            e.add_blank
                .iter()
                .map(|a| Item::New(Value::from(a.as_str()))),
        );
        ops.push(Op::Seq {
            path: path("apps"),
            items: apps,
        });
    }
    for app in &e.remove {
        let prefix = format!("{app}/");
        for rel in texts.keys().filter(|k| k.starts_with(&prefix)) {
            files.push(FileWrite {
                rel: rel.clone(),
                content: None,
            });
        }
    }
    for app in &e.add_blank {
        files.push(FileWrite {
            rel: format!("{app}/docker-compose.yml"),
            content: Some(blank_compose(app)),
        });
    }

    if let Some(list) = &e.storage {
        let filtered: Vec<StorageEntryEdit> = list
            .iter()
            .filter(|s| {
                s.app
                    .as_deref()
                    .is_none_or(|a| !e.remove.contains(&a.to_string()))
            })
            .cloned()
            .collect();
        if let Some(op) = seq_op(
            "storage",
            m.storage.len(),
            &filtered,
            |s| s.origin,
            |s, i| m.storage.get(i).is_some_and(|o| mount_eq(s, o)),
            storage_value,
        )? {
            ops.push(op);
        }
    }
    if let Some(list) = &e.data_mounts {
        if let Some(op) = seq_op(
            "data_mounts",
            m.data_mounts.len(),
            list,
            |d| d.origin,
            |d, i| m.data_mounts.get(i).is_some_and(|o| data_mount_eq(d, o)),
            data_mount_value,
        )? {
            ops.push(op);
        }
    }
    if let Some(list) = &e.log_files {
        if let Some(op) = seq_op(
            "log_files",
            m.log_files.len(),
            list,
            |l| l.origin,
            |l, i| m.log_files.get(i).is_some_and(|o| log_file_eq(l, o)),
            log_file_value,
        )? {
            ops.push(op);
        }
    }
    Ok((ops, files))
}

/// The change in a few words, for the commit subject and for the plan's
/// "data on the container is untouched" note.
pub fn describe(e: &AppsEdit) -> Vec<String> {
    let mut parts = Vec::new();
    if !e.remove.is_empty() {
        parts.push(format!(
            "remove app{} {} (the declaration only — data on the container stays)",
            if e.remove.len() == 1 { "" } else { "s" },
            e.remove.join(", ")
        ));
    }
    if !e.add_blank.is_empty() {
        parts.push(format!(
            "add blank app{} {}",
            if e.add_blank.len() == 1 { "" } else { "s" },
            e.add_blank.join(", ")
        ));
    }
    if e.storage.is_some() {
        parts.push("storage entries".into());
    }
    if e.data_mounts.is_some() {
        parts.push("data mounts".into());
    }
    if e.log_files.is_some() {
        parts.push("log files".into());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> StackManifest {
        serde_yaml::from_str(
            r#"
stack_name: demo
vmid: 200
hostname: 200-app-demo
network: { ip: "10.10.10.10/24", gateway: "10.10.10.1" }
resources: { cores: 2, memory_mb: 1024, disk_gb: 8 }
lxc: { template: "local:vztmpl/x.tar.zst" }
boot: {}
apps: [jellyfin, sonarr]
storage:
  - host_path: /appdata/demo/jellyfin-config
    mount_point: /config
    app: jellyfin
"#,
        )
        .unwrap()
    }

    #[test]
    fn remove_unknown_app_is_a_problem() {
        let m = manifest();
        let e = AppsEdit {
            remove: vec!["nope".into()],
            ..Default::default()
        };
        let p = problems(&m, &e);
        assert_eq!(p, vec!["nope is not an app of this stack"]);
    }

    #[test]
    fn add_blank_rejects_bad_name_and_clash() {
        let m = manifest();
        let e = AppsEdit {
            add_blank: vec!["Bad Name".into(), "sonarr".into()],
            ..Default::default()
        };
        let p = problems(&m, &e);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn remove_app_drops_apps_entry_and_files() {
        let m = manifest();
        let mut texts = StackTexts::new();
        texts.insert("jellyfin/docker-compose.yml".into(), "services: {}".into());
        texts.insert("jellyfin/.env".into(), "X=1".into());
        texts.insert("sonarr/docker-compose.yml".into(), "services: {}".into());
        let e = AppsEdit {
            remove: vec!["jellyfin".into()],
            ..Default::default()
        };
        let (ops, files) = apply(&m, &texts, &e).unwrap();
        assert_eq!(ops.len(), 1);
        let mut rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, ["jellyfin/.env", "jellyfin/docker-compose.yml"]);
        assert!(files.iter().all(|f| f.content.is_none()));
    }

    #[test]
    fn add_blank_app_writes_a_skeleton() {
        let m = manifest();
        let texts = StackTexts::new();
        let e = AppsEdit {
            add_blank: vec!["radarr".into()],
            ..Default::default()
        };
        let (ops, files) = apply(&m, &texts, &e).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, "radarr/docker-compose.yml");
        assert!(files[0].content.as_ref().unwrap().contains("radarr"));
    }

    #[test]
    fn storage_seq_keeps_unchanged_rows() {
        let m = manifest();
        let texts = StackTexts::new();
        let e = AppsEdit {
            storage: Some(vec![StorageEntryEdit {
                origin: Some(0),
                host_path: "/appdata/demo/jellyfin-config".into(),
                mount_point: "/config".into(),
                no_data: false,
                no_backup: None,
                host_owner_uid: None,
                app: Some("jellyfin".into()),
            }]),
            ..Default::default()
        };
        let (ops, _) = apply(&m, &texts, &e).unwrap();
        assert!(ops.is_empty(), "no change should mean no op: {ops:?}");
    }

    #[test]
    fn removing_an_app_drops_its_storage_entry_too() {
        let m = manifest();
        let texts = StackTexts::new();
        let e = AppsEdit {
            remove: vec!["jellyfin".into()],
            storage: Some(vec![StorageEntryEdit {
                origin: Some(0),
                host_path: "/appdata/demo/jellyfin-config".into(),
                mount_point: "/config".into(),
                no_data: false,
                no_backup: None,
                host_owner_uid: None,
                app: Some("jellyfin".into()),
            }]),
            ..Default::default()
        };
        let (ops, _) = apply(&m, &texts, &e).unwrap();
        // apps op + storage op (now empty, so a Seq rebuilding it away)
        assert!(ops.iter().any(
            |o| matches!(o, Op::Seq { path: p, items } if p == &path("storage") && items.is_empty())
        ));
    }
}
