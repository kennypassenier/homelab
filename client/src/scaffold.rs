//! Scaffold a new stack directory (G2 / D7): writes a real, deployable
//! stacks/<name>/ tree — lxc-compose.yml manifest + a starter app compose +
//! the promtail core app (D8) — using the preset the wizard picked. Only the
//! per-VM values are substituted; the rest is the canonical template.

use std::path::{Path, PathBuf};

pub struct Scaffolded {
    pub dir: PathBuf,
    pub files: Vec<String>,
}

/// Create `base/<name>/` with a manifest, the preset's primary app, and
/// promtail. Returns the created paths. Errors if the dir already exists.
pub fn scaffold_stack(
    base: &Path,
    name: &str,
    vmid: u16,
    ram_mb: u32,
    preset_app: Option<(&str, &str)>, // (app name, image)
) -> Result<Scaffolded, String> {
    let dir = base.join(name);
    if dir.exists() {
        return Err(format!("stacks/{} already exists", name));
    }
    let ip_suffix = vmid.saturating_sub(100);
    let mut files = Vec::new();

    // lxc-compose.yml (schema v2, intent only).
    let mut apps = Vec::new();
    if preset_app.is_some() {
        apps.push(name.to_string());
    }
    apps.push("promtail".to_string());
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
         network:\n  ip: 10.10.10.{ip_suffix}/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\n\n\
         resources:\n  cores: 2\n  memory_mb: {ram_mb}\n  swap_mb: 512\n  disk_gb: 8\n  storage: local-lvm\n\n\
         lxc:\n  template: \"local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst\"\n  unprivileged: true\n  features: \"nesting=1,keyctl=1\"\n\n\
         boot:\n  onboot: true\n  order: 90\n\n\
         apps:\n{apps_yaml}\n"
    );
    write_file(&dir.join("lxc-compose.yml"), &manifest, &mut files)?;

    // Primary app compose (if the preset has one).
    if let Some((app, image)) = preset_app {
        let compose = format!(
            "services:\n  {app}:\n    image: {image}\n    container_name: {app}\n    restart: unless-stopped\n    labels:\n      - com.homelab.backup.pause=true\n      - com.centurylinklabs.watchtower.enable=true\n    networks:\n      - {name}_net\n\nnetworks:\n  {name}_net:\n    external: true\n    name: {name}_net\n"
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
