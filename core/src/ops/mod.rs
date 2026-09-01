//! Operations: step lists executed through the shared runner (AR3).

pub mod backup;
pub mod busy;
pub mod dashboard;
pub mod deploy;
pub mod destroy;
pub mod discovery;
pub mod enable;
pub mod fleetcheck;
pub mod guards;
pub mod hardware;
pub mod homepage;
pub mod kea;
pub mod mirror;
pub mod native;
pub mod patch;
pub mod reconcile;
pub mod registry_cache;
pub mod resize;
pub mod selfupdate;
pub mod template;
pub mod update;
pub mod util;
pub mod zfs;

use crate::error::CoreError;
use crate::executor::{pct_sh, CmdOutput, Executor};
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
    /// H2: OPNsense Kea reservation config; None = feature off.
    pub kea: Option<kea::KeaCfg>,
    /// T1: directory on the host where per-stack Prometheus discovery files
    /// are written. None = feature off, and the scrape list stays whatever
    /// somebody last typed into prometheus.yml.
    pub metrics_targets_dir: Option<String>,
    /// T2: Grafana's provisioning directory, as a path INSIDE the gateway
    /// container. None = feature off, and dashboards stay hand-made.
    pub grafana_dashboards_dir: Option<String>,
    /// T51: Homepage's `services.yaml`, as a path on the PROXMOX HOST — it
    /// lives under `/appdata`, like every other app's configuration. None =
    /// feature off and the front page stays hand-made, which is how it came
    /// to be zero bytes.
    pub homepage_services_file: Option<String>,
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
    /// D60: the pull-through cache in the house, when there is one. None =
    /// every image keeps naming its own origin, which is also what happens
    /// when the cache is configured but does not answer.
    pub registry_cache: Option<registry_cache::CacheCfg>,
}
