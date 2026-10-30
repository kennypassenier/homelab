//! homelab-client library: the TUI (AR6) and TLS pinning, shared by the
//! `homelab` binary and the test suite.

pub mod answer;
pub mod apply;
pub mod checks;
pub mod cli_args;
pub mod cli_help;

pub mod edge;
pub mod envdrift;
pub mod link;
pub mod netredact;
pub mod output;
pub mod pinexists;
pub mod release;
pub mod repo_config;
pub mod routes;
pub mod scaffold;
pub mod snapshots;
pub mod spec;
pub mod status;
pub mod testplan;
pub mod tls;
#[cfg(feature = "tui")]
pub mod tui;
pub mod ui_catalog;
pub mod ui_cli;
pub mod ui_drive;
pub mod ui_preflight;
pub mod updatepolicy;
pub mod version;

use std::path::PathBuf;

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): `git
/// describe --dirty` of the tree this client was built from (build.rs), or
/// `unknown` outside a git tree. Printed by `ping` and `status` and sent
/// with every deploy.
pub const BUILD: &str = env!("HOMELAB_BUILD");

/// Path where the pinned TLS fingerprint is stored (A4, TOFU).
pub fn pin_path() -> PathBuf {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".config/homelab/pin")
}

pub fn load_pin() -> Option<String> {
    std::fs::read_to_string(pin_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_pin(fp: &str) {
    let path = pin_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, fp);
}

/// restore-two-sources-1 (Kenny, 2026-10-22: "ik wil ook zelf kunnen kiezen
/// welke van de twee terug wordt gezet"): `--source local|remote` taken out
/// of the command line before any verb's own parser sees it, so a parser
/// that keeps unknown words as positionals never reads `local` as a
/// snapshot. Absent: None, which the host reads as remote.
pub fn take_source(args: &[String]) -> Result<(Vec<String>, Option<String>), String> {
    let mut out = Vec::with_capacity(args.len());
    let mut source = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--source" {
            match it.next().map(String::as_str) {
                Some(v @ ("local" | "remote")) => source = Some(v.to_string()),
                _ => {
                    return Err(
                        "--source takes local (the copy on the host's own disks) or remote \
                         (Google Drive)"
                            .into(),
                    );
                }
            }
        } else {
            out.push(a.clone());
        }
    }
    Ok((out, source))
}

/// What `homelab restore` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreArgs {
    pub dir: String,
    pub snapshot: String,
    pub app: Option<String>,
    pub yes: bool,
    pub skip_safety_copy: bool,
}

/// The arguments after `homelab restore`. Flags may stand anywhere (fix-64);
/// `--app <name>` takes the word after it (fix-112, restore-stale-files-
/// mixed-nights, 2026-09-27: one app restored without its neighbours); what
/// remains is the stack directory, then the snapshot (default `latest`).
pub fn restore_args(rest: &[String]) -> Result<RestoreArgs, String> {
    const USAGE: &str = "usage: homelab restore stacks/<name> [snapshot] [--app <name>] \
                         [--source local|remote] [--yes] [--no-safety-copy]";
    let mut positional: Vec<String> = Vec::new();
    let mut app = None;
    let mut yes = false;
    let mut skip_safety_copy = false;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yes" => yes = true,
            "--no-safety-copy" => skip_safety_copy = true,
            "--app" => match it.next() {
                Some(name) if !name.starts_with("--") && !name.is_empty() => {
                    app = Some(name.clone())
                }
                _ => return Err(format!("--app needs an app name :: {}", USAGE)),
            },
            other if other.starts_with("--") => {}
            other => positional.push(other.to_string()),
        }
    }
    let mut pos = positional.into_iter();
    let dir = pos.next().ok_or_else(|| USAGE.to_string())?;
    Ok(RestoreArgs {
        dir,
        snapshot: pos.next().unwrap_or_else(|| "latest".into()),
        app,
        yes,
        skip_safety_copy,
    })
}

/// CLI exit rule for RPCs whose payload arrives as a separate broadcast
/// frame (GetConfig): RpcDone alone is not enough — the payload frame can
/// lose the race and would be dropped by an early exit. Regression guard
/// for the Config-race bug (fixed 2026-08-11).
pub fn rpc_can_exit(awaits_payload: bool, payload_seen: bool, rpc_done_ok: bool) -> bool {
    rpc_done_ok && (!awaits_payload || payload_seen)
}

/// fix-114 (native-rollback-copies-deleted, 2026-09-27): `<stack>/<unit>`
/// or `<stack>` for `homelab rollback-native`; a leading `stacks/` and a
/// trailing slash are accepted, so the path to the stack directory works.
pub fn stack_and_unit(arg: &str) -> (String, Option<String>) {
    let trimmed = arg.trim().trim_end_matches('/');
    let rest = trimmed.strip_prefix("stacks/").unwrap_or(trimmed);
    match rest.split_once('/') {
        Some((stack, unit)) if !unit.is_empty() => (stack.to_string(), Some(unit.to_string())),
        _ => (rest.to_string(), None),
    }
}
