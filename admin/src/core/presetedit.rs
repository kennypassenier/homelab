//! feat-preset-1 (D): editing `presets/` itself — a preset's metadata, its
//! app files, creating a new preset, removing one. Not a `StackEdit`:
//! `presets/` sits beside `stacks/`, not under any one stack, so it gets
//! its own scope and its own commit
//! (`WorkingCopy::transact_presets`, `admin/src/shell/workcopy.rs`,
//! mirroring `transact`). The same edit-plan → diff → commit flow as a
//! stack edit; Kenny never hand-edits a preset's YAML either. Pure:
//! current texts in, new texts out.

use homelab_client::scaffold::PresetMeta;
use serde::{Deserialize, Serialize};

use super::actions::Refusal;
use super::stackedit::{FileChange, RAW_MAX};

/// The file every preset has.
pub const PRESET_META: &str = "preset.yml";

fn refusal(why: impl Into<String>, fix: impl Into<String>) -> Refusal {
    Refusal::new("the preset edit", why, fix)
}

/// A preset directory name: non-empty, lowercase `[a-z0-9-]`, not
/// `_`-prefixed (`scan_presets` reserves that for core apps, presently
/// unused — D8).
pub fn valid_name(name: &str) -> Result<String, Refusal> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 40
        && !name.starts_with('_')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !ok {
        return Err(refusal(
            format!("{name:?} is not a preset name (lowercase letters, digits, '-', not starting with '_')"),
            "pick a name like the presets the catalogue already has",
        ));
    }
    Ok(name.to_string())
}

/// A path inside a preset the file editor may write: relative, no `..`, not
/// `preset.yml` itself under `RemoveFile` (that would leave the preset
/// without metadata), never a secret.
fn valid_file_path(path: &str) -> Result<String, Refusal> {
    let path = path.trim();
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(refusal(
            format!("{path:?} is not a path inside a preset"),
            "use a plain relative path, e.g. myapp/docker-compose.yml",
        ));
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == ".env" || name.ends_with(".env") || name.starts_with(".env") {
        return Err(refusal(
            format!(
                "{path} would hold secrets; a preset ships no secret, only the shape of the app"
            ),
            "leave secrets out of the preset; they are filled in per stack, through latch",
        ));
    }
    Ok(path.to_string())
}

/// Every range `preset.yml` is kept to, checked before any text is touched.
fn meta_problems(meta: &PresetMeta) -> Vec<String> {
    let mut out = Vec::new();
    if !(128..=262_144).contains(&meta.ram_mb) {
        out.push(format!(
            "memory must be from 128 to 262144 MB, not {}",
            meta.ram_mb
        ));
    }
    if let Some(c) = meta.cores {
        if !(1..=64).contains(&c) {
            out.push(format!("cores must be from 1 to 64, not {c}"));
        }
    }
    if let Some(d) = meta.disk_gb {
        if !(2..=4096).contains(&d) {
            out.push(format!("disk must be from 2 to 4096 GB, not {d}"));
        }
    }
    out
}

/// A cheap syntax check for a file the editor is about to write: YAML
/// files must at least parse (the same net the stack raw editor and
/// `check_dir` cast; a preset has no per-app schema to check beyond that
/// here — an app's own `checks.yml` is checked when it joins a stack).
fn syntax_problem(path: &str, content: &str) -> Option<String> {
    if !(path.ends_with(".yml") || path.ends_with(".yaml")) {
        return None;
    }
    serde_yaml::from_str::<serde_yaml::Value>(content)
        .err()
        .map(|e| format!("{path} does not read as YAML: {e}"))
}

/// The presets directory's texts, keyed `<preset>/<rel>` — the same shape
/// `stackedit::StackTexts` uses for a stack, read from `presets/` instead
/// (`WorkingCopy::preset_texts`).
pub type PresetTexts = std::collections::BTreeMap<String, String>;

/// What the browser sends to the presets editor's `…/plan` and `…/commit`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresetEdit {
    /// Create a preset (a name not yet in the working copy) or rewrite an
    /// existing one's `preset.yml` — the whole document; there are no
    /// hand-kept comments in a preset's metadata to preserve.
    Meta { name: String, meta: PresetMeta },
    /// One file of a preset, typed in whole — new or existing.
    File {
        name: String,
        path: String,
        content: String,
    },
    /// Remove one file of a preset (not `preset.yml` itself).
    RemoveFile { name: String, path: String },
    /// Rename one file of a preset (Area A's Files card, for a preset —
    /// not `preset.yml` itself).
    RenameFile {
        name: String,
        from: String,
        to: String,
    },
    /// Remove the whole preset — every file under `presets/<name>/`.
    Remove { name: String },
}

impl PresetEdit {
    pub fn kind(&self) -> &'static str {
        match self {
            PresetEdit::Meta { .. } => "meta",
            PresetEdit::File { .. } => "file",
            PresetEdit::RemoveFile { .. } => "remove_file",
            PresetEdit::RenameFile { .. } => "rename_file",
            PresetEdit::Remove { .. } => "remove",
        }
    }

    pub fn preset_name(&self) -> &str {
        match self {
            PresetEdit::Meta { name, .. }
            | PresetEdit::File { name, .. }
            | PresetEdit::RemoveFile { name, .. }
            | PresetEdit::RenameFile { name, .. }
            | PresetEdit::Remove { name } => name,
        }
    }
}

/// The change in a few words, for the commit subject.
pub fn describe(edit: &PresetEdit) -> String {
    match edit {
        PresetEdit::Meta { name, .. } => format!("{name}: preset.yml"),
        PresetEdit::File { name, path, .. } => format!("{name}: {path}"),
        PresetEdit::RemoveFile { name, path } => format!("{name}: remove {path}"),
        PresetEdit::RenameFile { name, from, to } => format!("{name}: {from} renamed to {to}"),
        PresetEdit::Remove { name } => format!("remove the {name} preset"),
    }
}

/// The new texts for one edit (only the files that change), and any
/// problems with it beyond a bad name or path (the YAML syntax check and
/// `preset.yml`'s own ranges).
pub fn changes(texts: &PresetTexts, edit: &PresetEdit) -> Result<Vec<FileChange>, Refusal> {
    let full = |name: &str, rel: &str| format!("presets/{name}/{rel}");
    match edit {
        PresetEdit::Meta { name, meta } => {
            let name = valid_name(name)?;
            let problems = meta_problems(meta);
            if !problems.is_empty() {
                return Err(refusal(
                    problems.join("; "),
                    "correct the values in the form",
                ));
            }
            let key = format!("{name}/{PRESET_META}");
            let old = texts.get(&key).cloned();
            let new = serde_yaml::to_string(meta)
                .map_err(|e| refusal(e.to_string(), "reload the presets editor and try again"))?;
            if old.as_deref() == Some(new.as_str()) {
                return Ok(Vec::new());
            }
            Ok(vec![FileChange {
                path: full(&name, PRESET_META),
                old,
                new: Some(new),
            }])
        }
        PresetEdit::File {
            name,
            path,
            content,
        } => {
            let name = valid_name(name)?;
            let rel = valid_file_path(path)?;
            if content.len() > RAW_MAX {
                return Err(refusal(
                    format!(
                        "{rel} would be {} bytes; the editor takes at most {RAW_MAX}",
                        content.len()
                    ),
                    "split the file, or edit it in a workstation clone",
                ));
            }
            if let Some(why) = syntax_problem(&rel, content) {
                return Err(refusal(why, "fix the YAML and try again"));
            }
            let key = format!("{name}/{rel}");
            let old = texts.get(&key).cloned();
            if old.as_deref() == Some(content.as_str()) {
                return Ok(Vec::new());
            }
            Ok(vec![FileChange {
                path: full(&name, &rel),
                old,
                new: Some(content.clone()),
            }])
        }
        PresetEdit::RemoveFile { name, path } => {
            let name = valid_name(name)?;
            let rel = valid_file_path(path)?;
            if rel == PRESET_META {
                return Err(refusal(
                    "preset.yml cannot be removed on its own",
                    "remove the whole preset instead",
                ));
            }
            let key = format!("{name}/{rel}");
            let Some(old) = texts.get(&key).cloned() else {
                return Err(refusal(
                    format!("{rel} is not a file of {name}"),
                    "pick a file the editor lists",
                ));
            };
            Ok(vec![FileChange {
                path: full(&name, &rel),
                old: Some(old),
                new: None,
            }])
        }
        PresetEdit::RenameFile { name, from, to } => {
            let name = valid_name(name)?;
            let from_rel = valid_file_path(from)?;
            let to_rel = valid_file_path(to)?;
            if from_rel == PRESET_META || to_rel == PRESET_META {
                return Err(refusal(
                    "preset.yml is the preset's own metadata and cannot be renamed",
                    "leave preset.yml where it is",
                ));
            }
            let from_key = format!("{name}/{from_rel}");
            let Some(content) = texts.get(&from_key).cloned() else {
                return Err(refusal(
                    format!("{from_rel} is not a file of {name}"),
                    "pick a file the editor lists",
                ));
            };
            if from_rel == to_rel {
                return Err(refusal(
                    "the new path is the same as the old one",
                    "change the path, or cancel",
                ));
            }
            let to_key = format!("{name}/{to_rel}");
            if texts.contains_key(&to_key) {
                return Err(refusal(
                    format!("{to_rel} already exists in {name}"),
                    "pick a path the editor does not already list",
                ));
            }
            Ok(vec![
                FileChange {
                    path: full(&name, &from_rel),
                    old: Some(content.clone()),
                    new: None,
                },
                FileChange {
                    path: full(&name, &to_rel),
                    old: None,
                    new: Some(content),
                },
            ])
        }
        PresetEdit::Remove { name } => {
            let name = valid_name(name)?;
            let prefix = format!("{name}/");
            let files: Vec<FileChange> = texts
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix))
                .map(|(k, v)| FileChange {
                    path: format!("presets/{k}"),
                    old: Some(v.clone()),
                    new: None,
                })
                .collect();
            if files.is_empty() {
                return Err(refusal(
                    format!("there is no preset called {name}"),
                    "pick one the page lists",
                ));
            }
            Ok(files)
        }
    }
}

/// Every path a set of changes touches outside `presets/`; the commit
/// refuses when this is not empty (`outside_stack`'s twin).
pub fn outside_presets<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|p| {
            !p.starts_with("presets/")
                || p.contains("/../")
                || p.ends_with("/..")
                || *p == "presets/"
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rejects_underscore_and_uppercase() {
        assert!(valid_name("_core").is_err());
        assert!(valid_name("MyPreset").is_err());
        assert!(valid_name("").is_err());
        assert!(valid_name("my-preset-2").is_ok());
    }

    #[test]
    fn file_path_rejects_secrets_and_traversal() {
        assert!(valid_file_path("app/.env").is_err());
        assert!(valid_file_path("../escape").is_err());
        assert!(valid_file_path("app/docker-compose.yml").is_ok());
    }

    #[test]
    fn meta_change_is_noop_when_equal() {
        let meta = PresetMeta {
            description: "x".into(),
            ram_mb: 1024,
            ..Default::default()
        };
        let text = serde_yaml::to_string(&meta).unwrap();
        let mut texts = PresetTexts::new();
        texts.insert("demo/preset.yml".into(), text);
        let edit = PresetEdit::Meta {
            name: "demo".into(),
            meta,
        };
        assert!(changes(&texts, &edit).unwrap().is_empty());
    }

    #[test]
    fn remove_missing_preset_is_refused() {
        let texts = PresetTexts::new();
        let edit = PresetEdit::Remove {
            name: "ghost".into(),
        };
        assert!(changes(&texts, &edit).is_err());
    }

    #[test]
    fn rename_file_moves_content_and_refuses_collisions() {
        let mut texts = PresetTexts::new();
        texts.insert(
            "demo/myapp/docker-compose.yml".into(),
            "services: {}".into(),
        );
        let edit = PresetEdit::RenameFile {
            name: "demo".into(),
            from: "myapp/docker-compose.yml".into(),
            to: "myapp2/docker-compose.yml".into(),
        };
        let out = changes(&texts, &edit).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .any(|c| c.path == "presets/demo/myapp/docker-compose.yml" && c.new.is_none()));
        assert!(out
            .iter()
            .any(|c| c.path == "presets/demo/myapp2/docker-compose.yml"
                && c.new.as_deref() == Some("services: {}")));

        texts.insert("demo/myapp2/docker-compose.yml".into(), "x".into());
        assert!(changes(&texts, &edit).is_err());
    }

    #[test]
    fn outside_presets_flags_traversal() {
        assert!(outside_presets(["presets/a/preset.yml"]).is_empty());
        assert_eq!(
            outside_presets(["stacks/a/lxc-compose.yml"]),
            vec!["stacks/a/lxc-compose.yml".to_string()]
        );
    }
}
