//! Edit forms: the edit forms Claude can open.

use super::*;

/// The edit forms Claude can open, besides the actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditKind {
    Settings,
    Raw,
    AddApp,
    Firewall,
    NewStack,
    HostSettings,
    Batch,
    Rollback,
    /// TUI parity (`homelab import`): a bundle as a new stack.
    Import,
    /// feat-native-1: one native unit's `service.yml` fields. Target
    /// `<stack>/<unit>` (`<stack>` alone picks its first native unit).
    Native,
    /// feat-native-1: a new native unit.
    AddNative,
    /// feat-preset-1: an existing preset's `preset.yml` and files. Target
    /// the preset's name.
    Preset,
    /// feat-preset-1: a new preset (name typed in the form itself).
    NewPreset,
    /// feat-stacks-9: network, lxc flags, resources.storage, on_demand,
    /// retention — the settings form's second page.
    SettingsExt,
    /// feat-stacks-10: the Apps tab — remove/add-blank an app, storage,
    /// data_mounts, log_files.
    Apps,
    /// feat-stacks-11: `latch_secrets` and `latch_files`.
    Latch,
    /// feat-checks-1: one app's whole `checks.yml`. Target `<stack>/<app>`.
    Checks,
    /// feat-tiles-1: the stack's `tiles:` map, created/renamed/removed.
    Tiles,
    /// feat-publish-1: one app's route + optional tile. Target
    /// `<stack>/<app>`.
    PublishApp,
}

impl EditKind {
    pub const ALL: [EditKind; 19] = [
        EditKind::Settings,
        EditKind::Raw,
        EditKind::AddApp,
        EditKind::Firewall,
        EditKind::NewStack,
        EditKind::HostSettings,
        EditKind::Batch,
        EditKind::Rollback,
        EditKind::Import,
        EditKind::Native,
        EditKind::AddNative,
        EditKind::Preset,
        EditKind::NewPreset,
        EditKind::SettingsExt,
        EditKind::Apps,
        EditKind::Latch,
        EditKind::Checks,
        EditKind::Tiles,
        EditKind::PublishApp,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            EditKind::Settings => "settings",
            EditKind::Raw => "raw",
            EditKind::AddApp => "add-app",
            EditKind::Firewall => "firewall",
            EditKind::NewStack => "new-stack",
            EditKind::HostSettings => "host-settings",
            EditKind::Batch => "batch",
            EditKind::Rollback => "rollback",
            EditKind::Import => "import",
            EditKind::Native => "native",
            EditKind::AddNative => "add-native",
            EditKind::Preset => "preset",
            EditKind::NewPreset => "new-preset",
            EditKind::SettingsExt => "settings-ext",
            EditKind::Apps => "apps",
            EditKind::Latch => "latch",
            EditKind::Checks => "checks",
            EditKind::Tiles => "tiles",
            EditKind::PublishApp => "publish",
        }
    }

    /// How `homelab ui open` names it.
    pub fn usage(self) -> &'static str {
        match self {
            EditKind::Settings => "settings <stack>",
            EditKind::Raw => "raw <stack>",
            EditKind::AddApp => "add-app <stack>",
            EditKind::Firewall => "firewall <stack>",
            EditKind::NewStack => "new-stack",
            EditKind::HostSettings => "host-settings",
            EditKind::Batch => "batch <action> <stack>,<stack>",
            EditKind::Rollback => "rollback <stack>",
            EditKind::Import => "import",
            EditKind::Native => "native <stack>[/<unit>]",
            EditKind::AddNative => "add-native <stack>",
            EditKind::Preset => "preset <name>",
            EditKind::NewPreset => "new-preset",
            EditKind::SettingsExt => "settings-ext <stack>",
            EditKind::Apps => "apps <stack>",
            EditKind::Latch => "latch <stack>",
            EditKind::Checks => "checks <stack>/<app>",
            EditKind::Tiles => "tiles <stack>",
            EditKind::PublishApp => "publish <stack>/<app>",
        }
    }

    /// `batch:<action>` is the batch form of that action.
    pub fn from_form(form: &str) -> Option<EditKind> {
        if form == "batch" || form.starts_with("batch:") {
            return Some(EditKind::Batch);
        }
        EditKind::ALL.iter().copied().find(|k| k.slug() == form)
    }

    /// A stack's own edit, ending in the plan and the commit.
    pub fn stack_edit(self) -> bool {
        matches!(
            self,
            EditKind::Settings
                | EditKind::Raw
                | EditKind::AddApp
                | EditKind::Firewall
                | EditKind::Native
                | EditKind::AddNative
                | EditKind::SettingsExt
                | EditKind::Apps
                | EditKind::Latch
                | EditKind::Checks
                | EditKind::Tiles
                | EditKind::PublishApp
        )
    }

    /// Storage/data_mounts/log_files/latch_files: the row dialogs Apps
    /// and Latch share (`Sub.kind` is the list's own name for these).
    pub fn row_list_kind(name: &str) -> bool {
        matches!(
            name,
            "storage"
                | "data_mounts"
                | "log_files"
                | "latch_files"
                | "checks"
                | "manual"
                | "probes"
                | "tiles"
                | "retention"
        )
    }

    /// A preset edit, ending in its own plan and commit
    /// (`/data/presets/plan`, `/data/presets/commit` — not a stack's).
    pub fn preset_edit(self) -> bool {
        matches!(self, EditKind::Preset | EditKind::NewPreset)
    }

    /// The scope a driving token needs to open it. host.toml can take the
    /// backups or the dashboard's route down: `all` (Kenny may lower it).
    pub fn scope(self) -> Scope {
        match self {
            EditKind::HostSettings => Scope::All,
            _ => Scope::Operate,
        }
    }
}
