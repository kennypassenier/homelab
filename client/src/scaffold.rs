//! Scaffold a new stack directory (G2 / D7): writes a real, deployable
//! stacks/<name>/ tree — lxc-compose.yml manifest + a starter app compose —
//! using the preset the wizard picked. Only the per-VM values are
//! substituted; the rest is the canonical template. The log shipper is not
//! part of the scaffold: the deploy installs Grafana Alloy in every container
//! itself (C1/C2, 2026-09-02).

use std::collections::BTreeMap;
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
    /// Core apps injected into every new stack (D8), copied from
    /// `presets/_core/<app>/`. Empty since the Alloy migration: promtail was
    /// the only one, and the deploy now installs the log shipper itself, so a
    /// scaffolded sidecar would be a second, end-of-life shipper (test-plan
    /// part A, 2026-09-26). Watchtower is deliberately dropped (D9); traefik
    /// was never auto-injected.
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
    /// Owner for /appdata dirs: the unprivileged-LXC root mapping (100000)
    /// plus the in-container uid most images run as (1000).
    pub appdata_owner_uid: u32,
}

impl Default for StackDefaults {
    fn default() -> Self {
        Self {
            ip_prefix: "10.10.10.".into(),
            cidr: 24,
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: 10,
            // 998, not 999. 999 is the v1 golden image and no stack has used
            // it for two generations: ten of the eleven live stacks clone 998
            // (v3 unprivileged) and the two privileged ones clone 997. A
            // default nobody uses is a default nobody checks, and a stack
            // scaffolded from it would start two generations behind on the
            // guards, the log caps and unattended-upgrades that B8 bakes in.
            template: "clone:996".into(),
            storage: "local-lvm".into(),
            features: "nesting=1,keyctl=1".into(),
            unprivileged: true,
            boot_order: 99,
            default_cores: 2,
            default_disk_gb: 32,
            core_apps: Vec::new(),
            swap_divisor: 4,
            swap_min_mb: 512,
            swap_max_mb: 2048,
            protection: true,
            appdata_owner_uid: 101000,
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
// preset is a file edit, never a recompile. `presets/_core/` would hold apps
// injected into every stack; it is empty since the deploy installs the log
// shipper itself. See docs/PRESET_GUIDE.md.

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
    /// H4: pass the host GPU into the container (VAAPI transcoding).
    pub gpu: bool,
    /// H4: give the container a /dev/net/tun device (VPN clients).
    pub vpn: bool,
    /// feat-tiles-2 (B): this preset's suggested tile per app, keyed by the
    /// app directory name — the tile step's default, offered rather than
    /// forced: the wizard still lets a blank entry mean no tile at all, and
    /// an explicit `TileChoice` in `StackParams::tiles` always wins over
    /// this one. Generic by design (a preset's own file, not code) — the
    /// `no app-specific knowledge in code` rule is about `core/`/`client/`
    /// Rust, not a preset's own declared data.
    pub tiles: BTreeMap<String, PresetTile>,
}

/// One preset app's suggested tile (`PresetMeta::tiles`), every field
/// optional — the wizard fills in what is missing with its own sensible
/// blanks (the app's own name, the preset's own default group).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PresetTile {
    /// The hostname template, e.g. `__NAME__.kp-soft.dev`; `__NAME__` is
    /// substituted with the stack's own name the same way the compose
    /// templates are (`copy_app_templates`). `None`: the wizard proposes
    /// nothing and the person types one, or leaves the tile out.
    pub hostname: Option<String>,
    pub name: Option<String>,
    pub group: Option<String>,
    pub description: Option<String>,
    pub watch_every: Option<u64>,
    pub down_after: Option<u64>,
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
            gpu: false,
            vpn: false,
            tiles: BTreeMap::new(),
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
    // app-knowledge (Kenny, 2026-09-30): no app is known here. The apps a
    // new stack can start from are the directories in presets/; without
    // them the wizard offers only the empty stack.
    vec![LoadedPreset {
        name: "custom".into(),
        meta: PresetMeta {
            description: "Empty stack — add apps later".into(),
            ram_mb: 1024,
            ..Default::default()
        },
        dir: None,
        apps: Vec::new(),
        synth_app: None,
    }]
}

/// The `/appdata` paths a scaffold WOULD create, without creating anything.
///
/// The wizard needs them before it writes: the question "does this app keep
/// files of its own" has to be asked about real paths, and the answer has to
/// reach the manifest the same run. Reads the same templates the scaffold
/// copies and applies the same substitution, so preview and result cannot
/// drift apart.
pub fn preview_appdata_paths(
    presets_base: &Path,
    preset: Option<&LoadedPreset>,
    name: &str,
    vmid: u16,
) -> Vec<String> {
    let ip = format!("10.10.10.{}", vmid.saturating_sub(100));
    let mut out = std::collections::BTreeSet::new();
    let mut scan = |raw: &str| {
        let content = substitute(raw, name, vmid, &ip);
        for path in appdata_paths_in(&content) {
            out.insert(path);
        }
    };
    if let Some(pr) = preset {
        if let Some(dir) = pr.dir.as_ref() {
            for app in &pr.apps {
                if let Ok(raw) = std::fs::read_to_string(dir.join(app).join("docker-compose.yml")) {
                    scan(&raw);
                }
            }
        } else if let Some((app, image)) = pr.synth_app.as_ref() {
            scan(&generic_compose(app, image, name));
        }
    }
    for core in &StackDefaults::default().core_apps {
        let f = presets_base
            .join("_core")
            .join(core)
            .join("docker-compose.yml");
        if let Ok(raw) = std::fs::read_to_string(f) {
            scan(&raw);
        }
    }
    out.into_iter().collect()
}

/// Substitute the scaffold placeholders in a template file.
pub fn substitute(template: &str, name: &str, vmid: u16, ip: &str) -> String {
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
    /// `/appdata` paths whose app keeps nothing of its own, written into the
    /// manifest as `no_data: true`.
    ///
    /// Kenny's rule, form B4b: "de TUI moet van alle features van dit project
    /// gebruik kunnen maken". A flag only the person editing YAML by hand can
    /// reach is a flag that will be forgotten at exactly the moment it is
    /// needed — the gateway's cloudflared directory sat empty and undeclared
    /// for months, and the backup it silently stopped was found by accident.
    pub no_data_paths: &'a [String],
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
        no_data_paths,
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
    let manifest_head = format!(
        "# Scaffolded by the homelab wizard (G2). Intent only — no state.\n\
         stack_name: {name}\n\
         vmid: {vmid}\n\
         hostname: {vmid}-app-{name}\n\n\
         network:\n  ip: {ip_prefix}{ip_suffix}/{cidr}\n  gateway: {gateway}\n  bridge: {bridge}\n  vlan: {vlan}\n\n\
         resources:\n  cores: {cores}\n  memory_mb: {ram_mb}\n  swap_mb: {swap_mb}\n  disk_gb: {disk_gb}\n  storage: {storage}\n\n\
         lxc:\n  template: \"{template}\"\n  unprivileged: {unprivileged}\n  features: \"{features}\"\n  protection: {protection}{hw}\n\n\
         boot:\n  onboot: true\n  order: {order}\n",
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
        hw = {
            let mut hw = String::new();
            if preset.map(|pr| pr.meta.gpu).unwrap_or(false) {
                hw.push_str("\n  gpu: true");
            }
            if preset.map(|pr| pr.meta.vpn).unwrap_or(false) {
                hw.push_str("\n  vpn: true");
            }
            hw
        },
        order = d.boot_order,
    );

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

    // Core apps (D8): copied from presets/_core/<app>/. A core app whose
    // directory is missing is refused rather than invented.
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
        } else {
            return Err(format!(
                "presets/_core/{} is missing from {} — restore the presets \
                 directory or drop {} from core_apps",
                core,
                presets_base.display(),
                core
            ));
        }
    }

    // Storage intent is DERIVED from the compose files (single source of
    // truth): every host bind under /appdata/ becomes a manifest storage
    // entry, so the deploy creates + chowns the host dir and mounts it into
    // the LXC. Nothing to keep in sync by hand.
    let appdata = appdata_mounts(&files);
    let storage_yaml = if appdata.is_empty() {
        String::new()
    } else {
        let entries = appdata
            .iter()
            .map(|path| {
                let hollow = if no_data_paths.iter().any(|n| n == path) {
                    "\n    # Declared to keep nothing of its own: this app gets no restic\n    # repository at all, so an empty directory here is the design rather\n    # than a backup that silently stopped (F154).\n    no_data: true"
                } else {
                    ""
                };
                format!(
                    "  - host_path: {p}\n    mount_point: {p}\n    host_owner_uid: {uid}{hollow}",
                    p = path,
                    uid = d.appdata_owner_uid,
                    hollow = hollow
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("\nstorage:\n{}\n", entries)
    };

    // feat-tiles-2 (B): one tile per app whose preset suggests one
    // (`PresetMeta::tiles`) — a default only, never forced: a preset with
    // none declared scaffolds exactly as it always did. The wizard's own
    // explicit per-app choice (add/edit/drop a tile, any group) is applied
    // afterward, through the same `stackedit_tiles::TilesEdit` a stack's
    // Tiles form already uses — one code path for "what a tile is",
    // whether it is set at scaffold time or edited into an existing stack
    // later.
    struct TileRow<'a> {
        host: String,
        name: String,
        group: String,
        description: Option<&'a str>,
        watch_every: Option<u64>,
        down_after: Option<u64>,
    }
    let tile_rows: Vec<TileRow> = apps
        .iter()
        .filter_map(|app| {
            let pt = preset.and_then(|pr| pr.meta.tiles.get(app))?;
            let hostname = pt.hostname.as_ref()?.replace("__NAME__", name);
            Some(TileRow {
                host: hostname,
                name: pt.name.clone().unwrap_or_else(|| app.clone()),
                group: pt.group.clone().unwrap_or_else(|| "Apps".to_string()),
                description: pt.description.as_deref(),
                watch_every: pt.watch_every,
                down_after: pt.down_after,
            })
        })
        .collect();
    let tiles_yaml = if tile_rows.is_empty() {
        String::new()
    } else {
        let entries = tile_rows
            .iter()
            .map(|row| {
                let mut s = format!(
                    "  {}:\n    name: \"{}\"\n    group: \"{}\"\n",
                    row.host,
                    yaml_escape(&row.name),
                    yaml_escape(&row.group)
                );
                if let Some(d) = row.description {
                    s += &format!("    description: \"{}\"\n", yaml_escape(d));
                }
                if let Some(w) = row.watch_every {
                    s += &format!("    watch_every: {w}\n");
                }
                if let Some(d) = row.down_after {
                    s += &format!("    down_after: {d}\n");
                }
                s
            })
            .collect::<Vec<_>>()
            .join("");
        format!("\ntiles:\n{entries}")
    };

    let manifest = format!("{manifest_head}{storage_yaml}{tiles_yaml}\napps:\n{apps_yaml}\n");
    write_file(&dir.join("lxc-compose.yml"), &manifest, &mut files)?;

    Ok(Scaffolded { dir, files })
}

/// Scan written compose files for `- /appdata/...:<container path>` host
/// binds. Returns the unique host paths, sorted.
fn appdata_mounts(files: &[String]) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    for f in files {
        if !f.ends_with("docker-compose.yml") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(f) else {
            continue;
        };
        for path in appdata_paths_in(&raw) {
            out.insert(path);
        }
    }
    out.into_iter().collect()
}

/// The host side of every `/appdata/...` bind in one compose file. Shared by
/// the scaffold and the wizard's preview so the two cannot disagree about
/// which paths a stack is going to have.
pub fn appdata_paths_in(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line
            .trim()
            .trim_start_matches("- ")
            .trim_matches(['"', '\'']);
        if let Some(host) = t.split(':').next() {
            if host.starts_with("/appdata/") {
                out.push(host.to_string());
            }
        }
    }
    out
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

/// A value written inside `"…"` in generated YAML: backslash and the
/// closing quote are the only two characters that would break it.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_file(path: &Path, content: &str, files: &mut Vec<String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {}", path.display(), e))?;
    files.push(path.display().to_string());
    Ok(())
}
