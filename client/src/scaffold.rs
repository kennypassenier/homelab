//! Scaffold a new stack directory (G2 / D7): writes a real, deployable
//! stacks/<name>/ tree — lxc-compose.yml manifest + a starter app compose +
//! the promtail core app (D8) — using the preset the wizard picked. Only the
//! per-VM values are substituted; the rest is the canonical template.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Configurable stack conventions (AR11): defaults match Kenny's proven
/// ansible + legacy-TUI values, overridable via client config so nothing is
/// hardcoded at the call site. Swap is a tiered formula, IP is derived from
/// the vmid, and the network/lxc knobs live here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StackDefaults {
    /// Last-octet base: ip = "{ip_prefix}{vmid - 100}".
    pub ip_prefix: String,
    pub cidr: u8,
    pub gateway: String,
    pub bridge: String,
    pub vlan: u16,
    pub template: String,
    pub storage: String,
    pub features: String,
    pub unprivileged: bool,
    /// Default startup order for application stacks (platform=5, mqtt=20 are
    /// per-stack overrides; role default is 99).
    pub boot_order: u16,
    pub default_cores: u16,
    pub default_disk_gb: u16,
    /// Core apps injected into every new stack (D8): promtail only. Watchtower
    /// is deliberately dropped (D9 — replaced by managed updates with a
    /// per-app update.policy label); traefik was never auto-injected.
    pub core_apps: Vec<String>,
    /// Swap auto-formula: clamp(RAM / divisor, min, max). For LXC, swap is a
    /// cap on shared HOST swap — keep it small so a runaway container OOMs
    /// fast instead of grinding the whole host. Matches Kenny's hand-tuned
    /// production values (mostly 512, media 2048). Editable per stack; 0 is
    /// valid for database-heavy stacks.
    pub swap_divisor: u32,
    pub swap_min_mb: u32,
    pub swap_max_mb: u32,
    /// Proxmox-level protection flag: refuses destroy at the hypervisor even
    /// outside this tool. Gated destroy (C2) lifts it deliberately first.
    pub protection: bool,
}

impl Default for StackDefaults {
    fn default() -> Self {
        Self {
            ip_prefix: "10.10.10.".into(),
            cidr: 24,
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: 10,
            template: "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst".into(),
            storage: "local-lvm".into(),
            features: "nesting=1,keyctl=1".into(),
            unprivileged: true,
            boot_order: 99,
            default_cores: 2,
            default_disk_gb: 32,
            core_apps: vec!["promtail".into()],
            swap_divisor: 4,
            swap_min_mb: 512,
            swap_max_mb: 2048,
            protection: true,
        }
    }
}

impl StackDefaults {
    /// Auto swap: clamp(RAM/4, 512, 2048) by default — container-appropriate
    /// sizing, unlike machine-style 1:1 rules.
    pub fn swap_for(&self, ram_mb: u32) -> u32 {
        (ram_mb / self.swap_divisor.max(1)).clamp(self.swap_min_mb, self.swap_max_mb)
    }
}

pub struct Scaffolded {
    pub dir: PathBuf,
    pub files: Vec<String>,
}

pub struct StackParams<'a> {
    pub name: &'a str,
    pub vmid: u16,
    pub ram_mb: u32,
    pub cores: u16,
    pub disk_gb: u16,
    /// None = auto via the swap formula; Some(0) is valid (no swap).
    pub swap_mb: Option<u32>,
    pub app: Option<(&'a str, &'a str)>, // (app name, image)
}

/// Create `base/<name>/` with a manifest, the preset's primary app, and
/// promtail. Returns the created paths. Errors if the dir already exists.
/// Uses [`StackDefaults::default`] for conventions; call the `_with` variant
/// to supply overrides.
pub fn scaffold_stack(base: &Path, p: &StackParams) -> Result<Scaffolded, String> {
    scaffold_stack_with(base, p, &StackDefaults::default())
}

pub fn scaffold_stack_with(
    base: &Path,
    p: &StackParams,
    d: &StackDefaults,
) -> Result<Scaffolded, String> {
    let StackParams {
        name,
        vmid,
        ram_mb,
        cores,
        disk_gb,
        swap_mb,
        app: preset_app,
    } = *p;
    let dir = base.join(name);
    if dir.exists() {
        return Err(format!("stacks/{} already exists", name));
    }
    let ip_suffix = vmid.saturating_sub(100);
    let swap_mb = swap_mb.unwrap_or_else(|| d.swap_for(ram_mb));
    let mut files = Vec::new();

    // lxc-compose.yml (schema v2, intent only).
    let mut apps = Vec::new();
    if preset_app.is_some() {
        apps.push(name.to_string());
    }
    for core in &d.core_apps {
        apps.push(core.clone());
    }
    let apps_yaml = apps
        .iter()
        .map(|a| format!("  - {}", a))
        .collect::<Vec<_>>()
        .join("\n");
    let manifest = format!(
        "# Scaffolded by the homelab wizard (G2). Intent only — no state.\n\
         stack_name: {name}\n\
         vmid: {vmid}\n\
         hostname: {vmid}-app-{name}\n\n\
         network:\n  ip: {ip_prefix}{ip_suffix}/{cidr}\n  gateway: {gateway}\n  bridge: {bridge}\n  vlan: {vlan}\n\n\
         resources:\n  cores: {cores}\n  memory_mb: {ram_mb}\n  swap_mb: {swap_mb}\n  disk_gb: {disk_gb}\n  storage: {storage}\n\n\
         lxc:\n  template: \"{template}\"\n  unprivileged: {unprivileged}\n  features: \"{features}\"\n  protection: {protection}\n\n\
         boot:\n  onboot: true\n  order: {order}\n\n\
         apps:\n{apps_yaml}\n",
        ip_prefix = d.ip_prefix,
        cidr = d.cidr,
        gateway = d.gateway,
        bridge = d.bridge,
        vlan = d.vlan,
        storage = d.storage,
        template = d.template,
        unprivileged = d.unprivileged,
        features = d.features,
        protection = d.protection,
        order = d.boot_order,
    );
    write_file(&dir.join("lxc-compose.yml"), &manifest, &mut files)?;

    // Primary app compose (if the preset has one).
    if let Some((app, image)) = preset_app {
        let compose = format!(
            "services:\n  {app}:\n    image: {image}\n    container_name: {app}\n    restart: unless-stopped\n    labels:\n      # backup: stop this container while it is snapshotted (E4)\n      - com.homelab.backup.pause=true\n      # managed updates (D9): manual by default; set auto or auto-after-Nd\n      - com.homelab.update.policy=manual\n    networks:\n      - {name}_net\n\nnetworks:\n  {name}_net:\n    external: true\n    name: {name}_net\n"
        );
        write_file(
            &dir.join(app).join("docker-compose.yml"),
            &compose,
            &mut files,
        )?;
    }

    // Promtail core app (D8) — ships logs to the existing Loki.
    let promtail_compose = format!(
        "services:\n  promtail:\n    image: grafana/promtail:3.0.0\n    container_name: promtail\n    command: -config.file=/etc/promtail/config.yml\n    volumes:\n      - /var/lib/docker/containers:/var/lib/docker/containers:ro\n      - /var/log:/var/log:ro\n      - ./promtail-config.yml:/etc/promtail/config.yml:ro\n      - ./data:/tmp/positions\n    restart: unless-stopped\n    networks:\n      - {name}_net\n\nnetworks:\n  {name}_net:\n    external: true\n    name: {name}_net\n"
    );
    write_file(
        &dir.join("promtail").join("docker-compose.yml"),
        &promtail_compose,
        &mut files,
    )?;
    let promtail_cfg = format!(
        "server:\n  http_listen_port: 9080\n  grpc_listen_port: 0\n\npositions:\n  filename: /tmp/positions/positions.yaml\n\nclients:\n  - url: http://10.10.10.4:3100/loki/api/v1/push\n\nscrape_configs:\n  - job_name: docker\n    static_configs:\n      - targets: [localhost]\n        labels:\n          job: docker\n          stack: {name}\n          host: {vmid}-app-{name}\n          __path__: /var/lib/docker/containers/*/*-json.log\n"
    );
    write_file(
        &dir.join("promtail").join("promtail-config.yml"),
        &promtail_cfg,
        &mut files,
    )?;

    Ok(Scaffolded { dir, files })
}

fn write_file(path: &Path, content: &str, files: &mut Vec<String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {}", path.display(), e))?;
    files.push(path.display().to_string());
    Ok(())
}
