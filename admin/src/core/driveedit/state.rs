//! Edit forms: the edit form's own state and the calls the shell runs.

use super::*;

/// A dialog on top of the form: the firewall's rule dialog, a host.toml
/// key's dialog.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sub {
    /// `rule` or `key`.
    pub kind: String,
    pub title: String,
    /// The rule's index (from 0) or the key; null for a new rule.
    pub target: Value,
    #[serde(skip)]
    pub fields: Vec<Field>,
    pub values: Values,
    pub errors: BTreeMap<String, String>,
}

/// The edit form's own state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditState {
    pub family: EditKind,
    /// The firewall as the editor holds it (`firewallModel`'s shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Value>,
    /// The table in words: the firewall's rules, host.toml's keys.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<Sub>,
    /// The plan, in short.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Value>,
    /// What the final press answered: the commit, host.toml, the batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// host.toml: the changes kept, by key, and the keys whose name was
    /// typed.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub staged: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub confirmed: BTreeSet<String>,
    /// The batch: the action and its stacks, and how many guards refuse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_action: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stacks: Vec<String>,
    pub guarded: usize,
    /// feat-preset-1: which named button built the current `body`, when
    /// it was not the form's own "next"/review (`save-file`,
    /// `rename-file`, `delete-file`, `remove-preset`) — the browser's
    /// `sync` clicks that DOM button to open the real plan dialog, the
    /// same way it always clicked "review" before there was more than one
    /// button to choose from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The edit the plan was made for; the commit sends the same.
    #[serde(skip)]
    pub body: Option<Value>,
    /// What `open` read.
    #[serde(skip)]
    pub data: Value,
}

impl EditState {
    pub(super) fn new(family: EditKind, data: Value) -> Self {
        EditState {
            family,
            model: None,
            rows: Vec::new(),
            sub: None,
            plan: None,
            result: None,
            staged: BTreeMap::new(),
            confirmed: BTreeSet::new(),
            batch_action: None,
            stacks: Vec::new(),
            guarded: 0,
            action: None,
            body: None,
            data,
        }
    }
}

/// What the shell runs for an edit form, through the same functions the
/// routes a click reaches run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCall {
    /// `POST /data/stacks/{stack}/plan`.
    Plan { stack: String, edit: Value },
    /// `POST /data/stacks/{stack}/commit`: the final press.
    Commit { stack: String, body: Value },
    /// `POST /data/stacks-new/appdata`.
    Appdata { body: Value },
    /// `POST /data/stacks-new/plan`.
    NewPlan { body: Value },
    /// `POST /data/stacks-new/commit`: the final press.
    NewCommit { body: Value },
    /// `PUT /data/host-settings`: the final press.
    HostWrite { body: Value },
    /// `POST /data/actions/batch`: the final press.
    Batch { body: Value },
    /// `POST /data/stacks-import/plan`.
    ImportPlan { body: Value },
    /// `POST /data/stacks-import/commit`: the final press.
    ImportCommit { body: Value },
    /// `POST /data/presets/plan` (feat-preset-1: `preset` and `new-preset`
    /// share this — a preset is not under any stack).
    PresetPlan { edit: Value },
    /// `POST /data/presets/commit`: the final press.
    PresetCommit { body: Value },
}

impl EditCall {
    /// Whether this is a form's final press.
    pub fn final_press(&self) -> bool {
        matches!(
            self,
            EditCall::Commit { .. }
                | EditCall::NewCommit { .. }
                | EditCall::HostWrite { .. }
                | EditCall::Batch { .. }
                | EditCall::ImportCommit { .. }
                | EditCall::PresetCommit { .. }
        )
    }
}
