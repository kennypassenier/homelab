//! fix-108 (help-flat-ids-typo-exit0, 2026-09-27): the command line's help,
//! one entry per verb.
//!
//! `homelab --help` was one flat list of 38 verbs, nearly every line with an
//! internal id and no gloss (`(E1)`, `(D9/B6)`, `(ask-8)`); `export|import
//! <file>` was wrong, `destroy --no-backup` and `release-update [tag]` were
//! missing, `homelab deploy --help` printed the whole list again, and a
//! mistyped verb printed it too and exited 0. The list is grouped the way
//! the README groups it, every verb has its own help with one example, and
//! an unknown verb gets a suggestion and exit code 2.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Daily,
    ChangeAStack,
    Native,
    Host,
    Local,
    Dashboard,
    Destructive,
}

impl Group {
    fn heading(self) -> &'static str {
        match self {
            Group::Daily => "Daily",
            Group::ChangeAStack => "Change a stack",
            Group::Native => "Native services (own programs as systemd units)",
            Group::Host => "Host and fleet",
            Group::Local => "Local, no host needed",
            Group::Dashboard => {
                "Drive the open dashboard (each step plays in every tab that follows)"
            }
            Group::Destructive => "Rare and destructive (each asks you to type the name)",
        }
    }
}

pub struct Verb {
    pub name: &'static str,
    /// What follows the verb, as typed.
    pub args: &'static str,
    pub what: &'static str,
    pub example: &'static str,
    pub group: Group,
}

const fn v(
    group: Group,
    name: &'static str,
    args: &'static str,
    what: &'static str,
    example: &'static str,
) -> Verb {
    Verb {
        name,
        args,
        what,
        example,
        group,
    }
}

use Group::*;

pub const VERBS: &[Verb] = &[
    v(
        Daily,
        "today",
        "[stacks/]",
        "what needs you: doctor, fleet check, incidents and manual checks in one list",
        "homelab today",
    ),
    v(
        Daily,
        "check",
        "[stacks/]",
        "hold the stack files against what runs on the host",
        "homelab check",
    ),
    v(
        Daily,
        "checks",
        "[answer <id>[,<id>,...] ok|nok [note]]",
        "list, or answer (one or several at once, same verdict), the checks only a person can do",
        "homelab checks answer 3f2a9c1e,9c1e3f2a ok",
    ),
    v(
        Daily,
        "answer",
        "[<operation>|<stack>|<question id>] allow|stop",
        "answer a question a running operation is waiting on, from any terminal (the run that raised it may be headless); without a name it answers the one open question, and is refused when more are open",
        "homelab answer deploy-mystack allow",
    ),
    v(
        Daily,
        "status",
        "[--json]",
        "the fleet as the host records it, one line per stack (--json: the raw FleetState)",
        "homelab status",
    ),
    v(
        Daily,
        "doctor",
        "",
        "the host's own diagnosis: state, backups, offsite, disk",
        "homelab doctor",
    ),
    v(
        Daily,
        "incidents",
        "[show <name>]",
        "the bundles failed operations left behind; `show` prints one: the error, the versions and the end of its transcript",
        "homelab incidents show 1790530911-deploy-mystack",
    ),
    v(
        Daily,
        "snapshots",
        "stacks/<name> [--json]",
        "every backup snapshot of a stack's repositories, newest first — the id `homelab restore` takes",
        "homelab snapshots stacks/mystack",
    ),
    v(
        Daily,
        "ping",
        "",
        "is the host there, which version, and where the address came from",
        "homelab ping",
    ),
    v(
        Daily,
        "tui",
        "[--offline]",
        "the terminal interface; --offline runs it against a demo host",
        "homelab tui",
    ),
    v(
        ChangeAStack,
        "plan",
        "stacks/<name>",
        "validate a stack here and print what a deploy would send",
        "homelab plan mystack",
    ),
    v(
        ChangeAStack,
        "deploy",
        "stacks/<name> [--force]",
        "create or reconcile the container; what the files no longer declare is removed, data stays; refused when the host runs a commit this tree does not contain (another deploy would be undone) unless --force",
        "homelab deploy mystack",
    ),
    v(
        ChangeAStack,
        "apply",
        "[stacks/] [--plan] [--dry-run] [--yes] [--no-backup] [--force]",
        "show the per-file plan of every changed stack, ask once, deploy (refused, before anything is sent, when the host runs a commit this tree does not contain, unless --force); a stack whose directory is gone is destroyed only after its name is typed; --plan prints the plan only and exits 0 in sync, 2 with changes pending",
        "homelab apply --plan",
    ),
    v(
        ChangeAStack,
        "backup",
        "stacks/<name>",
        "a restic snapshot of the stack now",
        "homelab backup mystack",
    ),
    v(
        ChangeAStack,
        "restore",
        "stacks/<name> [snapshot] [--app <app>] [--yes] [--no-safety-copy]",
        "restore the stack's data, one night across its repositories, from 'latest' unless a snapshot is named; --app restores one app; asks for the stack name and keeps a copy of the current data",
        "homelab restore mystack latest",
    ),
    v(
        ChangeAStack,
        "update",
        "stacks/<name> [app]",
        "pull and recreate one app or all, with rollback",
        "homelab update mystack myapp",
    ),
    v(
        ChangeAStack,
        "resize",
        "stacks/<name>",
        "apply the manifest's memory, cores and disk to the running container",
        "homelab resize mystack",
    ),
    v(
        ChangeAStack,
        "enable",
        "<stack>",
        "take a stack back into the nightly backup and update, and start-on-boot",
        "homelab enable mystack",
    ),
    v(
        ChangeAStack,
        "disable",
        "<stack>",
        "park a stack: no nightly backup or update, start-on-boot cleared; no container is stopped",
        "homelab disable mystack",
    ),
    v(
        ChangeAStack,
        "new",
        "<name> --preset <p> --vmid <n> [--ram MiB] [--cores N] [--disk GiB] [--swap MiB] [--no-data <path>]",
        "scaffold stacks/<name> from a preset",
        "homelab new newstack --preset custom --vmid 121",
    ),
    v(
        Native,
        "adopt",
        "stacks/<name>",
        "take over a hand-built container described by its service.yml; restarts nothing",
        "homelab adopt mystack",
    ),
    v(
        Native,
        "install-native",
        "stacks/<name>[/<unit>] [<tag> | --file <path>]",
        "install a service's binary from its release, or from a local file",
        "homelab install-native mystack/myunit",
    ),
    v(
        Native,
        "backup-native",
        "<stack>",
        "back up an adopted service now",
        "homelab backup-native mystack",
    ),
    v(
        Native,
        "update-native",
        "<stack>",
        "update an adopted service the way its own update policy says",
        "homelab update-native mystack",
    ),
    v(
        Native,
        "release-update-native",
        "<stack>",
        "install the newest release of each service in the stack",
        "homelab release-update-native mystack",
    ),
    v(
        Native,
        "rollback-native",
        "<stack>[/<unit>]",
        "put a native service's previous binary back after a bad release: runs its health check, swaps on failure and parks the stack's updates",
        "homelab rollback-native mystack/myunit",
    ),
    v(
        Native,
        "restore-native",
        "<stack>[/<unit>] [snapshot] [--yes]",
        "restore a native service's data from a snapshot ('latest' unless named); a unit restores only that one, otherwise every unit of the stack",
        "homelab restore-native mystack/myunit",
    ),
    v(
        Host,
        "config",
        "",
        "the host's nightly hour, notification target and retention tiers",
        "homelab config",
    ),
    v(
        Host,
        "patch",
        "",
        "apt update and dist-upgrade every managed container, one at a time",
        "homelab patch",
    ),
    v(
        Host,
        "guards",
        "<vmid>",
        "apply the runaway guards: log caps, journald limits, logrotate, a weekly prune",
        "homelab guards 104",
    ),
    v(
        Host,
        "exec",
        "<vmid> <command...>",
        "run a command in a container; refused unless exec_enabled = true in host.toml",
        "homelab exec 121 systemctl status myunit",
    ),
    v(
        Host,
        "zfs-replicate",
        "",
        "run the ZFS snapshot and replication jobs now",
        "homelab zfs-replicate",
    ),
    v(
        Host,
        "backup-host-meta",
        "",
        "snapshot the daemon's own state now: vault, state, TLS, intent repository",
        "homelab backup-host-meta",
    ),
    v(
        Host,
        "backup-devices",
        "",
        "fetch each configured device's own configuration now",
        "homelab backup-devices",
    ),
    v(
        Host,
        "templates",
        "",
        "list the golden container templates",
        "homelab templates",
    ),
    v(
        Host,
        "template-build",
        "[vmid] [version] [--privileged] [--base <vztmpl>]",
        "build a golden template on a scratch vmid (999 and version 1 when not given)",
        "homelab template-build 999 5 --base debian-13-standard_13.1-2_amd64.tar.zst",
    ),
    v(
        Host,
        "release-update",
        "[tag]",
        "download the host daemon's release (newest when no tag), verify it, ship it to the host",
        "homelab release-update v3.60.0",
    ),
    v(
        Host,
        "host",
        "restart",
        "restart the host daemon so settings saved to host.toml load; refused while a job runs",
        "homelab host restart",
    ),
    v(
        Host,
        "self-update",
        "<path-to-homelab-host>",
        "ship a locally built host daemon to the host; it rolls back on a failed selfcheck",
        "homelab self-update target/release/homelab-host",
    ),
    v(
        Host,
        "self-install",
        "[tag]",
        "replace this client with a release's (newest when no tag), signature- and checksum-verified",
        "homelab self-install",
    ),
    v(
        Host,
        "token",
        "issue <name> <read|operate|all> | list | revoke <name>",
        "manage per-machine tokens, so one can be revoked without touching the others",
        "homelab token issue laptop operate",
    ),
    v(
        Local,
        "presets",
        "",
        "list the preset catalog",
        "homelab presets",
    ),
    v(
        Local,
        "runbook",
        "[out.md]",
        "write the disaster-recovery runbook from the stacks (docs/DR_RUNBOOK.md by default)",
        "homelab runbook",
    ),
    v(
        Local,
        "testplan",
        "[out.md]",
        "regenerate docs/deployment/TEST_PLAN.md from the test suites",
        "homelab testplan",
    ),
    v(
        Local,
        "update-policy",
        "[doc.md]",
        "regenerate the policy table in docs/deployment/UPDATE_POLICY.md from the stack files",
        "homelab update-policy",
    ),
    v(
        Local,
        "export",
        "stacks/<name> [out.yml]",
        "write the stack definition as one bundle; .env files are never in it",
        "homelab export mystack mystack-bundle.yml",
    ),
    v(
        Local,
        "import",
        "<bundle.yml> <new-name> <vmid>",
        "write a bundle back as a new stack under stacks/, then validate it",
        "homelab import mystack-bundle.yml newstack 121",
    ),
    v(
        Local,
        "help",
        "",
        "this list; `homelab <command> --help` for one command",
        "homelab help",
    ),
    v(
        Dashboard,
        "ui",
        "<step> [--json]",
        "one step in the dashboard: goto <path>, open <action> [stack], type <field> <text>, pick <field> <value>, check <field> on|off, press next|back|confirm, press confirm --wait (the press, then as finish), finish (waits for the open dialog's job to end, prints its outcome, then closes the dialog and hands the dashboard back at once), answer [operation] allow|stop (the banner's Allow/Stop for a running operation's question), close, state, done, plan \"<step>\" … (the whole sequence up front); answers what is on screen once the step ran (a tab in Live view announces each step with a 3 s countdown, and a viewer may pause or stop it), and the final press runs the action once on the dashboard's side",
        "homelab ui open deploy mystack",
    ),
    v(
        Destructive,
        "destroy",
        "stacks/<name> [--no-backup] [--yes]",
        "back up, then destroy the container; works from the host's record when the directory is gone; --yes skips typing the name (the dashboard's copied line carries it once the name was typed there)",
        "homelab destroy oldstack",
    ),
    v(
        Destructive,
        "forget",
        "<stack>",
        "for a container already gone: drop its record and registrations; touches no container",
        "homelab forget oldstack",
    ),
    v(
        Destructive,
        "wipe",
        "<stack>[/<app>] [--yes]",
        "delete what a retired stack or app kept: backups, /appdata, vault copies; --yes skips typing the name",
        "homelab wipe oldstack",
    ),
    v(
        Destructive,
        "prune-orphans",
        "stacks/<name> [--yes]",
        "remove files the repository dropped, without a deploy (a deploy does this itself); --yes skips typing the name",
        "homelab prune-orphans mystack",
    ),
];

fn line(verb: &Verb) -> String {
    let call = if verb.args.is_empty() {
        format!("homelab {}", verb.name)
    } else {
        format!("homelab {} {}", verb.name, verb.args)
    };
    if call.chars().count() > 44 {
        format!("  {}\n  {:<44} {}", call, "", verb.what)
    } else {
        format!("  {:<44} {}", call, verb.what)
    }
}

/// The whole list, grouped.
pub fn usage() -> String {
    let mut out = format!(
        "homelab v{} — usage: homelab <command> [arguments]\n\
         `homelab <command> --help` shows one command with an example. A stack is \
         named mystack or stacks/mystack, from any directory.\n",
        env!("CARGO_PKG_VERSION")
    );
    for group in [
        Daily,
        ChangeAStack,
        Native,
        Host,
        Local,
        Dashboard,
        Destructive,
    ] {
        out.push_str(&format!("\n{}\n", group.heading()));
        for verb in VERBS.iter().filter(|v| v.group == group) {
            out.push_str(&line(verb));
            out.push('\n');
        }
    }
    out.push_str(
        "\nenv: HOMELAB_TOKEN and HOMELAB_HOST (default 10.10.5.250:8443), from the \
         environment or ~/.config/homelab/env; HOMELAB_REPO names the repository when \
         the command runs outside it\n\
         cert pin: ~/.config/homelab/pin\n\
         --answer allow|stop: answers any question the command's operation raises, \
         without waiting; on a terminal with no --answer, the same question \
         is asked here as [a]/[s]\n",
    );
    out
}

/// One verb's help: what it takes, what it does, one example.
pub fn verb_help(name: &str) -> Option<String> {
    let verb = VERBS.iter().find(|v| v.name == name)?;
    Some(format!(
        "{}\n  {}\n  example: {}\n",
        if verb.args.is_empty() {
            format!("homelab {}", verb.name)
        } else {
            format!("homelab {} {}", verb.name, verb.args)
        },
        verb.what,
        verb.example
    ))
}

pub fn is_verb(name: &str) -> bool {
    VERBS.iter().any(|v| v.name == name)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The verb a mistyped one most likely meant, when one is close enough.
pub fn suggest(typed: &str) -> Option<&'static str> {
    VERBS
        .iter()
        .map(|v| (distance(typed, v.name), v.name))
        .filter(|(d, name)| *d <= 2 && *d * 2 < name.len().max(typed.len()))
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| name)
}
