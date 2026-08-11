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

// ── Data-driven presets (G2) ────────────────────────────────────────────────
// A preset is a DIRECTORY under presets/: `preset.yml` (metadata) plus one
// subdirectory per app holding the literal files to install (compose, config,
// …). Files are copied with placeholder substitution — adding or changing a
// preset is a file edit, never a recompile. `presets/_core/` holds apps
// injected into every stack (promtail). See docs/PRESET_GUIDE.md.

/// Metadata from `preset.yml`. Everything except `description`/`ram_mb` is an
/// optional override on [`StackDefaults`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetMeta {
    pub description: String,
    pub ram_mb: u32,
    pub cores: Option<u16>,
    pub disk_gb: Option<u16>,
    /// LXC features override (e.g. a future GPU/TUN preset).
    pub features: Option<String>,
    pub unprivileged: Option<bool>,
}

impl Default for PresetMeta {
    fn default() -> Self {
        Self {
            description: String::new(),
            ram_mb: 1024,
            cores: None,
            disk_gb: None,
            features: None,
            unprivileged: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedPreset {
    pub name: String,
    pub meta: PresetMeta,
    /// Preset directory on disk; None for synthetic (test/demo) presets.
    pub dir: Option<PathBuf>,
    /// App subdirectories, in sorted order.
    pub apps: Vec<String>,
    /// Synthetic fallback: (app, image) generates a generic compose when
    /// there is no directory to copy from.
    pub synth_app: Option<(String, String)>,
}

/// Scan `presets/` (dirs with a preset.yml; `_`-prefixed dirs are reserved
/// for core apps). Returns them sorted, with "custom" forced last. Falls back
/// to [`synthetic_presets`] when the directory is missing or empty so the
/// wizard always works.
pub fn scan_presets(base: &Path) -> Vec<LoadedPreset> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !dir.is_dir() || name.starts_with('_') {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(dir.join("preset.yml")) else {
                continue;
            };
            let Ok(meta) = serde_yaml::from_str::<PresetMeta>(&raw) else {
                continue;
            };
            let mut apps: Vec<String> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            apps.sort();
            out.push(LoadedPreset {
                name,
                meta,
                dir: Some(dir),
                apps,
                synth_app: None,
            });
        }
    }
    out.sort_by(|a, b| {
        (a.name == "custom")
            .cmp(&(b.name == "custom"))
            .then(a.name.cmp(&b.name))
    });
    if out.is_empty() {
        synthetic_presets()
    } else {
        out
    }
}

/// Built-in fallback presets (also used by tests and the offline demo when
/// no presets/ directory exists). Mirrors the shipped preset set.
pub fn synthetic_presets() -> Vec<LoadedPreset> {
    let synth = |name: &str, desc: &str, image: &str, ram: u32| LoadedPreset {
        name: name.into(),
        meta: PresetMeta {
            description: desc.into(),
            ram_mb: ram,
            ..Default::default()
        },
        dir: None,
        apps: vec![name.into()],
        synth_app: Some((name.into(), image.into())),
    };
    vec![
        synth(
            "actual",
            "Envelope budgeting",
            "actualbudget/actual-server:latest",
            512,
        ),
        synth(
            "jellyfin",
            "Media server (VAAPI)",
            "jellyfin/jellyfin:latest",
            4096,
        ),
        synth(
            "mealie",
            "Recipes + meal planning",
            "ghcr.io/mealie-recipes/mealie:latest",
            512,
        ),
        synth(
            "syncthing",
            "Obsidian vault peer",
            "syncthing/syncthing:latest",
            512,
        ),
        synth(
            "uptime-kuma",
            "Uptime monitoring",
            "louislam/uptime-kuma:1",
            512,
        ),
        LoadedPreset {
            name: "custom".into(),
            meta: PresetMeta {
                description: "Empty stack — add apps later".into(),
                ram_mb: 1024,
                ..Default::default()
            },
            dir: None,
            apps: Vec::new(),
            synth_app: None,
        },
    ]
}

/// Substitute the scaffold placeholders in a template file.
fn substitute(template: &str, name: &str, vmid: u16, ip: &str) -> String {
    template
        .replace("__STACK__", name)
        .replace("__VMID__", &vmid.to_string())
        .replace("__HOSTNAME__", &format!("{}-app-{}", vmid, name))
        .replace("__IP__", ip)
}

pub struct StackParams<'a> {
    pub name: &'a str,
    pub vmid: u16,
    pub ram_mb: u32,
    pub cores: u16,
    pub disk_gb: u16,
    /// None = auto via the swap formula; Some(0) is valid (no swap).
    pub swap_mb: Option<u32>,
    pub preset: Option<&'a LoadedPreset>,
}

/// Create `base/<name>/` with a manifest, the preset's apps, and the core
/// apps from `presets/_core/`. Returns the created paths. Errors if the dir
/// already exists. Uses [`StackDefaults::default`] for conventions; call the
/// `_with` variant to supply overrides.
pub fn scaffold_stack(
    base: &Path,
    presets_base: &Path,
    p: &StackParams,
) -> Result<Scaffolded, String> {
    scaffold_stack_with(base, presets_base, p, &StackDefaults::default())
}

pub fn scaffold_stack_with(
    base: &Path,
    presets_base: &Path,
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
        preset,
    } = *p;
    let dir = base.join(name);
    if dir.exists() {
        return Err(format!("stacks/{} already exists", name));
    }
    let ip_suffix = vmid.saturating_sub(100);
    let ip = format!("{}{}", d.ip_prefix, ip_suffix);
    let swap_mb = swap_mb.unwrap_or_else(|| d.swap_for(ram_mb));
    let mut files = Vec::new();

    // Preset overrides on the stack conventions.
    let features = preset
        .and_then(|pr| pr.meta.features.clone())
        .unwrap_or_else(|| d.features.clone());
    let unprivileged = preset
        .and_then(|pr| pr.meta.unprivileged)
        .unwrap_or(d.unprivileged);

    // lxc-compose.yml (schema v2, intent only). The apps list uses the APP
    // directory names — they drive /opt/<stack>/<app> on deploy.
    let mut apps: Vec<String> = preset.map(|pr| pr.apps.clone()).unwrap_or_default();
    for core in &d.core_apps {
        if !apps.contains(core) {
            apps.push(core.clone());
        }
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
        unprivileged = unprivileged,
        features = features,
        protection = d.protection,
        order = d.boot_order,
    );
    write_file(&dir.join("lxc-compose.yml"), &manifest, &mut files)?;

    // Preset apps: copy the preset's template files with substitution, or
    // generate a generic compose for synthetic presets.
    if let Some(pr) = preset {
        if let Some(preset_dir) = pr.dir.as_ref() {
            for app in &pr.apps {
                copy_app_templates(
                    &preset_dir.join(app),
                    &dir.join(app),
                    name,
                    vmid,
                    &ip,
                    &mut files,
                )?;
            }
        } else if let Some((app, image)) = pr.synth_app.as_ref() {
            let compose = generic_compose(app, image, name);
            write_file(
                &dir.join(app).join("docker-compose.yml"),
                &compose,
                &mut files,
            )?;
        }
    }

    // Core apps (D8): copied from presets/_core/<app>/, falling back to the
    // built-in promtail template when the directory is missing.
    for core in &d.core_apps {
        if apps.iter().filter(|a| *a == core).count() == 0 {
            continue; // preset removed it deliberately
        }
        if dir.join(core).exists() {
            continue; // preset shipped its own version
        }
        let core_dir = presets_base.join("_core").join(core);
        if core_dir.is_dir() {
            copy_app_templates(&core_dir, &dir.join(core), name, vmid, &ip, &mut files)?;
        } else if core == "promtail" {
            builtin_promtail(&dir, name, vmid, &mut files)?;
        }
    }

    Ok(Scaffolded { dir, files })
}

/// Copy every file in `src` to `dst` with placeholder substitution.
fn copy_app_templates(
    src: &Path,
    dst: &Path,
    name: &str,
    vmid: u16,
    ip: &str,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(src).map_err(|e| format!("{}: {}", src.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let raw =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let content = substitute(&raw, name, vmid, ip);
        write_file(&dst.join(entry.file_name()), &content, files)?;
    }
    Ok(())
}

fn generic_compose(app: &str, image: &str, name: &str) -> String {
    format!(
        "services:\n  {app}:\n    image: {image}\n    container_name: {app}\n    restart: unless-stopped\n    labels:\n      # backup: stop this container while it is snapshotted (E4)\n      - com.homelab.backup.pause=true\n      # managed updates (D9): manual by default; set auto or auto-after-Nd\n      - com.homelab.update.policy=manual\n    networks:\n      - {name}_net\n\nnetworks:\n  {name}_net:\n    external: true\n    name: {name}_net\n"
    )
}

/// Built-in promtail (D8) used when presets/_core/promtail is absent.
fn builtin_promtail(
    dir: &Path,
    name: &str,
    vmid: u16,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let promtail_compose = format!(
        "services:\n  promtail:\n    image: grafana/promtail:3.0.0\n    container_name: promtail\n    command: -config.file=/etc/promtail/config.yml\n    volumes:\n      - /var/lib/docker/containers:/var/lib/docker/containers:ro\n      - /var/log:/var/log:ro\n      - ./promtail-config.yml:/etc/promtail/config.yml:ro\n      - ./data:/tmp/positions\n    restart: unless-stopped\n    networks:\n      - {name}_net\n\nnetworks:\n  {name}_net:\n    external: true\n    name: {name}_net\n"
    );
    write_file(
        &dir.join("promtail").join("docker-compose.yml"),
        &promtail_compose,
        files,
    )?;
    let promtail_cfg = format!(
        "server:\n  http_listen_port: 9080\n  grpc_listen_port: 0\n\npositions:\n  filename: /tmp/positions/positions.yaml\n\nclients:\n  - url: http://10.10.10.4:3100/loki/api/v1/push\n\nscrape_configs:\n  - job_name: docker\n    static_configs:\n      - targets: [localhost]\n        labels:\n          job: docker\n          stack: {name}\n          host: {vmid}-app-{name}\n          __path__: /var/lib/docker/containers/*/*-json.log\n"
    );
    write_file(
        &dir.join("promtail").join("promtail-config.yml"),
        &promtail_cfg,
        files,
    )?;
    Ok(())
}

fn write_file(path: &Path, content: &str, files: &mut Vec<String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {}", path.display(), e))?;
    files.push(path.display().to_string());
    Ok(())
}
