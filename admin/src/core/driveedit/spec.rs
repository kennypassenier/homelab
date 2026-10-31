//! Edit forms: the description of the edit forms (`formspec.json` → `edit`).

use super::*;

/// One field of an edit form in `formspec.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct EditFieldDef {
    pub name: String,
    pub id: String,
    pub kind: FieldKind,
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub choices: Option<Vec<Choice>>,
    #[serde(default)]
    pub current: Option<Value>,
}

impl EditFieldDef {
    pub fn field(&self) -> Field {
        Field {
            name: self.name.clone(),
            id: self.id.clone(),
            kind: self.kind,
            label: self.label.clone(),
            help: self.help.clone(),
            required: self.required,
            pattern: self.pattern.clone(),
            placeholder: self.placeholder.clone(),
            source: None,
            empty: None,
            danger: false,
            when: None,
            expect: None,
            min: self.min,
            max: self.max,
            choices: self.choices.clone(),
            current: self.current.clone(),
            show_when: None,
            change_when: None,
        }
    }

    /// With `{word}`s filled in (an image's key, a data folder's path).
    pub(super) fn filled(&self, words: &[(&str, &str)]) -> Field {
        let mut f = self.field();
        f.name = fill(&f.name, words);
        f.id = fill(&f.id, words);
        f.label = fill(&f.label, words);
        f.help = fill(&f.help, words);
        f
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EditStepDef {
    pub id: String,
    pub label: String,
    pub fields: Vec<EditFieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewDefaults {
    pub ram_mb: u64,
    pub cores: u64,
    pub disk_gb: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostKeyIds {
    pub id: String,
    pub confirm_id: String,
    pub confirm_label: String,
}

/// `formspec.json` → `edit`.
#[derive(Debug, Clone, Deserialize)]
pub struct EditSpec {
    pub messages: BTreeMap<String, String>,
    pub no_touch: Vec<i64>,
    pub follow: BTreeMap<String, String>,
    pub settings: Vec<EditFieldDef>,
    pub image: EditFieldDef,
    pub tile_watch: EditFieldDef,
    pub tile_down: EditFieldDef,
    pub commit: Vec<EditFieldDef>,
    pub raw: Vec<EditFieldDef>,
    pub add_app: Vec<EditFieldDef>,
    /// feat-native-1: the native-service form (one unit's `service.yml`).
    pub native: Vec<EditFieldDef>,
    /// feat-native-1: "add a native unit".
    pub add_native: Vec<EditFieldDef>,
    pub firewall: Vec<EditFieldDef>,
    pub rule: Vec<EditFieldDef>,
    pub new_stack: Vec<EditStepDef>,
    pub new_defaults: NewDefaults,
    pub nodata: EditFieldDef,
    pub host_key: HostKeyIds,
    pub rollback: Vec<EditFieldDef>,
    /// TUI parity: the import form's first step (the bundle, the name, the
    /// container number).
    pub import: Vec<EditFieldDef>,
    /// feat-preset-1: `preset.yml`'s own fields, shared by `preset` (an
    /// existing one) and `new-preset`.
    pub preset_meta: Vec<EditFieldDef>,
    /// feat-preset-1: `new-preset`'s own first field, ahead of
    /// `preset_meta`.
    pub new_preset_name: EditFieldDef,
    /// feat-preset-1: the Files card's own fields, on the same "meta" step.
    pub preset_file: Vec<EditFieldDef>,
    /// feat-stacks-9: the settings form's second page.
    pub settings_ext: Vec<EditFieldDef>,
    /// feat-stacks-10: one app's "remove" checkbox / the blank-app field.
    pub apps_remove: EditFieldDef,
    pub apps_add_blank: EditFieldDef,
    /// feat-tiles-3: one app's optional tile hostname, on the add-app step.
    pub add_app_tile: EditFieldDef,
    pub storage_entry: Vec<EditFieldDef>,
    pub data_mount: Vec<EditFieldDef>,
    pub log_file: Vec<EditFieldDef>,
    /// W2: the settings-ext form's retention row dialog.
    pub retention_tier: Vec<EditFieldDef>,
    /// feat-stacks-11: one app's `latch_secrets` checkbox.
    pub latch_secret: EditFieldDef,
    /// gap 74 (13c7353a): one native unit's place in the restart order.
    pub latch_order: EditFieldDef,
    pub latch_file: Vec<EditFieldDef>,
    /// feat-checks-1: the checks form's row dialogs and its scalar fields
    /// (which app, the busy check, the link).
    pub check_row: Vec<EditFieldDef>,
    pub probe_row: Vec<EditFieldDef>,
    pub manual_row: Vec<EditFieldDef>,
    pub checks_scalar: Vec<EditFieldDef>,
    /// feat-tiles-1: one tile's row dialog.
    pub tile_row: Vec<EditFieldDef>,
    /// feat-publish-1: the publish-app dialog.
    pub publish: Vec<EditFieldDef>,
}

pub(super) fn es() -> &'static EditSpec {
    &spec().edit
}

/// A message of the edit forms, `{word}`s filled in.
pub fn say(key: &str, words: &[(&str, &str)]) -> String {
    fill(
        es().messages.get(key).map(String::as_str).unwrap_or(key),
        words,
    )
}
