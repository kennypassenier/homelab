//! feat-client-1 · the host address lives in the repository.
//!
//! Kenny, 2026-09-19 (gap-13 deep dive): "ik wil niet dat het afhankelijk
//! is van de machine". Until then the daemon's address was a compiled-in
//! default with a per-machine override in `~/.config/homelab/env`, so a
//! second desktop — or the Windows box he has in mind — started from the
//! wrong door and trusted whatever certificate answered first. The address
//! and the daemon's fingerprint are facts about the fleet; the token is the
//! only thing that is a fact about the machine, and it stays where it was.
//!
//! Precedence, most specific first: a `HOMELAB_HOST` already in the process
//! environment (a one-off override typed before the command — that is how
//! the 3.52.0 rollout got past the stalled path), then `config/client.toml`
//! found in the working directory or any parent, then the machine's env
//! files, then the default. The repository beating the machine is the
//! point, not an accident of ordering.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the daemon was reachable before the file existed. Kept as the last
/// resort so a checkout without the file still connects, and so the source
/// of the address can be reported honestly (`HostSource::Default`).
pub const DEFAULT_HOST: &str = "10.10.5.250:8443";

/// Relative to the repository root; searched for upward from the working
/// directory, the way `git` finds its own root.
pub const REPO_FILE: &str = "config/client.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientConfig {
    /// `address:port` of the host daemon's WebSocket API.
    pub host: Option<String>,
    /// SHA-256 fingerprint of the daemon's TLS certificate, colon-separated
    /// hex, with or without a `SHA256:` prefix. A fresh machine pins this
    /// instead of trusting whatever answers first.
    pub pin: Option<String>,
    /// app-knowledge (2026-09-30): the read-only Cloudflare token the edge
    /// comparison uses, `~` for the home directory. Unset: the edge is not
    /// compared. It used to be a path in client/src/edge.rs naming the zone.
    #[serde(default)]
    pub edge_token_file: Option<String>,
}

/// Where the address in use came from, so `homelab ping` can say so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSource {
    Environment,
    RepoConfig(PathBuf),
    MachineConfig,
    Default,
}

impl std::fmt::Display for HostSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostSource::Environment => write!(f, "HOMELAB_HOST in the environment"),
            HostSource::RepoConfig(p) => write!(f, "{}", p.display()),
            HostSource::MachineConfig => write!(f, "~/.config/homelab/env"),
            HostSource::Default => write!(f, "built-in default"),
        }
    }
}

/// Walk up from `start` until a `config/client.toml` appears.
pub fn find_repo_file(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(REPO_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// fix-101 (cli-path-vs-name-and-cwd, 2026-09-27): the repository root,
/// found once. Up from `start` for `config/client.toml`, the way the address
/// is found; else `env_repo` (`HOMELAB_REPO`, which `~/.config/homelab/env`
/// may set) when it holds that file. The verbs read `./stacks`, so run from
/// `/tmp` `check` compared half the fleet and `deploy` found nothing.
pub fn repo_root(start: &Path, env_repo: Option<&str>) -> Option<PathBuf> {
    let root_of = |file: PathBuf| file.parent()?.parent().map(Path::to_path_buf);
    if let Some(root) = find_repo_file(start).and_then(root_of) {
        return Some(root);
    }
    let dir = PathBuf::from(env_repo?.trim());
    dir.join(REPO_FILE).is_file().then_some(dir)
}

/// fix-101: the stacks directory the verbs read: the repository's when one
/// was found, else `stacks` where the command runs, as before.
pub fn stacks_dir(root: Option<&Path>) -> PathBuf {
    root.map(|r| r.join("stacks"))
        .unwrap_or_else(|| PathBuf::from("stacks"))
}

/// fix-101: the stack an argument names, however it is spelled: `almanac`,
/// `almanac/`, `stacks/almanac`, or a full path into a stacks directory all
/// say `almanac`. A part below the stack (`kyu/kyu-runner`, `media/radarr`)
/// is kept. The verbs that take a name answered `stack 'stacks/almanac' is
/// not in host state` to the path form.
pub fn stack_name(arg: &str) -> String {
    let s = arg.trim().trim_end_matches('/');
    let s = s.strip_prefix("./").unwrap_or(s);
    let s = match s.rfind("stacks/") {
        Some(i) if i == 0 || s[..i].ends_with('/') => &s[i + "stacks/".len()..],
        _ => s,
    };
    s.to_string()
}

/// fix-101: the stack directory an argument names. The path as typed when it
/// is a stack directory from `cwd` (with an `lxc-compose.yml` or a
/// `service.yml`); otherwise the stack of that name in the repository's
/// stacks directory. The verbs that take a path answered `homelab deploy
/// almanac` with "cannot read almanac/lxc-compose.yml".
pub fn stack_dir(arg: &str, cwd: &Path, root: Option<&Path>) -> PathBuf {
    let typed = cwd.join(arg.trim());
    if typed.join("lxc-compose.yml").is_file() || typed.join("service.yml").is_file() {
        return typed.components().collect();
    }
    stacks_dir(root).join(stack_name(arg))
}

/// Read the repository file if there is one. A file that exists but cannot
/// be parsed is an error, never an absence: standing rule 45 — an input the
/// program could not parse is refused rather than quietly replaced by a
/// default, and a typo here would otherwise send every command to the wrong
/// door without a word.
pub fn load(start: &Path) -> Result<Option<(PathBuf, ClientConfig)>, String> {
    let Some(path) = find_repo_file(start) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let cfg: ClientConfig = toml::from_str(&text).map_err(|e| {
        format!(
            "{} does not parse: {} :: it is TOML — the address is quoted, as in \
             host = \"10.10.10.250:8443\" — and the only keys are host and pin",
            path.display(),
            e.message()
        )
    })?;
    Ok(Some((path, cfg)))
}

/// Pick the address and say where it came from.
///
/// `explicit_env` is what the process environment held BEFORE the env files
/// were read into it; `machine` is what it holds after. Telling the two
/// apart is what lets the repository beat the machine file while a value
/// typed before the command still beats everything.
pub fn resolve_host(
    explicit_env: Option<String>,
    repo: Option<(&Path, &ClientConfig)>,
    machine: Option<String>,
) -> (String, HostSource) {
    if let Some(h) = explicit_env.filter(|h| !h.trim().is_empty()) {
        return (h, HostSource::Environment);
    }
    if let Some((path, cfg)) = repo
        && let Some(h) = cfg.host.as_deref().filter(|h| !h.trim().is_empty())
    {
        return (h.to_string(), HostSource::RepoConfig(path.to_path_buf()));
    }
    if let Some(h) = machine.filter(|h| !h.trim().is_empty()) {
        return (h, HostSource::MachineConfig);
    }
    (DEFAULT_HOST.to_string(), HostSource::Default)
}

/// The outcome of laying the machine's pin next to the repository's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinDecision {
    /// The fingerprint to verify against; `None` means trust on first use,
    /// exactly as before the repository file existed.
    pub pin: Option<String>,
    /// The machine had nothing and took the repository's word — the caller
    /// saves it so the next connect is an ordinary pinned one.
    pub adopted_from_repo: bool,
}

fn normalise(fp: &str) -> String {
    fp.trim()
        .trim_start_matches("SHA256:")
        .trim_start_matches("sha256:")
        .to_ascii_uppercase()
}

/// fix-149 (first-connect-pin, Kenny 2026-09-27: "Pin in de client"): the
/// fleet's certificate fingerprint as `config/client.toml` said when this
/// client was compiled (`client/build.rs`), without its `SHA256:` prefix.
/// `None` for a client built from a tree without a pin.
pub fn built_in_pin() -> Option<&'static str> {
    let pin = env!("HOMELAB_BUILT_IN_PIN").trim();
    let pin = pin.strip_prefix("SHA256:").unwrap_or(pin);
    (!pin.is_empty()).then_some(pin)
}

/// fix-149 (see REGISTER.md): [`reconcile_pin`] with the pin the client was
/// built with on top. With a built-in pin that is the only certificate
/// trusted, first connection included; a machine pin or a repository pin
/// that disagrees with it is refused, never followed, because either one
/// differing is exactly what a changed certificate or a stale checkout
/// looks like, and a person has to look. A client built without a pin
/// behaves as before.
pub fn reconcile_pin_built_in(
    built_in: Option<&str>,
    machine: Option<String>,
    repo: Option<&str>,
) -> Result<PinDecision, String> {
    let Some(built) = built_in.map(normalise).filter(|b| !b.is_empty()) else {
        return reconcile_pin(machine, repo);
    };
    if let Some(r) = repo.map(normalise).filter(|r| !r.is_empty())
        && r != built
    {
        return Err(format!(
            "{} names the host certificate {} but this client was built for {} :: \
                 either the certificate changed and this client predates it — rebuild it \
                 from the repository (`make install`) or install the release built from it \
                 (`homelab self-install`) — or the repository is not the one this client \
                 came from",
            REPO_FILE, r, built
        ));
    }
    let machine = machine.map(|m| normalise(&m)).filter(|m| !m.is_empty());
    if let Some(m) = &machine
        && *m != built
    {
        return Err(format!(
            "the host certificate pinned on this machine (~/.config/homelab/pin: {}) is not \
                 the one this client was built for ({}) :: delete ~/.config/homelab/pin; the \
                 client pins its own",
            m, built
        ));
    }
    Ok(PinDecision {
        pin: Some(built),
        adopted_from_repo: machine.is_none(),
    })
}

/// A repository pin fills an empty machine and never overrules a different
/// one: a machine that pinned something else is what a changed certificate
/// looks like, and the two answers have to be put in front of a person.
pub fn reconcile_pin(machine: Option<String>, repo: Option<&str>) -> Result<PinDecision, String> {
    let machine = machine.map(|m| normalise(&m)).filter(|m| !m.is_empty());
    let repo = repo.map(normalise).filter(|r| !r.is_empty());
    match (machine, repo) {
        (Some(m), Some(r)) if m != r => Err(format!(
            "the host certificate pinned on this machine (~/.config/homelab/pin: {}) is not the \
             one {} names ({}) :: either the daemon's certificate changed — then update the \
             repository file after checking the fingerprint the host printed at boot — or this \
             machine pinned a stale one: delete ~/.config/homelab/pin to take the repository's",
            m, REPO_FILE, r
        )),
        (Some(m), _) => Ok(PinDecision {
            pin: Some(m),
            adopted_from_repo: false,
        }),
        (None, Some(r)) => Ok(PinDecision {
            pin: Some(r),
            adopted_from_repo: true,
        }),
        (None, None) => Ok(PinDecision {
            pin: None,
            adopted_from_repo: false,
        }),
    }
}
