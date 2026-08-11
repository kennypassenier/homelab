//! Build a DeploySpec from a local stack directory (shared by CLI and TUI).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use homelab_proto::{DeploySpec, FileBlob, GatewayRoute, StackManifest};

#[derive(Deserialize)]
struct StackFile {
    #[serde(flatten)]
    manifest: StackManifest,
    #[serde(default)]
    gateway_route: Option<GatewayRouteFile>,
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
    collect(dir, dir, &mut files, &mut env)?;

    let gateway_route = match stack_file.gateway_route.as_ref() {
        Some(g) => {
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
    })
}

fn collect(
    root: &Path,
    dir: &Path,
    files: &mut Vec<FileBlob>,
    env: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            collect(root, &path, files, env)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if rel == "lxc-compose.yml" || rel == "traefik-routes.yml" {
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
