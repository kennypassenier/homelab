//! fix-107 (see REGISTER.md): every key of the TUI in one table, which the
//! key map (`h`), the footer and the palette are all drawn from. What a key
//! DOES is still decided in `model::on_key`; the palette's ids are checked
//! against that dispatcher by `h19_every_palette_action_reaches_a_real_handler`.

use crate::tui::model::Tab;

/// Where a key works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every tab.
    Everywhere,
    /// Every tab but SHELL, where typing owns the keyboard.
    AllButShell,
    /// DASHBOARD and STACKS: the keys that act on the selected stack.
    Stacks,
    Logs,
    Doctor,
    Shell,
}

pub struct Binding {
    /// Exactly as pressed: lowercase for a plain key, SHIFT+ for a capital.
    /// Empty for an action only the palette offers.
    pub key: &'static str,
    /// What it does; the key map shows it and the palette lists it.
    pub what: &'static str,
    pub scope: Scope,
    /// The footer's short form, when the footer shows it.
    pub footer: Option<&'static str>,
    /// The palette's id for it, when the palette offers it.
    pub action: Option<&'static str>,
}

impl Binding {
    /// Does this key work on `tab`?
    pub fn shown_on(&self, tab: Tab) -> bool {
        match self.scope {
            Scope::Everywhere => true,
            Scope::AllButShell => tab != Tab::Shell,
            Scope::Stacks => matches!(tab, Tab::Dashboard | Tab::Stacks),
            Scope::Logs => tab == Tab::Logs,
            Scope::Doctor => tab == Tab::Doctor,
            Scope::Shell => tab == Tab::Shell,
        }
    }
}

const fn b(
    key: &'static str,
    what: &'static str,
    scope: Scope,
    footer: Option<&'static str>,
    action: Option<&'static str>,
) -> Binding {
    Binding {
        key,
        what,
        scope,
        footer,
        action,
    }
}

/// Table order is footer order: the tab keys first, the tab's own keys,
/// then palette, help and quit.
pub const KEYMAP: &[Binding] = &[
    b(
        "1-5/TAB",
        "switch tab (AZERTY: & é \" ' (; SHIFT+TAB goes back)",
        Scope::AllButShell,
        Some("tabs"),
        None,
    ),
    b(
        "TAB",
        "switch tab (digits type here)",
        Scope::Shell,
        Some("tabs"),
        None,
    ),
    // DASHBOARD and STACKS.
    b("UP/DOWN", "move the selection", Scope::Stacks, None, None),
    b(
        "n",
        "new stack: the wizard",
        Scope::Stacks,
        Some("new stack"),
        Some("op.new"),
    ),
    b(
        "p",
        "plan: what a deploy of the selected stack would change",
        Scope::Stacks,
        Some("plan"),
        Some("op.plan"),
    ),
    b(
        "SHIFT+D",
        "deploy: selected stack",
        Scope::Stacks,
        Some("deploy"),
        Some("op.deploy"),
    ),
    b(
        "r",
        "refresh fleet state and what needs you",
        Scope::Stacks,
        Some("refresh"),
        Some("refresh"),
    ),
    b(
        "SHIFT+B",
        "backup: selected stack",
        Scope::Stacks,
        None,
        Some("op.backup"),
    ),
    b(
        "SHIFT+U",
        "update: pull the images of the selected stack",
        Scope::Stacks,
        None,
        Some("op.update"),
    ),
    b(
        "SHIFT+R",
        "restore: selected stack (asks for the name first)",
        Scope::Stacks,
        None,
        Some("op.restore"),
    ),
    b(
        "g",
        "guards: apply to selected stack",
        Scope::Stacks,
        None,
        Some("op.guards"),
    ),
    b(
        "SHIFT+A",
        "adopt: native services of selected stack",
        Scope::Stacks,
        None,
        Some("op.adopt"),
    ),
    b(
        "SHIFT+I",
        "install-native: binaries of selected stack",
        Scope::Stacks,
        None,
        Some("op.install-native"),
    ),
    b(
        "e",
        "park or unpark: selected stack for the nightly run (asks first)",
        Scope::Stacks,
        None,
        Some("op.park"),
    ),
    b(
        "c",
        "fleet check: repository against reality",
        Scope::Stacks,
        None,
        Some("op.check"),
    ),
    b(
        "i",
        "incidents: list captured bundles",
        Scope::Stacks,
        None,
        Some("op.incidents"),
    ),
    // fix-102 (tui-single-keys-no-confirm, 2026-09-27): `u` sat one Shift
    // from SHIFT+U and started a host self-update with no question asked.
    // The command palette ("op.host-update") is the only way to reach it
    // now, so there is one fewer key with consequences this large.
    b(
        "",
        "host update, when a newer release is offered (asks first)",
        Scope::Stacks,
        None,
        Some("op.host-update"),
    ),
    // fix-107/fix-66 (apply-in-the-tui): `homelab apply`'s deploy half,
    // palette-only like host update — deploying every changed stack in the
    // repository at once is not a thing to reach for by reflex.
    b(
        "",
        "apply: deploy every stack whose files changed (asks first; destroy stays CLI-only)",
        Scope::Stacks,
        None,
        Some("op.apply"),
    ),
    // fix-64 (restore-no-confirm-no-safety-snapshot): a read, not one of
    // the six stack operations with keys — palette-only, same as `homelab
    // snapshots` on the command line.
    b(
        "",
        "snapshots: every backup of the selected stack's repositories",
        Scope::Stacks,
        None,
        Some("op.snapshots"),
    ),
    // LOG_STREAM.
    b(
        "LEFT/RIGHT",
        "log source",
        Scope::Logs,
        Some("source"),
        None,
    ),
    b("UP/DOWN", "scroll", Scope::Logs, Some("scroll"), None),
    b("SPACE", "follow on/off", Scope::Logs, Some("follow"), None),
    b(
        "SHIFT+G",
        "back to the tail",
        Scope::Logs,
        Some("tail"),
        None,
    ),
    // DOCTOR.
    b(
        "r/ENTER",
        "doctor: run it again",
        Scope::Doctor,
        Some("re-run"),
        Some("doctor"),
    ),
    // SHELL.
    b(
        "type+ENTER",
        "run the line in the target container",
        Scope::Shell,
        Some("run"),
        None,
    ),
    b(
        "LEFT/RIGHT",
        "target container (empty input)",
        Scope::Shell,
        Some("target (empty input)"),
        None,
    ),
    b(
        "UP",
        "recall the last line",
        Scope::Shell,
        Some("recall"),
        None,
    ),
    // Everywhere.
    b(
        "CTRL+K",
        "command palette (CTRL+P too)",
        Scope::Everywhere,
        Some("palette"),
        None,
    ),
    b(
        "F2",
        "cycle effects, kept for the next launch",
        Scope::Everywhere,
        None,
        Some("fx"),
    ),
    b(
        "h",
        "this key map",
        Scope::AllButShell,
        Some("help"),
        Some("help"),
    ),
    b(
        "q",
        "quit (asks when something would be lost)",
        Scope::AllButShell,
        Some("quit"),
        Some("quit"),
    ),
];

/// The footer of `tab`: (key, short form), in table order.
pub fn footer(tab: Tab) -> Vec<(&'static str, &'static str)> {
    KEYMAP
        .iter()
        .filter(|b| b.shown_on(tab))
        .filter_map(|b| b.footer.map(|f| (b.key, f)))
        .collect()
}

pub struct PaletteAction {
    /// What it does and, when it has one, its key.
    pub label: String,
    pub id: &'static str,
}

fn tab_id(t: Tab) -> &'static str {
    match t {
        Tab::Dashboard => "tab.dashboard",
        Tab::Stacks => "tab.stacks",
        Tab::Logs => "tab.logs",
        Tab::Doctor => "tab.doctor",
        Tab::Shell => "tab.shell",
    }
}

/// Every action the palette offers: one per tab, then every key that has an
/// action, with the key it has.
pub fn palette() -> Vec<PaletteAction> {
    let mut out: Vec<PaletteAction> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| PaletteAction {
            label: format!("go: {} [{}]", t.title().to_lowercase(), i + 1),
            id: tab_id(*t),
        })
        .collect();
    for b in KEYMAP {
        let Some(id) = b.action else { continue };
        let label = if b.key.is_empty() {
            b.what.to_string()
        } else {
            format!("{} [{}]", b.what, b.key)
        };
        out.push(PaletteAction { label, id });
    }
    out
}
