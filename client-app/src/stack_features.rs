use serde_yaml::{Mapping, Value};
use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const CORE_APPS: [&str; 3] = ["promtail", "watchtower", "traefik"];
const GPU_NODES_INTEL: [&str; 2] = ["/dev/dri/renderD128", "/dev/dri/card0"];

pub struct AddAppOptions {
    pub include_promtail: bool,
    pub include_traefik: bool,
    pub subdomain: Option<String>,
}

pub struct AddCoreAppsResult {
    pub added: Vec<String>,
}

#[allow(dead_code)]
pub struct CreateStackResult {
    pub core_apps_added: Vec<String>,
}
pub struct PostDeploySummary {
    pub app_count: usize,
    pub missing_compose: Vec<String>,
}

pub fn is_core_app(app_name: &str) -> bool {
    CORE_APPS.contains(&app_name)
}

pub fn create_stack(stack_name: &str) -> io::Result<CreateStackResult> {
    if !is_valid_stack_name(stack_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid stack name: use lowercase letters, numbers and hyphens",
        ));
    }

    let stack_dir = format!("stacks/{}", stack_name);
    if Path::new(&stack_dir).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "stack already exists",
        ));
    }

    fs::create_dir_all(&stack_dir)?;
    write_stack_setup_script(stack_name)?;

    crate::scaffold::ensure_lxc_compose(stack_name)?;
    let added = add_missing_core_apps(stack_name)?.added;

    Ok(CreateStackResult {
        core_apps_added: added,
    })
}

pub fn delete_stack(stack_name: &str) -> io::Result<()> {
    let stack_dir = format!("stacks/{}", stack_name);
    if Path::new(&stack_dir).exists() {
        fs::remove_dir_all(stack_dir)?;
    }
    Ok(())
}

pub fn validate_stack_filesystem_layout(stack_name: &str) -> io::Result<()> {
    let stack_dir = format!("stacks/{}", stack_name);
    if !Path::new(&stack_dir).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "stack directory is missing",
        ));
    }

    let lxc_compose = format!("{}/lxc-compose.yml", stack_dir);
    if !Path::new(&lxc_compose).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "lxc-compose.yml is missing",
        ));
    }

    for app in crate::app_list::list_apps_for_stack(stack_name) {
        let compose_path = format!("stacks/{}/{}/docker-compose.yml", stack_name, app);
        if !Path::new(&compose_path).exists() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing docker-compose.yml for app '{}'", app),
            ));
        }
    }

    Ok(())
}

pub fn post_deploy_summary(stack_name: &str) -> PostDeploySummary {
    let apps = crate::app_list::list_apps_for_stack(stack_name);
    let mut missing_compose = Vec::new();

    for app in &apps {
        let compose_path = format!("stacks/{}/{}/docker-compose.yml", stack_name, app);
        if !Path::new(&compose_path).exists() {
            missing_compose.push(app.clone());
        }
    }

    PostDeploySummary {
        app_count: apps.len(),
        missing_compose,
    }
}

pub fn set_gpu_passthrough_for_app(
    stack_name: &str,
    app_name: &str,
    enabled: bool,
) -> io::Result<()> {
    let compose_path = format!("stacks/{}/{}/docker-compose.yml", stack_name, app_name);
    let raw = fs::read_to_string(&compose_path)?;
    let mut doc: Value = serde_yaml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid compose root"))?;

    let services_key = Value::String("services".to_string());
    let services = root
        .get_mut(&services_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing services block"))?;

    let app_key = if services.contains_key(Value::String(app_name.to_string())) {
        Value::String(app_name.to_string())
    } else {
        services
            .keys()
            .find_map(|k| k.as_str().map(|s| Value::String(s.to_string())))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no service found"))?
    };

    let service = services
        .get_mut(&app_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid service block"))?;

    apply_gpu_compose_fields(service, app_name, enabled);
    set_gpu_host_hint(stack_name, app_name, enabled)?;

    let serialized = serde_yaml::to_string(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(compose_path, serialized)
}

fn apply_gpu_compose_fields(service: &mut Mapping, app_name: &str, enabled: bool) {
    let devices_key = Value::String("devices".to_string());
    let group_key = Value::String("group_add".to_string());
    let env_key = Value::String("environment".to_string());

    if enabled {
        let device_values = GPU_NODES_INTEL
            .iter()
            .map(|n| Value::String(format!("{}:{}", n, n)))
            .collect::<Vec<_>>();
        service.insert(devices_key, Value::Sequence(device_values));

        service.insert(
            group_key,
            Value::Sequence(vec![
                Value::String("104".to_string()),
                Value::String("44".to_string()),
            ]),
        );

        if app_name.contains("jellyfin") {
            let entry =
                Value::String("DOCKER_MODS=linuxserver/mods:jellyfin-opencl-intel".to_string());
            match service.get_mut(&env_key) {
                Some(Value::Sequence(seq)) => {
                    if !seq.iter().any(|v| v == &entry) {
                        seq.push(entry);
                    }
                }
                Some(Value::Mapping(map)) => {
                    map.insert(
                        Value::String("DOCKER_MODS".to_string()),
                        Value::String("linuxserver/mods:jellyfin-opencl-intel".to_string()),
                    );
                }
                _ => {
                    service.insert(env_key, Value::Sequence(vec![entry]));
                }
            }
        }
    } else {
        service.remove(&devices_key);
        service.remove(&group_key);

        if let Some(env) = service.get_mut(&env_key) {
            match env {
                Value::Sequence(seq) => {
                    seq.retain(|v| {
                        v.as_str()
                            .map(|s| {
                                !s.starts_with("DOCKER_MODS=linuxserver/mods:jellyfin-opencl-intel")
                            })
                            .unwrap_or(true)
                    });
                }
                Value::Mapping(map) => {
                    map.remove(Value::String("DOCKER_MODS".to_string()));
                }
                _ => {}
            }
        }
    }
}

fn set_gpu_host_hint(stack_name: &str, app_name: &str, enabled: bool) -> io::Result<()> {
    let path = format!("stacks/{}/lxc-compose.yml", stack_name);
    if !Path::new(&path).exists() {
        return Ok(());
    }

    let raw = fs::read_to_string(&path)?;
    let mut doc: Value = serde_yaml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    if !doc.is_mapping() {
        doc = Value::Mapping(Mapping::new());
    }
    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    let hw_key = Value::String("hardware".to_string());
    if !root.contains_key(&hw_key) {
        root.insert(hw_key.clone(), Value::Mapping(Mapping::new()));
    }
    let hw = root
        .get_mut(&hw_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid hardware block"))?;

    let gpu_key = Value::String("gpu".to_string());
    if enabled {
        let mut gpu = Mapping::new();
        gpu.insert(Value::String("enabled".to_string()), Value::Bool(true));
        gpu.insert(
            Value::String("profile".to_string()),
            Value::String("intel_igpu".to_string()),
        );
        gpu.insert(
            Value::String("target_app".to_string()),
            Value::String(app_name.to_string()),
        );
        hw.insert(gpu_key, Value::Mapping(gpu));
    } else {
        hw.remove(&gpu_key);
    }

    let serialized = serde_yaml::to_string(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(path, serialized)
}

pub fn add_app_to_stack(
    stack_name: &str,
    app_name: &str,
    docker_image: &str,
    options: &AddAppOptions,
) -> io::Result<()> {
    create_app_dirs(stack_name, app_name)?;
    create_app_config_dir(stack_name, app_name)?;

    crate::scaffold::ensure_lxc_compose(stack_name)?;
    crate::scaffold::ensure_app_config_mount(stack_name, app_name)?;

    let domain = std::env::var("DOMAIN").unwrap_or_else(|_| "example.com".to_string());
    let compose = app_compose_yaml(stack_name, app_name, docker_image, options, &domain);
    fs::write(
        format!("stacks/{}/{}/docker-compose.yml", stack_name, app_name),
        compose,
    )?;

    Ok(())
}

pub fn read_app_docker_image(stack_name: &str, app_name: &str) -> io::Result<String> {
    let compose_path = format!("stacks/{}/{}/docker-compose.yml", stack_name, app_name);
    let raw = fs::read_to_string(&compose_path)?;
    let doc: Value = serde_yaml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let root = doc
        .as_mapping()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid compose root"))?;
    let services = root
        .get(Value::String("services".to_string()))
        .and_then(Value::as_mapping)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing services block"))?;

    let service_key = if services.contains_key(Value::String(app_name.to_string())) {
        Value::String(app_name.to_string())
    } else {
        services
            .keys()
            .find_map(|key| key.as_str().map(|value| Value::String(value.to_string())))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no service found"))?
    };

    services
        .get(&service_key)
        .and_then(Value::as_mapping)
        .and_then(|service| service.get(Value::String("image".to_string())))
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing image field"))
}

pub fn set_app_docker_image(
    stack_name: &str,
    app_name: &str,
    docker_image: &str,
) -> io::Result<()> {
    let compose_path = format!("stacks/{}/{}/docker-compose.yml", stack_name, app_name);
    let raw = fs::read_to_string(&compose_path)?;
    let mut doc: Value = serde_yaml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid compose root"))?;
    let services = root
        .get_mut(Value::String("services".to_string()))
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing services block"))?;

    let service_key = if services.contains_key(Value::String(app_name.to_string())) {
        Value::String(app_name.to_string())
    } else {
        services
            .keys()
            .find_map(|key| key.as_str().map(|value| Value::String(value.to_string())))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no service found"))?
    };

    let service = services
        .get_mut(&service_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid service block"))?;
    service.insert(
        Value::String("image".to_string()),
        Value::String(docker_image.to_string()),
    );

    let serialized = serde_yaml::to_string(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(compose_path, serialized)
}

pub fn delete_app_from_stack(stack_name: &str, app_name: &str) -> io::Result<()> {
    if is_core_app(app_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "core app must be removed with delete-core-app flow",
        ));
    }

    let app_dir = format!("stacks/{}/{}", stack_name, app_name);
    let app_cfg_dir = format!("stacks/{}/{}-config", stack_name, app_name);

    if Path::new(&app_dir).exists() {
        fs::remove_dir_all(&app_dir)?;
    }
    if Path::new(&app_cfg_dir).exists() {
        fs::remove_dir_all(&app_cfg_dir)?;
    }

    let _ = crate::scaffold::remove_app_config_mount(stack_name, app_name);
    Ok(())
}

pub fn delete_core_app_from_stack(stack_name: &str, app_name: &str) -> io::Result<()> {
    if !is_core_app(app_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "non-core app cannot use delete-core-app flow",
        ));
    }

    let app_dir = format!("stacks/{}/{}", stack_name, app_name);
    let app_cfg_dir = format!("stacks/{}/{}-config", stack_name, app_name);

    if Path::new(&app_dir).exists() {
        fs::remove_dir_all(&app_dir)?;
    }
    if Path::new(&app_cfg_dir).exists() {
        fs::remove_dir_all(&app_cfg_dir)?;
    }

    let _ = crate::scaffold::remove_app_config_mount(stack_name, app_name);
    Ok(())
}

pub fn add_missing_core_apps(stack_name: &str) -> io::Result<AddCoreAppsResult> {
    let mut added = Vec::new();
    crate::scaffold::ensure_lxc_compose(stack_name)?;

    if !core_app_exists(stack_name, "promtail") {
        scaffold_promtail(stack_name)?;
        crate::scaffold::ensure_app_config_mount(stack_name, "promtail")?;
        added.push("promtail".to_string());
    }
    if !core_app_exists(stack_name, "watchtower") {
        scaffold_watchtower(stack_name)?;
        added.push("watchtower".to_string());
    }
    if !core_app_exists(stack_name, "traefik") {
        scaffold_traefik(stack_name)?;
        crate::scaffold::ensure_app_config_mount(stack_name, "traefik")?;
        added.push("traefik".to_string());
    }

    Ok(AddCoreAppsResult { added })
}

fn create_app_dirs(stack_name: &str, app_name: &str) -> io::Result<()> {
    fs::create_dir_all(format!("stacks/{}/{}", stack_name, app_name))
}

fn write_stack_setup_script(stack_name: &str) -> io::Result<()> {
    let path = format!("stacks/{}/setup.sh", stack_name);
    if Path::new(&path).exists() {
        return Ok(());
    }

    fs::write(
        &path,
        "#!/usr/bin/env bash\nset -euo pipefail\n\n# Pre-sync hook scaffold for stack deployment.\n# Keep this script idempotent and fast (<30s).\n# Do not create /appdata or /opt/appdata paths here.\n\nexit 0\n",
    )?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

pub fn validate_setup_hook(stack_name: &str) -> io::Result<()> {
    let path = format!("stacks/{}/setup.sh", stack_name);
    if !Path::new(&path).exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&path)?;
    let first_line = content.lines().next().unwrap_or_default().trim();
    if first_line != "#!/usr/bin/env bash" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "setup.sh must start with #!/usr/bin/env bash",
        ));
    }

    let forbidden = ["mkdir -p /appdata", "mkdir -p /opt/appdata", " pre-sync.sh"];
    for token in forbidden {
        if content.contains(token) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("setup.sh contains forbidden pattern: {}", token),
            ));
        }
    }

    Ok(())
}

fn create_app_config_dir(stack_name: &str, app_name: &str) -> io::Result<()> {
    let dir = format!("stacks/{}/{}-config", stack_name, app_name);
    fs::create_dir_all(&dir)?;
    fs::write(format!("{}/.gitkeep", dir), "")
}

fn core_app_exists(stack_name: &str, app_name: &str) -> bool {
    Path::new(&format!("stacks/{}/{}", stack_name, app_name)).exists()
}

fn is_valid_stack_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 29 {
        return false;
    }

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }

    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn app_compose_yaml(
    stack_name: &str,
    app_name: &str,
    docker_image: &str,
    options: &AddAppOptions,
    domain: &str,
) -> String {
    let mut out = String::new();
    out.push_str("services:\n");
    out.push_str(&format!("  {}:\n", app_name));
    out.push_str(&format!("    image: {}\n", docker_image));
    out.push_str("    restart: unless-stopped\n");
    out.push_str("    volumes:\n");
    out.push_str(&format!("      - /appdata/{}-config:/config\n", app_name));

    if options.include_traefik {
        out.push_str("    labels:\n");
        out.push_str("      - \"traefik.enable=true\"\n");
        if let Some(subdomain) = &options.subdomain {
            let fqdn = format!("{}.{}", subdomain, domain);
            out.push_str(&format!(
                "      - \"traefik.http.routers.{}.rule=Host(\\\"{}\\\") || Host(\\\"{}\\\") || Host(\\\"{}\\\") || Host(\\\"{}\\\")\"\n",
                app_name, 
                fqdn,
                format!("{}.{}", subdomain, domain), // FQDN
                subdomain,
                app_name
            ));
        } else {
            out.push_str(&format!(
                "      - \"traefik.http.routers.{}.rule=Host(\\\"{}.local\\\")\"\n",
                app_name, app_name
            ));
        }
        out.push_str("      - \"traefik.http.services.app.loadbalancer.server.port=80\"\n");
    }

    if options.include_promtail {
        out.push_str("\n  promtail:\n");
        out.push_str("    image: grafana/promtail:latest\n");
        out.push_str("    restart: unless-stopped\n");
        out.push_str("    volumes:\n");
        out.push_str("      - /var/log:/var/log:ro\n");
        out.push_str("      - /var/run/docker.sock:/var/run/docker.sock:ro\n");
        out.push_str("      - /appdata/promtail-config:/etc/promtail\n");
        out.push_str(
            "    command: -config.file=/etc/promtail/config.yml -config.expand-env=true\n",
        );
    }

    out.push_str("\nnetworks:\n");
    out.push_str("  default:\n");
    out.push_str(&format!("    name: {}_{}\n", stack_name, app_name));
    out
}

fn scaffold_promtail(stack_name: &str) -> io::Result<()> {
    let app_dir = format!("stacks/{}/promtail", stack_name);
    let cfg_dir = format!("stacks/{}/promtail-config", stack_name);
    fs::create_dir_all(&app_dir)?;
    fs::create_dir_all(&cfg_dir)?;

    fs::write(
        format!("{}/docker-compose.yml", app_dir),
        format!(
            "services:\n  promtail:\n    image: grafana/promtail:latest\n    container_name: {}-promtail\n    restart: unless-stopped\n    volumes:\n      - /var/log:/var/log:ro\n      - /var/run/docker.sock:/var/run/docker.sock:ro\n      - /appdata/promtail-config:/etc/promtail\n    env_file:\n      - .env\n    command: -config.file=/etc/promtail/config.yml -config.expand-env=true\n    labels:\n      com.centurylinklabs.watchtower.enable: \"true\"\n",
            stack_name
        ),
    )?;

    fs::write(
        format!("{}/config.yml", cfg_dir),
        format!(
            "server:\n  http_listen_port: 9080\n  grpc_listen_port: 0\n\npositions:\n  filename: /tmp/positions.yaml\n\nclients:\n  - url: ${{LOKI_URL}}/loki/api/v1/push\n\nscrape_configs:\n  - job_name: docker\n    static_configs:\n      - targets: [localhost]\n        labels:\n          job: docker\n          stack: {}\n          __path__: /var/log/docker/*.log\n",
            stack_name
        ),
    )?;

    // Subscribe-intent member of the latch promtail_config group.
    // The runtime .env is written to /appdata/ by pre-sync.sh via latch sync — this file is never read by Docker.
    fs::write(
        format!("{}/.env", app_dir),
        "# latch:group=promtail_config\nLOKI_URL=\n",
    )?;

    ensure_shared_promtail_env_template()?;

    fs::write(format!("{}/.gitkeep", cfg_dir), "")
}

fn ensure_shared_promtail_env_template() -> io::Result<()> {
    let dir = "config/promtail";
    let path = format!("{}/.env", dir);
    fs::create_dir_all(dir)?;
    if Path::new(&path).exists() {
        return Ok(());
    }

    fs::write(
        path,
        "# Central Promtail configuration for all stacks.\n# This file is managed via latch group sync.\n# latch:group=promtail_config\n\n# Loki push endpoint (without /loki/api/v1/push).\n# Example: https://loki.kp-soft.dev\nLOKI_URL=\n",
    )
}

fn scaffold_watchtower(stack_name: &str) -> io::Result<()> {
    let app_dir = format!("stacks/{}/watchtower", stack_name);
    fs::create_dir_all(&app_dir)?;
    // Watchtower is not user-selectable anymore; every new stack gets it.
    fs::write(
        format!("{}/docker-compose.yml", app_dir),
        format!(
            "services:\n  watchtower:\n    image: containrrr/watchtower:latest\n    container_name: {}-watchtower\n    restart: unless-stopped\n    volumes:\n      - /var/run/docker.sock:/var/run/docker.sock\n    environment:\n      WATCHTOWER_LABEL_ENABLE: \"true\"\n      WATCHTOWER_CLEANUP: \"true\"\n      WATCHTOWER_POLL_INTERVAL: \"86400\"\n      WATCHTOWER_ROLLING_RESTART: \"true\"\n    labels:\n      com.centurylinklabs.watchtower.enable: \"true\"\n",
            stack_name
        ),
    )
}

fn scaffold_traefik(stack_name: &str) -> io::Result<()> {
    let app_dir = format!("stacks/{}/traefik", stack_name);
    let cfg_dir = format!("stacks/{}/traefik-config", stack_name);
    fs::create_dir_all(&app_dir)?;
    fs::create_dir_all(&cfg_dir)?;
    fs::create_dir_all(format!("{}/acme", cfg_dir))?;

    fs::write(
        format!("{}/docker-compose.yml", app_dir),
        format!(
            "services:\n  traefik:\n    image: traefik:v3\n    container_name: {}-traefik\n    restart: unless-stopped\n    ports:\n      - \"80:80\"\n      - \"443:443\"\n    volumes:\n      - /var/run/docker.sock:/var/run/docker.sock:ro\n      - /appdata/traefik-config:/etc/traefik\n      - /appdata/traefik-config/acme:/acme\n    labels:\n      com.centurylinklabs.watchtower.enable: \"true\"\n      traefik.enable: \"true\"\n",
            stack_name
        ),
    )?;

    fs::write(
        format!("{}/traefik.yml", cfg_dir),
        "providers:\n  docker:\n    exposedByDefault: false\nentryPoints:\n  web:\n    address: \":80\"\n  websecure:\n    address: \":443\"\ncertificatesResolvers:\n  letsencrypt:\n    acme:\n      email: ${ACME_EMAIL}\n      storage: /acme/acme.json\n      tlsChallenge: {}\n",
    )
}
