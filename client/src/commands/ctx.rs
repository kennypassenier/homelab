//! Shared command context: colours, process-wide settings, repository paths and env-file loading.

use super::*;

pub(crate) const C_RESET: Paint = Paint("\x1b[0m");

pub(crate) const C_CYAN: Paint = Paint("\x1b[36m");

pub(crate) const C_GREEN: Paint = Paint("\x1b[32m");

pub(crate) const C_YELLOW: Paint = Paint("\x1b[33m");

pub(crate) const C_RED: Paint = Paint("\x1b[31m");

pub(crate) const C_DIM: Paint = Paint("\x1b[2m");

/// Where the address in use came from, and the repository's pin — set once
/// in `main`, read by `rpc`, which is called from every verb.
pub(crate) static HOST_SOURCE: std::sync::OnceLock<homelab_client::repo_config::HostSource> =
    std::sync::OnceLock::new();

pub(crate) static REPO_PIN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// fix-101 (cli-path-vs-name-and-cwd, 2026-09-27): the repository, found once
/// in `main`, so every verb reads the same stacks from any directory.
pub(crate) static REPO_ROOT: std::sync::OnceLock<Option<std::path::PathBuf>> =
    std::sync::OnceLock::new();

// fix-66: `--answer allow|stop`, read once in `run`. Pre-answers any host
// question an operation started from this command raises, so a script
// never waits on a question nobody is watching for.
pub(crate) static PRE_ANSWER: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();

/// fix-185: this build's own version, carried with every `homelab ui` step
/// so the dashboard can tell a step from a stale tab's.
pub(crate) const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn repo_root() -> Option<&'static Path> {
    REPO_ROOT.get().and_then(|r| r.as_deref())
}

pub(crate) fn cwd() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// fix-101: the stack directory an argument names, `almanac` or
/// `stacks/almanac` alike.
pub(crate) fn stack_dir(arg: &str) -> std::path::PathBuf {
    let dir = homelab_client::repo_config::stack_dir(arg, &cwd(), repo_root());
    if repo_root().is_none() && !dir.exists() {
        eprintln!(
            "{}note: no repository found from here — run this inside it, or set \
             HOMELAB_REPO=<path> in ~/.config/homelab/env{}",
            C_YELLOW, C_RESET
        );
    }
    dir
}

/// fix-101: the stacks directory, the repository's wherever the command runs.
pub(crate) fn stacks_base() -> std::path::PathBuf {
    homelab_client::repo_config::stacks_dir(repo_root())
}

/// fix-101: a path inside the repository, or relative to where the command
/// runs when no repository was found.
pub(crate) fn in_repo(rel: &str) -> std::path::PathBuf {
    repo_root()
        .map(|r| r.join(rel))
        .unwrap_or_else(|| std::path::PathBuf::from(rel))
}

pub(crate) fn die(msg: &str) -> ! {
    eprintln!("{}error:{} {}", C_RED, C_RESET, msg);
    std::process::exit(1);
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): every route in the
/// stacks directory `base`, held against every other before anything is
/// sent — one owner per hostname, every backend a stack's address or
/// declared external. The whole directory rather than the one stack: a
/// duplicate hostname is a fact about two stacks, and the one being planned
/// may be either of them.
/// feat-stacks-7: the verbs that act on a stack take their arguments from
/// `cli_args::parse`, the parser the dashboard's "copy as CLI command" is
/// tested against.
pub(crate) fn invocation(args: &[String]) -> homelab_client::cli_args::Invocation {
    match homelab_client::cli_args::parse(args.get(1..).unwrap_or(&[])) {
        Ok(Some(inv)) => inv,
        Ok(None) => die("internal: this verb has no parsed form"),
        Err(e) => die(&e),
    }
}

pub(crate) fn check_fleet_routes(base: &Path) {
    match spec::fleet_route_problems(base) {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => die(&format!("route check failed:\n  {}", problems.join("\n  "))),
        Err(e) => die(&format!("route check: {}", e)),
    }
}

/// Sets one `KEY=value` line of an env file when it is a HOMELAB_* key the
/// environment does not already have.
fn apply_env_line(line: &str) {
    let t = line.trim();
    if t.starts_with('#') || t.is_empty() {
        return;
    }
    let Some((k, v)) = t.split_once('=') else {
        return;
    };
    let k = k.trim().trim_start_matches("export ").trim();
    // Every HOMELAB_* key, not a hand-picked pair.
    //
    // The first version named HOMELAB_HOST and HOMELAB_TOKEN
    // explicitly and silently left HOMELAB_LATCH_ENV behind, so
    // `homelab check` worked from anywhere and `homelab deploy` of any
    // stack with secrets did not — a fix that turned "works from any
    // directory" into "works for some commands", which is worse than
    // not fixing it, because it looks done.
    if !k.starts_with("HOMELAB_") {
        return;
    }
    if std::env::var(k).is_ok() {
        return; // the environment already said so
    }
    let v = v.trim().trim_matches('"').trim_matches('\'');
    if !v.is_empty() {
        // SAFETY: this runs before the tokio runtime starts (this is
        // the first thing main() calls), so no other thread exists
        // yet to observe the write. The one exception to the
        // workspace's `unsafe_code = "deny"` besides fix-53's
        // `libc::killpg`.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var(k, v);
        }
    }
}

/// Fill HOMELAB_HOST/HOMELAB_TOKEN from a config file when they are not
/// already in the environment.
///
/// `~/.config/homelab/env` first, then `./.env` for anybody standing in the
/// repository. Both are `KEY=value` files; quotes are stripped because a
/// shell-sourced file usually has them and a token with a quote in it would
/// otherwise fail in a way that reads like a wrong token.
pub(crate) fn load_config_env() {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(std::path::PathBuf::from(home).join(".config/homelab/env"));
    }
    candidates.push(std::path::PathBuf::from(".env"));
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        text.lines().for_each(apply_env_line);
    }
}
