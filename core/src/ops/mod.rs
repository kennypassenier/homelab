//! Operations: step lists executed through the shared runner (AR3).

/// Run a step, and end the operation with its error when it fails (A3).
///
/// Defined here, before the modules, so every operation sees this one
/// (rust-code-hygiene, 2026-09-27: there were eleven identical copies).
/// deploy.rs shadows it with its own, which also marks the stack incomplete.
macro_rules! step {
    ($runner:expr_2021, $name:expr_2021, $body:expr_2021) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

pub mod backup;
pub mod busy;
pub mod deploy;
pub mod deployguard;
pub mod destroy;
pub mod devicebackup;
pub mod discovery;
pub mod edge;
pub mod enable;
pub mod facts;
pub mod fleetcheck;
pub mod guards;
pub mod hardware;
pub mod homeaddress;
pub mod hostcpu;
pub mod livestatus;
pub mod logshipper;
pub mod manualchecks;
pub mod mirror;
pub mod native;
pub mod night;
pub mod patch;
pub mod pinexists;
pub mod pins;
pub mod pool;
pub mod probes;
pub mod reconcile;
pub mod registry_cache;
pub mod resize;
pub mod restarthost;
pub mod restoredrill;
pub mod retired;
pub mod secondcopy;
pub mod secrets;
pub mod selfupdate;
pub mod template;
pub mod tiles;
pub mod today;
pub mod update;
pub mod util;
pub mod watched;
pub mod zfs;

use crate::error::CoreError;
use crate::executor::{CmdOutput, Executor, pct_sh};
use crate::runner::Journal;
use crate::safety::SafetyConfig;
use crate::sink::Sink;

/// A1/A2 guard shared by every mutating op that targets an existing
/// container: the vmid must not be on the no-touch list AND must carry the
/// expected `<vmid>-app-<stack>` hostname. Deploy/destroy embed this in
/// their pipelines; backup/restore/update call it up front.
pub(crate) async fn guard_target(
    exec: &dyn Executor,
    safety: &SafetyConfig,
    vmid: u16,
    expected_hostname: &str,
) -> Result<(), CoreError> {
    if safety.no_touch.contains(&vmid) {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is on the no-touch list",
            vmid
        )));
    }
    let cfg = exec
        .run(&crate::executor::Cmd::new(
            "pct",
            &["config", &vmid.to_string()],
            30,
        ))
        .await?;
    if !cfg.success() {
        return Err(CoreError::Other(format!("vmid {} does not exist", vmid)));
    }
    let live = cfg
        .stdout
        .lines()
        .find(|l| l.starts_with("hostname:"))
        .map(|l| l.trim_start_matches("hostname:").trim().to_string())
        .unwrap_or_default();
    if live != expected_hostname {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is '{}', expected '{}' — refusing",
            vmid, live, expected_hostname
        )));
    }
    Ok(())
}

/// Shared helper: run a shell script inside an LXC (re-exported for ops).
pub(crate) async fn util_pct_sh(
    exec: &dyn Executor,
    vmid: u16,
    script: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    pct_sh(exec, vmid, script, timeout_s).await
}

/// Everything an operation needs from the outside world. Constructed by the
/// host per request; constructed from mocks in tests.
pub struct OpCtx<'a> {
    pub exec: &'a dyn Executor,
    pub sink: &'a dyn Sink,
    pub journal: &'a dyn Journal,
    pub safety: SafetyConfig,
    /// e.g. /var/lib/homelab
    pub state_dir: String,
    /// Unix timestamp supplied by the caller — core never reads clocks.
    pub now_unix: u64,
    /// T1: directory on the host where per-stack Prometheus discovery files
    /// are written. None = feature off, and the scrape list stays whatever
    /// somebody last typed into prometheus.yml.
    pub metrics_targets_dir: Option<String>,
    /// C1/C2: where the log shipper pushes. Unset means no shipper is
    /// installed at all, which is deliberate — a deploy that cannot know
    /// where Loki is must not guess an address and report success.
    pub loki_url: Option<String>,
    /// Where restic lives, for the operations that read the repository
    /// without being a backup themselves — E3 auto-restore and the
    /// `last_backup` recovery in deploy. Both used to build their own
    /// `BackupCfg::default()` while the host had a configured one, so a
    /// changed `restic_base` in settings.toml would have sent them to a
    /// repository that does not exist: auto-restore then reports "no
    /// snapshot — fresh" and the deploy continues with an empty config
    /// directory. Not optional on purpose — the caller has to say which
    /// repository it means.
    pub backup: backup::BackupCfg,
    /// T69: how an operation reaches whoever is watching, when a step finds
    /// something only a person can judge. `Unattended` everywhere there is
    /// nobody — the nightly scheduler, a test — and that is not a stub but
    /// the honest answer: a question asked into an empty room must not hang
    /// the night, and must not pretend to have been answered either.
    pub asker: &'a dyn crate::ask::Asker,
    /// D60: the pull-through cache in the house, when there is one. None =
    /// every image keeps naming its own origin, which is also what happens
    /// when the cache is configured but does not answer.
    pub registry_cache: Option<registry_cache::CacheCfg>,
    /// rule-20 (disk-audit, 2026-10-01): the fleet default log rotation —
    /// applied to a data mount that declares no `rotate:` of its own and
    /// does not explicitly opt out (`DataMount::no_default_rotate`). None =
    /// no fleet default, which is the behaviour every stack had before this:
    /// only an explicit `rotate:` rotates anything.
    pub default_log_rotation: Option<guards::FleetLogRotationDefault>,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// dashboard's own address, from which its once-a-minute tile watch
    /// reaches every stack's tiles. None or empty = feature off — the
    /// firewall step derives no rule from `tiles:`, exactly as if the
    /// stack declared none. Fleet-wide setting `tile_watch_source`
    /// (`core::hostconfig`).
    pub tile_watch_source: Option<String>,
    /// tile-watch-watcher-out (found 2026-10-01): every applied stack's own
    /// ip and the tile-watch ports its own tiles probe — every stack, with
    /// or without a firewall of its own, since the watcher's own `OUT DROP`
    /// blocks both alike — computed once from `HostState`
    /// (`core::ops::tiles::fleet_tile_watch_targets`) and handed to the
    /// deploy so it can derive the watcher's own OUT rule the same way the
    /// fleet check and the editplan preview do. Empty when there is no
    /// state to read yet, or the caller has none to offer.
    pub tile_watch_targets: crate::firewall::FleetTileTargets,
    /// The stack whose own ip is `tile_watch_source` — the watcher itself —
    /// so a deploy that changes the fleet's tile-watch targets can also
    /// re-render and write THAT stack's firewall file in the same op,
    /// without redeploying it (`core::ops::tiles::tile_watch_watcher`).
    /// None when the watcher is not a currently-applied stack, or
    /// `tile_watch_source` is unset.
    pub tile_watch_watcher: Option<crate::firewall::TileWatcher>,
}
