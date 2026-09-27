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
    v(Daily, "today", "[stacks/]", "what needs you: doctor, fleet check, incidents and manual checks in one list", "homelab today"),
    v(Daily, "check", "[stacks/]", "hold the stack files against what runs on the host", "homelab check"),
    v(Daily, "checks", "[answer <id> ok|nok [note]]", "list, or answer, the checks only a person can do", "homelab checks answer 3f2a9c1e ok"),
    v(Daily, "status", "", "the fleet as the host records it", "homelab status"),
    v(Daily, "doctor", "", "the host's own diagnosis: state, backups, offsite, disk", "homelab doctor"),
    v(Daily, "incidents", "", "the bundles failed operations left behind", "homelab incidents"),
    v(Daily, "ping", "", "is the host there, which version, and where the address came from", "homelab ping"),
    v(Daily, "tui", "[--offline]", "the terminal interface; --offline runs it against a demo host", "homelab tui"),
    v(ChangeAStack, "plan", "stacks/<name>", "validate a stack here and print what a deploy would send", "homelab plan almanac"),
    v(ChangeAStack, "deploy", "stacks/<name>", "create or reconcile the container; what the files no longer declare is removed, data stays", "homelab deploy almanac"),
    v(ChangeAStack, "apply", "[stacks/] [--dry-run] [--yes] [--no-backup]", "show the per-file plan of every changed stack, ask once, deploy; a stack whose directory is gone is destroyed only after its name is typed", "homelab apply --dry-run"),
    v(ChangeAStack, "backup", "stacks/<name>", "a restic snapshot of the stack now", "homelab backup almanac"),
    v(ChangeAStack, "restore", "stacks/<name> [snapshot]", "restore the stack's data, from 'latest' unless a snapshot is named", "homelab restore almanac latest"),
    v(ChangeAStack, "update", "stacks/<name> [app]", "pull and recreate one app or all, with rollback", "homelab update media jellyfin"),
    v(ChangeAStack, "resize", "stacks/<name>", "apply the manifest's memory, cores and disk to the running container", "homelab resize media"),
    v(ChangeAStack, "enable", "<stack>", "take a stack back into the nightly backup and update, and start-on-boot", "homelab enable media"),
    v(ChangeAStack, "disable", "<stack>", "park a stack: no nightly backup or update, start-on-boot cleared; no container is stopped", "homelab disable media"),
    v(ChangeAStack, "new", "<name> --preset <p> --vmid <n> [--ram MiB] [--cores N] [--disk GiB] [--swap MiB] [--no-data <path>]", "scaffold stacks/<name> from a preset", "homelab new notes --preset syncthing --vmid 120"),
    v(Native, "adopt", "stacks/<name>", "take over a hand-built container described by its service.yml; restarts nothing", "homelab adopt almanac"),
    v(Native, "install-native", "stacks/<name>[/<unit>] [<tag> | --file <path>]", "install a service's binary from its release, or from a local file", "homelab install-native kyu/kyu-runner"),
    v(Native, "backup-native", "<stack>", "back up an adopted service now", "homelab backup-native kyu"),
    v(Native, "update-native", "<stack>", "update an adopted service the way its own update policy says", "homelab update-native kyu"),
    v(Native, "release-update-native", "<stack>", "install the newest release of each service in the stack", "homelab release-update-native almanac"),
    v(Host, "config", "", "the host's nightly hour, notification target and retention tiers", "homelab config"),
    v(Host, "patch", "", "apt update and dist-upgrade every managed container, one at a time", "homelab patch"),
    v(Host, "guards", "<vmid>", "apply the runaway guards: log caps, journald limits, logrotate, a weekly prune", "homelab guards 104"),
    v(Host, "exec", "<vmid> <command...>", "run a command in a container; refused unless exec_enabled = true in host.toml", "homelab exec 109 systemctl status kyu"),
    v(Host, "zfs-replicate", "", "run the ZFS snapshot and replication jobs now", "homelab zfs-replicate"),
    v(Host, "backup-host-meta", "", "snapshot the daemon's own state now: vault, state, TLS, intent repository", "homelab backup-host-meta"),
    v(Host, "backup-devices", "", "fetch each configured device's own configuration now", "homelab backup-devices"),
    v(Host, "templates", "", "list the golden container templates", "homelab templates"),
    v(Host, "template-build", "[vmid] [version] [--privileged] [--base <vztmpl>]", "build a golden template on a scratch vmid (999 and version 1 when not given)", "homelab template-build 999 5 --base debian-13-standard_13.1-2_amd64.tar.zst"),
    v(Host, "release-update", "[tag]", "download the host daemon's release (newest when no tag), verify it, ship it to the host", "homelab release-update v3.60.0"),
    v(Host, "self-update", "<path-to-homelab-host>", "ship a locally built host daemon to the host; it rolls back on a failed selfcheck", "homelab self-update target/release/homelab-host"),
    v(Host, "self-install", "[tag]", "replace this client with a release's (newest when no tag), checksum-verified", "homelab self-install"),
    v(Local, "presets", "", "list the preset catalog", "homelab presets"),
    v(Local, "runbook", "[out.md]", "write the disaster-recovery runbook from the stacks (docs/DR_RUNBOOK.md by default)", "homelab runbook"),
    v(Local, "testplan", "[out.md]", "regenerate docs/deployment/TEST_PLAN.md from the test suites", "homelab testplan"),
    v(Local, "dashboard", "<stack> <app>...", "print a stack's Grafana dashboard, the one a deploy writes", "homelab dashboard media jellyfin"),
    v(Local, "export", "stacks/<name> [out.yml]", "write the stack definition as one bundle; .env files are never in it", "homelab export almanac almanac-bundle.yml"),
    v(Local, "import", "<bundle.yml> <new-name> <vmid>", "write a bundle back as a new stack under stacks/, then validate it", "homelab import almanac-bundle.yml calendar 121"),
    v(Local, "help", "", "this list; `homelab <command> --help` for one command", "homelab help"),
    v(Destructive, "destroy", "stacks/<name> [--no-backup]", "back up, then destroy the container; works from the host's record when the directory is gone", "homelab destroy drill"),
    v(Destructive, "forget", "<stack>", "for a container already gone: drop its record and registrations; touches no container", "homelab forget drill"),
    v(Destructive, "wipe", "<stack>[/<app>]", "delete what a retired stack or app kept: backups, /appdata, vault copies", "homelab wipe drill"),
    v(Destructive, "prune-orphans", "stacks/<name>", "remove files the repository dropped, without a deploy (a deploy does this itself)", "homelab prune-orphans media"),
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
         named almanac or stacks/almanac, from any directory.\n",
        env!("CARGO_PKG_VERSION")
    );
    for group in [Daily, ChangeAStack, Native, Host, Local, Destructive] {
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
         cert pin: ~/.config/homelab/pin\n",
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
