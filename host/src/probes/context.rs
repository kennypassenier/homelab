//! What the security probes need from the configuration.

use crate::*;

/// fix-130: what the new doctor probes need from the configuration.
pub(crate) struct ProbeContext {
    pub(crate) listen: String,
    pub(crate) exec_enabled: bool,
    pub(crate) config_path: String,
    pub(crate) password_file: String,
    pub(crate) privileged_vmids: Vec<u16>,
    pub(crate) no_touch: Vec<u16>,
    pub(crate) drill_interval_s: u64,
    pub(crate) state_dir: String,
    /// fix-130 (second half): where to read the gateway's routes directory,
    /// to name any file in it that no stack declares (the same judgement the
    /// fleet check carries, `fleetcheck::unowned_route_files`).
    pub(crate) gateway_vmid: u16,
    pub(crate) gateway_routes_dir: String,
}

impl ProbeContext {
    pub(crate) fn of(config: &Config) -> Self {
        ProbeContext {
            listen: config.listen.to_string(),
            exec_enabled: config.exec_enabled,
            config_path: config.config_path.clone(),
            password_file: config.backup.password_file.clone(),
            privileged_vmids: config.safety.privileged_vmids.clone().unwrap_or_default(),
            no_touch: config.safety.no_touch.clone(),
            drill_interval_s: config.restore_drill_interval_s,
            state_dir: config.state_dir.clone(),
            gateway_vmid: config.safety.gateway_vmid,
            gateway_routes_dir: config.safety.gateway_routes_dir.clone(),
        }
    }
}
