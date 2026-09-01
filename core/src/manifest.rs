//! Stack manifest (lxc-compose.yml v2, intent only) + THE validator (D10).
//! This is the single validation implementation — the client imports it for
//! instant wizard feedback, the host imports it as its trust boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackManifest {
    pub stack_name: String,
    pub vmid: u16,
    pub hostname: String,
    pub network: NetworkSpec,
    pub resources: ResourceSpec,
    pub lxc: LxcSpec,
    pub boot: BootSpec,
    #[serde(default)]
    pub storage: Vec<MountSpec>,
    pub apps: Vec<String>,
    /// A container that runs no docker at all: its services are native
    /// systemd units, adopted with C7 and supervised through their own
    /// `service.yml`. CT 109 (kyu, kyu-runner, http-switchboard) and CT 112
    /// (almanac) are the two.
    ///
    /// It has to be said out loud rather than inferred from an empty `apps:`
    /// list, because an empty list is exactly what a docker stack looks like
    /// when somebody forgot to fill it in. Kenny asked for these containers
    /// to be rebuildable from the repository like every other one
    /// (2026-08-31); this is the field that lets a manifest describe a
    /// container whose whole job is to hold a binary and a unit file.
    ///
    /// What a rebuild then needs, in order: this manifest recreates the
    /// container, `homelab restore` puts the service's data back from its
    /// own restic repository, and the binary is installed the way C7 already
    /// installs it. None of that is docker's business.
    #[serde(default)]
    pub native_only: bool,
    /// The systemd units this container runs, by name. Each one has a
    /// `service.yml` describing it and a `<unit>/<unit>.service` file beside
    /// it — the unit file lives in the repository now, which it did not
    /// before: if CT 109 had been destroyed on 2026-08-31, nobody would have
    /// had the four unit files that make its services exist. They were only
    /// inside the containers they describe.
    ///
    /// Kenny's N1 (2026-08-31): a native service gets the same full cycle a
    /// docker app gets, so these are declared here for the same reason `apps`
    /// is — the deploy installs them, and a directory may be owned by one.
    #[serde(default)]
    pub natives: Vec<String>,
    /// M1 (2026-08-31): folders that belong to something else. Attached to
    /// the container, never owned by it.
    ///
    /// `storage:` is deliberately strict — under `/appdata/`, named
    /// `<app>-config` — because those are the directories the orchestrator
    /// creates, chowns and backs up, and a restore finds its snapshots by
    /// that name. The media libraries are none of those things: they are two
    /// ZFS datasets that Proxmox hands to CT 103 the fileserver and to the
    /// media containers at the same time, deliberately outside the backup
    /// scope, and terabytes large. Stretching `storage:` to fit them would
    /// have cost the guarantee for every directory that IS ours.
    ///
    /// So they are declared separately and treated as read-only facts about
    /// the host: not created, not chowned, not backed up — and a deploy
    /// refuses if one is missing, because a media container that comes up
    /// with an empty library is exactly the silent failure this project
    /// keeps finding.
    #[serde(default)]
    pub data_mounts: Vec<DataMount>,
    /// W2: this stack's own snapshot retention, overriding the fleet-wide
    /// setting. Absent = the fleet-wide policy, which is the right answer for
    /// almost every stack.
    ///
    /// It exists because one setting for stacks that differ by two orders of
    /// magnitude does not hold: media needed `keep-daily=4` typed by hand
    /// because 24 GB a night against the fleet-wide fourteen would cost half
    /// a terabyte, while kyu at 231 MB could comfortably keep two months.
    /// That difference belongs in the stack file, not in somebody's memory.
    #[serde(default)]
    pub retention: Option<Vec<crate::retention::RetentionTier>>,
    /// A private image registry this stack must sign in to before it can
    /// pull.
    ///
    /// The orchestrator pushes compose files and runs `docker compose pull`;
    /// an image behind a login simply fails there. kp-soft was the first
    /// stack to need one, and on 2026-08-31 it was solved by hand — a single
    /// `docker login` on the container that no manifest recorded. A container
    /// rebuilt from scratch would have failed at the pull with nothing to say
    /// why, which is the same shape as every other finding of that day: a
    /// step that exists only in someone's memory.
    #[serde(default)]
    pub registry_login: Option<RegistryLogin>,
}

/// Where the credentials for a private registry come from.
///
/// Deliberately no new secrets channel: the values ride in an app's ordinary
/// `.env`, which already travels from latch through the host vault to the
/// container. One mechanism, already proven, already backed up.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryLogin {
    /// The registry host, e.g. `ghcr.io`.
    pub registry: String,
    /// The app whose `.env` carries `REGISTRY_USER` and `REGISTRY_TOKEN`.
    pub app: String,
}

impl StackManifest {
    pub fn canonical_hostname(&self) -> String {
        format!("{}-app-{}", self.vmid, self.stack_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSpec {
    /// e.g. "10.10.10.10/24"
    pub ip: String,
    pub gateway: String,
    #[serde(default = "default_bridge")]
    pub bridge: String,
    #[serde(default)]
    pub vlan: Option<u16>,
}

fn default_bridge() -> String {
    "vmbr0".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cores: u16,
    pub memory_mb: u32,
    #[serde(default)]
    pub swap_mb: u32,
    pub disk_gb: u32,
    #[serde(default = "default_storage")]
    pub storage: String,
}

fn default_storage() -> String {
    "local-lvm".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LxcSpec {
    pub template: String,
    #[serde(default = "yes")]
    pub unprivileged: bool,
    #[serde(default = "default_features")]
    pub features: String,
    /// Proxmox protection flag: hypervisor refuses destroy while set.
    #[serde(default)]
    pub protection: bool,
    /// H4: pass the host GPU (/dev/dri) into the container (VAAPI).
    #[serde(default)]
    pub gpu: bool,
    /// H4: give the container a /dev/net/tun device (VPN clients).
    #[serde(default)]
    pub vpn: bool,
}

fn yes() -> bool {
    true
}
fn default_features() -> String {
    "nesting=1,keyctl=1".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSpec {
    #[serde(default = "yes")]
    pub onboot: bool,
    #[serde(default)]
    pub order: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub host_path: String,
    pub mount_point: String,
    #[serde(default)]
    pub host_owner_uid: Option<u32>,
    /// D25: which app owns this directory. The restic repository is named
    /// after the owner, so an app that moves to another stack keeps its whole
    /// backup history — which is the point: before this, the repository was
    /// named after the STACK, and moving an app meant starting from nothing.
    /// Absent = the stack owns it (host-level paths with no single app).
    #[serde(default)]
    pub app: Option<String>,
}

/// A bind mount of a directory the orchestrator does not own (M1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataMount {
    /// The path on the Proxmox host. Must already exist; the orchestrator
    /// never creates it.
    pub host_path: String,
    /// Where it appears inside the container. This is part of the
    /// application's configuration — every library path in the *arr
    /// databases and every Jellyfin library points at it — so it is frozen
    /// across a rebuild, not chosen fresh.
    pub mount_point: String,
    /// Free-text note on whose directory this is, kept in the manifest so
    /// the next reader does not have to work it out.
    #[serde(default)]
    pub note: Option<String>,
}

impl MountSpec {
    /// The restic repository this path belongs to: its app, or the stack.
    pub fn owner<'a>(&'a self, stack_name: &'a str) -> &'a str {
        self.app.as_deref().unwrap_or(stack_name)
    }
}

// ── Deploy payload ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBlob {
    /// Relative to the stack root, e.g. "syncthing/docker-compose.yml".
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

/// The only cross-stack write the system allows: one traefik route fragment
/// into the gateway's watched routes directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRoute {
    pub gateway_vmid: u16,
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySpec {
    pub manifest: StackManifest,
    pub files: Vec<FileBlob>,
    /// Per-app .env content — the secrets channel; never enters git (A5).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub gateway_route: Option<GatewayRoute>,
    /// J1-J3: per-app health checks, keyed by app name. Orchestrator input
    /// like the manifest itself — it never enters the container, because what
    /// "healthy" means is the homelab's question about the service rather
    /// than something the service needs to be told.
    #[serde(default)]
    pub checks: BTreeMap<String, crate::checks::ServiceChecks>,
}

// ── Validation (D10) ─────────────────────────────────────────────────────────

/// Validate a full deploy spec. Returns every problem found (not just the
/// first) so the wizard can show them all at once.
/// Manifest-only validation (D10): the checks every manifest-bearing RPC
/// must pass — deploy adds file/env checks on top. Security review 2026-08-11:
/// app names reach `sh -c` strings inside containers, so they are constrained
/// to the same [a-z0-9-] alphabet as stack names, and this validator now runs
/// for backup/restore/update/resize too, not just deploy.
pub fn validate_manifest(m: &StackManifest) -> Result<(), CoreError> {
    let mut problems: Vec<String> = Vec::new();
    collect_manifest_problems(m, &mut problems);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CoreError::Validation(problems.join("; ")))
    }
}

fn collect_manifest_problems(m: &StackManifest, problems: &mut Vec<String>) {
    for app in &m.apps {
        if app.is_empty()
            || !app
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            problems.push(format!(
                "app name '{}' must be non-empty lowercase [a-z0-9-] (it is used in shell paths)",
                app
            ));
        }
    }
}

pub fn validate(spec: &DeploySpec) -> Result<(), CoreError> {
    let mut problems: Vec<String> = Vec::new();
    let m = &spec.manifest;
    collect_manifest_problems(m, &mut problems);

    if m.stack_name.is_empty()
        || !m
            .stack_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        problems.push(format!(
            "stack_name '{}' must be non-empty lowercase [a-z0-9-]",
            m.stack_name
        ));
    }
    if !(100..=354).contains(&m.vmid) {
        problems.push(format!("vmid {} outside the allowed range 100-354", m.vmid));
    }
    if m.hostname != m.canonical_hostname() {
        problems.push(format!(
            "hostname '{}' must be canonical '{}'",
            m.hostname,
            m.canonical_hostname()
        ));
    }
    if !m.network.ip.contains('/') {
        problems.push(format!(
            "network.ip '{}' must be CIDR (e.g. 10.10.10.10/24)",
            m.network.ip
        ));
    }
    if m.resources.memory_mb < 128 {
        problems.push(format!(
            "memory_mb {} below the sane floor of 128",
            m.resources.memory_mb
        ));
    }
    if m.resources.cores == 0 {
        problems.push("cores must be at least 1".into());
    }
    if m.resources.disk_gb < 2 {
        problems.push(format!(
            "disk_gb {} below the sane floor of 2",
            m.resources.disk_gb
        ));
    }
    if m.native_only && m.natives.is_empty() {
        problems.push(
            "native_only is set but no natives are declared :: name the systemd units this \
             container runs, so the deploy knows what to install and a rebuild is not guesswork"
                .into(),
        );
    }
    if m.apps.is_empty() && !m.native_only {
        problems.push(
            "a stack needs at least one app :: a container that deliberately runs no docker \
             says so with `native_only: true`, so that an empty list is never mistaken for a \
             list somebody forgot to fill in"
                .into(),
        );
    }
    if !m.apps.is_empty() && m.native_only {
        problems.push(format!(
            "native_only is set but the stack declares apps ({}) :: one of the two is wrong",
            m.apps.join(", ")
        ));
    }
    // M1: the two lists must stay distinct. A directory the orchestrator owns
    // belongs in `storage:` and gets the strict rules; a directory it merely
    // borrows belongs in `data_mounts:` and gets none of them. Blurring the
    // two is how the strict rule quietly stops meaning anything.
    for dm in &m.data_mounts {
        if !dm.host_path.starts_with('/') || !dm.mount_point.starts_with('/') {
            problems.push(format!(
                "data_mounts '{}' -> '{}': both paths must be absolute",
                dm.host_path, dm.mount_point
            ));
        }
        if dm.host_path.starts_with("/appdata/") {
            problems.push(format!(
                "data_mounts host_path '{}' is under /appdata/, which is where the \
                 directories this stack OWNS live :: declare it under storage: instead, \
                 so it is created, backed up and restored",
                dm.host_path
            ));
        }
        if m.storage.iter().any(|s| s.mount_point == dm.mount_point) {
            problems.push(format!(
                "mount_point '{}' is claimed by both storage: and data_mounts:",
                dm.mount_point
            ));
        }
    }
    for mount in &m.storage {
        if !mount.host_path.starts_with("/appdata/") {
            problems.push(format!(
                "storage host_path '{}' must live under /appdata/",
                mount.host_path
            ));
        }
        if !mount.mount_point.starts_with('/') {
            problems.push(format!(
                "mount_point '{}' must be absolute",
                mount.mount_point
            ));
        }
        // O5: an unprivileged LXC maps uid N inside to N+100000 on the host,
        // a privileged one maps it to itself. The wrong number produces a
        // directory the service cannot use while the deploy reports success,
        // and the app simply does not start.
        // O7: a config directory is named after the app that owns it. The
        // restore looks paths up by name, so a directory renamed on a whim
        // loses track of its own snapshots — and a rule nothing enforced is
        // how this drifted into three shapes across two stacks. Kenny chose
        // the literal rule at the mini-round over a weaker one that would
        // have tolerated the drift.
        if let Some(dir) = mount.host_path.rsplit('/').next() {
            if !dir.ends_with("-config") {
                let owner = mount.owner(&m.stack_name);
                problems.push(format!(
                    "storage '{}' must be named '{}-config' — the restore finds a path by its name",
                    mount.host_path, owner
                ));
            } else if let Some(app) = &mount.app {
                let expected = format!("{}-config", app);
                if dir != expected {
                    problems.push(format!(
                        "storage '{}' is owned by app '{}' but is not named '{}' — the name and the owner must agree, or the directory says one thing and the backup does another",
                        mount.host_path, app, expected
                    ));
                }
            }
        }
        if let Some(app) = &mount.app {
            // A directory may belong to a docker app or to a native systemd
            // unit: both are things this stack runs, and both want their own
            // restic repository named after them (D25).
            if !m.apps.contains(app) && !m.natives.contains(app) {
                problems.push(format!(
                    "storage '{}' is owned by '{}', which this stack declares neither as an app nor as a native unit",
                    mount.host_path, app
                ));
            }
        }
        if let Some(uid) = mount.host_owner_uid {
            const MAP: u32 = 100_000;
            if m.lxc.unprivileged && uid < MAP {
                problems.push(format!(
                    "storage host_owner_uid {} on unprivileged stack '{}': uid {} inside the container is {} on the host — use {}",
                    uid, m.stack_name, uid, uid + MAP, uid + MAP
                ));
            }
            if !m.lxc.unprivileged && uid >= MAP {
                problems.push(format!(
                    "storage host_owner_uid {} on PRIVILEGED stack '{}': there is no id mapping, so the host uid is the container uid — use {}",
                    uid, m.stack_name, uid - MAP
                ));
            }
        }
    }
    // A service's checks are refused before they are ever run when they
    // cannot answer the question they exist for. Kenny's challenge: this was
    // called discipline, and two thirds of it did not have to be.
    for (app, sc) in &spec.checks {
        for problem in crate::checks::shortcomings(sc) {
            problems.push(format!("checks for '{}': {}", app, problem));
        }
    }

    for f in &spec.files {
        if f.path.contains("..") || f.path.starts_with('/') {
            problems.push(format!("file path '{}' escapes the stack root", f.path));
        }
    }
    for app in spec.env.keys() {
        if !m.apps.contains(app) {
            problems.push(format!("env provided for unknown app '{}'", app));
        }
    }
    if let Some(r) = &m.registry_login {
        if !m.apps.contains(&r.app) {
            problems.push(format!(
                "registry_login reads its credentials from app '{}', which this stack does not declare",
                r.app
            ));
        }
        if r.registry.is_empty() || r.registry.contains('/') {
            problems.push(format!(
                "registry_login.registry '{}' must be a bare host like 'ghcr.io'",
                r.registry
            ));
        }
    }
    if let Some(route) = &spec.gateway_route {
        if route.filename.contains('/')
            || route.filename.contains("..")
            || !route.filename.ends_with(".yml")
        {
            problems.push(format!(
                "gateway route filename '{}' invalid",
                route.filename
            ));
        }
    }

    // Every /appdata bind in a compose file must be declared as manifest
    // storage — otherwise docker silently creates the dir on the container
    // rootfs: unbacked-up and lost on destroy (the synctest-108 bug class).
    let declared: Vec<&str> = m.storage.iter().map(|s| s.host_path.as_str()).collect();
    for f in &spec.files {
        if !f.path.ends_with("docker-compose.yml") {
            continue;
        }
        for line in f.content.lines() {
            let t = line
                .trim()
                .trim_start_matches("- ")
                .trim_matches(['"', '\'']);
            if let Some(host) = t.split(':').next() {
                // A path INSIDE a declared mount is covered by that mount:
                // the bytes land on the host exactly as intended, which is
                // the only thing this check is about. Requiring an exact
                // match refused a perfectly safe layout — the pull-through
                // cache keeps one directory per upstream registry under a
                // single declared mount, and O7 would have forced four apps
                // to express that.
                let covered = declared.contains(&host)
                    || declared
                        .iter()
                        .any(|d| host.starts_with(&format!("{}/", d)));
                if host.starts_with("/appdata/") && !covered {
                    problems.push(format!(
                        "{}: bind '{}' is not declared under storage: — data would land on the container rootfs (add a storage entry or remove the bind)",
                        f.path, host
                    ));
                }
            }
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(CoreError::Validation(problems.join("; ")))
    }
}

/// B4: deterministic fingerprint of a deploy's full intent — manifest,
/// files, env — so the client can compare its local stack directory against
/// what the host last applied. Any byte of difference flips the hash.
pub fn intent_hash(spec: &DeploySpec) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    // Manifest via canonical JSON (BTreeMap-backed serde keeps field order).
    if let Ok(m) = serde_json::to_vec(&spec.manifest) {
        h.update(&m);
    }
    let mut files: Vec<_> = spec.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for f in files {
        h.update(f.path.as_bytes());
        h.update([0]);
        h.update(f.content.as_bytes());
        h.update([0]);
    }
    for (app, env) in &spec.env {
        h.update(app.as_bytes());
        h.update([0]);
        h.update(env.as_bytes());
        h.update([0]);
    }
    let out = h.finalize();
    out.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// Hex sha256 of arbitrary bytes (shared by push staging + release verify).
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}
