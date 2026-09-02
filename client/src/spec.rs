//! Build a DeploySpec from a local stack directory (shared by CLI and TUI).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use homelab_proto::{DeploySpec, FileBlob, GatewayRoute, StackManifest};

/// A typo used to be free. `latch_secret:` instead of `latch_secrets:` parsed
/// cleanly, deployed cleanly, and produced a container with no secrets in it;
/// `gateway_routes:` instead of `gateway_route:` produced a hostname with no
/// route. Both are the shape that cost the downloader its disks on
/// 2026-08-31 — a field the reader did not recognise and silently dropped.
///
/// `deny_unknown_fields` has to sit on the OUTER struct: `flatten` swallows
/// everything it does not recognise and hands it on, so the inner manifest can
/// never see a stray key.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackFile {
    #[serde(flatten)]
    manifest: StackManifest,
    #[serde(default)]
    gateway_route: Option<GatewayRouteFile>,
    /// D12: apps whose .env comes from latch instead of a plaintext file.
    /// Client-side sugar only — the wire and the host vault see the same
    /// env content either way.
    #[serde(default)]
    latch_secrets: Vec<String>,
}

#[derive(Deserialize)]
struct GatewayRouteFile {
    filename: String,
    #[serde(default = "default_gw")]
    gateway_vmid: u16,
}

fn default_gw() -> u16 {
    104
}

pub fn build_spec(dir: &Path) -> Result<DeploySpec, String> {
    let manifest_path = dir.join("lxc-compose.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    let stack_file: StackFile =
        serde_yaml::from_str(&raw).map_err(|e| format!("manifest parse: {}", e))?;

    let mut files: Vec<FileBlob> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut checks: BTreeMap<String, homelab_core::checks::ServiceChecks> = BTreeMap::new();
    collect(dir, dir, &mut files, &mut env, &mut checks)?;
    fetch_latch_secrets(
        dir,
        &stack_file.latch_secrets,
        &stack_file.manifest.apps,
        &mut env,
    )?;

    let gateway_route = match stack_file.gateway_route.as_ref() {
        Some(g) => {
            // The filename is declared here and independently DERIVED by
            // destroy (`<vmid>-app-<stack>.yml`), which has no access to this
            // field — the manifest that reaches the host does not carry it.
            // As long as the two can disagree, a stack that names its file
            // anything else deploys fine and leaves a router behind when it is
            // destroyed, still answering for a hostname that has moved. That
            // is F115 exactly. Requiring them to agree removes the class
            // rather than the symptom.
            let derived = format!(
                "{}-app-{}.yml",
                stack_file.manifest.vmid, stack_file.manifest.stack_name
            );
            if g.filename != derived {
                return Err(format!(
                    "gateway_route.filename is '{}' but destroy removes '{}' — \
                     they must match, or the route outlives the stack",
                    g.filename, derived
                ));
            }
            let route_path = dir.join("traefik-routes.yml");
            let content = std::fs::read_to_string(&route_path)
                .map_err(|e| format!("gateway_route set but {}: {}", route_path.display(), e))?;
            Some(GatewayRoute {
                gateway_vmid: g.gateway_vmid,
                filename: g.filename.clone(),
                content,
            })
        }
        None => None,
    };

    Ok(DeploySpec {
        manifest: stack_file.manifest,
        files,
        env,
        gateway_route,
        checks,
    })
}

fn collect(
    root: &Path,
    dir: &Path,
    files: &mut Vec<FileBlob>,
    env: &mut BTreeMap<String, String>,
    checks: &mut BTreeMap<String, homelab_core::checks::ServiceChecks>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            collect(root, &path, files, env, checks)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        // Orchestrator input, not container content: service.yml describes a
        // native unit to the homelab and has no business inside the container.
        if rel == "lxc-compose.yml" || rel == "traefik-routes.yml" || name == "service.yml" {
            continue;
        }
        // Same reason: checks.yml says what healthy MEANS for this service.
        // That is the homelab's question about the service, not something the
        // service has to be told, so it travels beside the spec rather than
        // into the container.
        if name == "checks.yml" {
            let app = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", rel, e))?;
            let parsed: homelab_core::checks::ServiceChecks =
                serde_yaml::from_str(&text).map_err(|e| format!("{}: {}", rel, e))?;
            checks.insert(app, parsed);
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|_| format!("non-utf8 file not supported: {}", rel))?;
        if name == ".env" {
            let app = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            env.insert(app, content);
        } else {
            files.push(FileBlob {
                path: rel,
                content,
                mode: None,
            });
        }
    }
    Ok(())
}

/// D12: fill in each declared app's env by asking latch, in memory only —
/// no plaintext .env needs to exist on the workstation. The latch project
/// root is the stacks/ directory (one `latch init` there, once), so the
/// path inside the project is `<stack>/<app>/.env`. `--expand` is
/// deliberate: docker compose does its own ${VAR} interpolation on .env
/// content, so raw latch templates would collide with it — latch resolves
/// them first or fails hard.
fn fetch_latch_secrets(
    dir: &Path,
    apps: &[String],
    manifest_apps: &[String],
    env: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    if apps.is_empty() {
        return Ok(());
    }
    // A latch_secrets entry that names no real app would fetch secrets into
    // the void; manifest app names are also what keeps the latch path free
    // of characters latch refuses (validated [a-z0-9-], so no '__').
    for app in apps {
        if !manifest_apps.contains(app) {
            return Err(format!(
                "latch_secrets names '{}' but the stack has no such app :: \
                 apps are [{}]",
                app,
                manifest_apps.join(", ")
            ));
        }
    }
    let latch_env = std::env::var("HOMELAB_LATCH_ENV").map_err(|_| {
        "latch_secrets is set but HOMELAB_LATCH_ENV is not :: set it to the \
         latch environment to read (e.g. HOMELAB_LATCH_ENV=prod in .env)"
            .to_string()
    })?;
    let stack = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "stack dir has no name".to_string())?;
    let project_root = dir.parent().unwrap_or(dir);
    for app in apps {
        // Two sources for the same app is ambiguity, not convenience: a
        // stale plaintext file silently shadowing latch (or the reverse)
        // is exactly the failure mode D12 exists to kill.
        if env.contains_key(app) {
            return Err(format!(
                "app '{}' has BOTH a plaintext .env and latch_secrets :: \
                 delete stacks/{}/{}/.env or drop '{}' from latch_secrets",
                app, stack, app, app
            ));
        }
        let rel = format!("{}/{}/.env", stack, app);
        let out = std::process::Command::new("latch")
            .args(["cat", &rel, "--env", &latch_env, "--expand"])
            .current_dir(project_root)
            .output()
            .map_err(|e| {
                format!(
                    "cannot run latch for app '{}': {} :: install latch (or \
                     remove latch_secrets from the stack file)",
                    app, e
                )
            })?;
        if !out.status.success() {
            return Err(format!(
                "latch cat {} --env {} failed: {}",
                rel,
                latch_env,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        // latch keeps content and messages strictly separated: informational
        // notes (e.g. the offline stale-cache notice) arrive on stderr with
        // exit 0 — pass them on rather than swallowing them.
        if !out.stderr.is_empty() {
            eprintln!("[latch] {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        let content = String::from_utf8(out.stdout)
            .map_err(|_| format!("latch returned non-utf8 content for '{}'", app))?;
        if content.trim().is_empty() {
            return Err(format!(
                "latch returned empty content for app '{}' ({} in env '{}') :: \
                 commit+push the env file in latch first",
                app, rel, latch_env
            ));
        }
        env.insert(app.clone(), content);
    }
    Ok(())
}

/// Scan a directory for deployable stack dirs (those with lxc-compose.yml).
pub fn scan_local_stacks(base: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("lxc-compose.yml").exists() {
                out.push((entry.file_name().to_string_lossy().to_string(), path));
            }
        }
    }
    out.sort();
    out
}
/// The repositories a stack's data actually lives in, named the way restic
/// names them. One per owning app, not one per stack — the runbook said the
/// latter and was wrong for every stack in the fleet.
fn restic_repos(m: &homelab_core::manifest::StackManifest) -> String {
    let groups = homelab_core::ops::backup::owner_groups(m);
    if groups.is_empty() {
        return "no /appdata paths — nothing to restore".to_string();
    }
    groups
        .iter()
        .map(|(owner, _)| format!("`…:{}-config`", owner))
        .collect::<Vec<_>>()
        .join(", ")
}

/// How this stack comes back, which is not the same command for every stack.
///
/// A native stack has no compose apps and `homelab deploy` is not its path:
/// kyu and almanac are systemd units, adopted rather than deployed. The
/// runbook said "deploy" for all thirteen — harmless while everything works,
/// and exactly the wrong instruction at the moment it is read.
fn recreate_line(dir_name: &str, m: &homelab_core::manifest::StackManifest) -> String {
    if m.apps.is_empty() {
        format!(
            "`homelab adopt stacks/{}` — native services; the unit files and \
             binaries come from the service's own release, not from a compose pull",
            dir_name
        )
    } else {
        format!(
            "`homelab deploy stacks/{}` (or by hand per Layer 2)",
            dir_name
        )
    }
}

/// E7: generate the disaster-recovery runbook from the local stacks dir.
/// Deliberately plain markdown with copy-pasteable commands — this document
/// must be useful when the TUI, the host daemon, or the whole host is down.
/// Returns the number of stacks included.
pub fn generate_runbook(stacks_dir: &Path, out_path: &str) -> Result<usize, String> {
    let stacks = scan_local_stacks(stacks_dir);
    let mut doc = String::new();
    doc.push_str(
        "# Disaster-recovery runbook\n\n\
         *Generated by `homelab runbook` — regenerate after every stack change.*\n\n\
         This document assumes the worst: the TUI is gone, the host daemon is\n\
         down, maybe the whole Proxmox host is fresh. Everything below is plain\n\
         shell on the Proxmox host (root) unless stated otherwise.\n\n\
         ## Layer 0 — What runs where\n\n\
         - Proxmox host `10.10.5.250` (ssh root, key auth).\n\
         - **Four** ZFS pools are attached to the Proxmox host, not two. This\n\
           line said two until 2026-09-02, and following it after a host loss\n\
           would have re-attached half the storage. Read off the machine that\n\
           day: `HDD12TB` (10.9T), `HDD18TB` (16.4T), `HDD4TB` (3.62T),\n\
           `HDD2TB` (1.81T).\n\
         - CT 103 (infra-fileserver), which shares everything over Samba, has\n\
           a subvolume on **three** of them: `HDD12TB/subvol-103-disk-0`\n\
           (8.40T), `HDD18TB/subvol-103-disk-0` (4.78T) and\n\
           `HDD4TB/subvol-103-disk-0` (45.3G) — 13.2 TB in total. `HDD2TB`\n\
           carries paperless's media and consume datasets. Losing a container\n\
           never loses this data; losing the host means re-attaching **all\n\
           four** pools first.\n\
         - `HDD18TB/replica/...` holds the E8 replicas of the 2TB and 4TB\n\
           pools; `HDD18TB/REPLICA_*` the frozen ones from the retired cron\n\
           script. A replica is a COPY and is never the thing to mount at a\n\
           live path — one of them was set to do exactly that (F177), so\n\
           check `canmount` before mounting any of them.\n\
         - App config data lives on the host under `/appdata/<stack>/…`,\n\
           bind-mounted into each container — it survives container recreation\n\
           and is what restic backs up.\n\
         - Host daemon: `homelab-host.service`, config `/etc/homelab/host.toml`\n\
           (token), state + git intent repo + incidents under `/var/lib/homelab`.\n\n\
         ## Layer 1 — Recover the host daemon\n\n\
         ```sh\n\
         systemctl status homelab-host       # is it running?\n\
         journalctl -u homelab-host -n 50    # why not?\n\
         curl -sk https://127.0.0.1:8443/api/health\n\
         ```\n\
         Reinstall if needed: build `homelab-host` (Debian 12 target), copy to\n\
         `/usr/local/bin/homelab-host`, keep the existing `/etc/homelab/host.toml`\n\
         and systemd unit, `systemctl daemon-reload && systemctl restart homelab-host`.\n\
         The TLS cert lives in `/var/lib/homelab` — keep it to keep the client pin.\n\n\
         ## Layer 2 — Recover a stack without the daemon\n\n\
         Every stack is just an LXC + docker compose files. By hand:\n\
         ```sh\n\
         pct list                              # what exists\n\
         pct start <vmid>\n\
         pct exec <vmid> -- sh -c 'cd /opt/<stack>/<app> && docker compose up -d'\n\
         ```\n\
         The exact files the daemon deployed are in git: `/var/lib/homelab/repo`.\n\n\
         ## Layer 3 — Restore data from backup\n\n\
         Restic repos are per OWNING APP, not per stack:\n\
         `rclone:gdrive:homelab-backups/<app>-config` — the exact names per stack are\n\
         listed below. Password\n\
         file `/var/lib/homelab/secrets/restic.pw` (keep an offline copy of this\n\
         password — without it backups are unreadable!).\n\
         ```sh\n\
         export RESTIC_REPOSITORY=rclone:gdrive:homelab-backups/<app>-config\n\
         export RESTIC_PASSWORD_FILE=/var/lib/homelab/secrets/restic.pw\n\
         restic snapshots\n\
         restic restore latest --target /\n\
         ```\n\n\
         ## Stacks\n\n",
    );
    let mut included = 0usize;
    for (name, path) in &stacks {
        let raw = std::fs::read_to_string(path.join("lxc-compose.yml"))
            .map_err(|e| format!("{}: {}", name, e))?;
        // Legacy v1 stacks in the same dir don't parse as v2 manifests —
        // list them as such rather than aborting the whole runbook.
        let m: homelab_core::manifest::StackManifest = match serde_yaml::from_str(&raw) {
            Ok(m) => m,
            Err(_) => {
                doc.push_str(&format!(
                    "### {} — LEGACY (v1 manifest, not deployable by v2)\n\n\
                     Recover by hand per Layer 2, or migrate it to a v2 stack first.\n\n",
                    name
                ));
                continue;
            }
        };
        included += 1;
        doc.push_str(&format!(
            "### {} (vmid {})\n\n\
             - hostname `{}`, ip `{}`\n\
             - resources: {} core(s), {} MiB RAM, {} MiB swap, {} GiB disk\n\
             - apps: {}\n\
             - recreate from scratch: {}\n\
             - data restore from: {}\n{}\n",
            m.stack_name,
            m.vmid,
            m.hostname,
            m.network.ip,
            m.resources.cores,
            m.resources.memory_mb,
            m.resources.swap_mb,
            m.resources.disk_gb,
            if m.apps.is_empty() {
                "none — native services under systemd, not compose".to_string()
            } else {
                m.apps.join(", ")
            },
            recreate_line(name, &m),
            restic_repos(&m),
            not_kept(&m),
        ));
    }
    doc.push_str(
        "## Full-host rebuild order\n\n\
         1. Install Proxmox, restore network config (vmbr0, VLAN 10).\n\
         2. Re-attach `/HDD12TB` + `/HDD18TB` mounts.\n\
         3. Restore `/etc/homelab/host.toml` + `/var/lib/homelab` (or accept a\n\
            new TLS cert + re-pin the client, and a fresh token).\n\
         4. Recreate no-touch guests from Proxmox backups (HA VM 101 first).\n\
         5. `homelab deploy` each stack above; restic restore fills the data.\n",
    );
    std::fs::write(out_path, &doc).map_err(|e| e.to_string())?;
    Ok(included)
}

/// Z3: what this stack deliberately does NOT keep, and why.
///
/// A runbook that lists only what can be restored quietly implies everything
/// else is covered. Naming the exceptions is the difference between reading
/// "the cache is not in the backup, on purpose, because it re-downloads" at
/// 3am and concluding the backup is broken.
fn not_kept(m: &homelab_core::manifest::StackManifest) -> String {
    let lines: Vec<String> = m
        .storage
        .iter()
        .filter_map(|s| {
            s.no_backup
                .as_ref()
                .map(|why| format!("- NOT backed up: `{}` — {}", s.host_path, why))
        })
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

// ── D11: stack export/import bundles ────────────────────────────────────────

/// A shareable single-file stack bundle: manifest + files, NEVER secrets.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Bundle {
    pub bundle_version: u32,
    pub exported_from: String,
    pub manifest: StackManifest,
    pub files: Vec<FileBlob>,
}

/// Export a stack directory to a single YAML bundle. `.env` files are
/// excluded by construction (build_spec routes them into `env`, which is
/// deliberately not part of the bundle).
pub fn export_bundle(dir: &Path, out_path: &str) -> Result<usize, String> {
    let spec = build_spec(dir)?;
    let bundle = Bundle {
        bundle_version: 1,
        exported_from: spec.manifest.stack_name.clone(),
        manifest: spec.manifest,
        files: spec.files,
    };
    let raw = serde_yaml::to_string(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(out_path, &raw).map_err(|e| e.to_string())?;
    Ok(bundle.files.len())
}

/// Import a bundle as a NEW stack: substitute the old stack identity (name,
/// vmid, hostname, ip, /appdata paths, _net network) for the new one, then
/// write a normal stack directory. Secrets must be added afterwards as
/// stacks/<name>/<app>/.env — they are never in a bundle.
pub fn import_bundle(
    bundle_path: &Path,
    stacks_dir: &Path,
    new_name: &str,
    new_vmid: u16,
) -> Result<PathBuf, String> {
    let raw = std::fs::read_to_string(bundle_path).map_err(|e| e.to_string())?;
    let bundle: Bundle = serde_yaml::from_str(&raw).map_err(|e| format!("bundle parse: {}", e))?;
    if bundle.bundle_version != 1 {
        return Err(format!(
            "unsupported bundle version {}",
            bundle.bundle_version
        ));
    }
    let dest = stacks_dir.join(new_name);
    if dest.exists() {
        return Err(format!("stacks/{} already exists", new_name));
    }
    let old = &bundle.exported_from;
    let old_vmid = bundle.manifest.vmid;
    let defaults = crate::scaffold::StackDefaults::default();
    let new_ip_host = format!("{}{}", defaults.ip_prefix, new_vmid.saturating_sub(100));

    // Manifest: identity fields + derived values + path renames.
    let mut m = bundle.manifest.clone();
    m.stack_name = new_name.to_string();
    m.vmid = new_vmid;
    m.hostname = format!("{}-app-{}", new_vmid, new_name);
    // Keep the CIDR suffix from the original ip.
    let cidr = m.network.ip.split('/').nth(1).unwrap_or("24").to_string();
    m.network.ip = format!("{}/{}", new_ip_host, cidr);
    for mount in m.storage.iter_mut() {
        mount.host_path = mount.host_path.replace(
            &format!("/appdata/{}/", old),
            &format!("/appdata/{}/", new_name),
        );
        mount.mount_point = mount.mount_point.replace(
            &format!("/appdata/{}/", old),
            &format!("/appdata/{}/", new_name),
        );
    }
    let manifest_yaml = serde_yaml::to_string(&m).map_err(|e| format!("manifest render: {}", e))?;
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("lxc-compose.yml"), manifest_yaml).map_err(|e| e.to_string())?;

    // Files: same substitutions inside content (D7 mechanics).
    let old_host = format!("{}-app-{}", old_vmid, old);
    for f in &bundle.files {
        let content = f
            .content
            .replace(&format!("{}_net", old), &format!("{}_net", new_name))
            .replace(
                &format!("/appdata/{}/", old),
                &format!("/appdata/{}/", new_name),
            )
            .replace(&old_host, &m.hostname);
        let path = dest.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Y4: every stack directory in the repository with the vmid it claims. Only
/// the client can see this — the host has the intent repo of what it actually
/// deployed, not the files sitting in front of the author.
pub fn stack_files_with_vmids(base: &str) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = dir.join("lxc-compose.yml");
        let native = dir.join("service.yml");
        let path = if manifest.exists() {
            manifest
        } else if native.exists() {
            native
        } else {
            continue;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Deliberately a line scan rather than a full parse: a file too broken
        // to deserialize is exactly the one worth reporting, and refusing to
        // look at it would hide it.
        if let Some(vmid) = raw
            .lines()
            .find_map(|l| l.trim().strip_prefix("vmid:"))
            .and_then(|v| v.trim().parse::<u16>().ok())
        {
            out.push((
                format!("{}/{}", base, entry.file_name().to_string_lossy()),
                vmid,
            ));
        }
    }
    out.sort();
    out
}

/// T71: every native service a stack directory declares, with the unit file
/// that makes it exist.
///
/// A native stack keeps one `service.yml` per unit, and where it sits depends
/// on how many there are: a stack with a single service puts it at the top
/// (`stacks/almanac/service.yml`), and a stack with several gives each its
/// own directory (`stacks/kyu/kyu-runner/service.yml`). Both shapes are real
/// on this fleet, so both are read here rather than in each caller.
///
/// The unit file is returned alongside because install-native cannot do
/// anything without it, and a service whose `.service` file is missing from
/// the repository is exactly the one worth reporting: it would rebuild into a
/// container that holds the program and nothing to run it.
pub fn native_services(
    dir: &std::path::Path,
) -> Vec<(homelab_proto::NativeServiceManifest, Option<String>)> {
    let mut out = Vec::new();
    let mut paths = vec![dir.join("service.yml")];
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut subs: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .map(|p| p.join("service.yml"))
            .filter(|p| p.exists())
            .collect();
        subs.sort();
        paths.extend(subs);
    }
    for path in paths {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(m) = serde_yaml::from_str::<homelab_proto::NativeServiceManifest>(&raw) else {
            continue;
        };
        let unit_name = format!("{}.service", m.unit);
        let parent = path.parent().unwrap_or(dir);
        let unit_file = [parent.join(&unit_name), dir.join(&m.unit).join(&unit_name)]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok());
        out.push((m, unit_file));
    }
    out.sort_by(|a, b| a.0.unit.cmp(&b.0.unit));
    out.dedup_by(|a, b| a.0.unit == b.0.unit);
    out
}
