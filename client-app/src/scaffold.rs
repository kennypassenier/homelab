#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_yaml::{Mapping, Value};

/// Holds the core template variables required to scaffold a new application
pub struct AppServiceTemplate<'a> {
    pub app_name: &'a str,
    pub mac_address: &'a str,
    pub domain_name: &'a str,
}

/// Creates the necessary directory structure for a new application within a stack
pub fn create_app_dirs(stack_dir: &str, app_name: &str) -> io::Result<()> {
    let path = format!("stacks/{}/{}", stack_dir, app_name);
    let dir_path = Path::new(&path);

    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    Ok(())
}

/// Generates a randomized MAC address for isolated container networking
pub fn generate_mac_address() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "02:42:ac:11:{:02x}:{:02x}",
        rng.gen_range(0..255),
        rng.gen_range(0..255)
    )
}

/// Returns the standard Watchtower docker-compose snippet
pub fn watchtower_service_yaml() -> &'static str {
    r#"
    watchtower:
        image: containrrr/watchtower
        volumes:
            - /var/run/docker.sock:/var/run/docker.sock
        command: --interval 86400
        restart: unless-stopped
"#
}

/// Returns the standard Promtail docker-compose snippet
pub fn promtail_service_yaml() -> &'static str {
    r#"
    promtail:
        image: grafana/promtail:latest
        volumes:
            - /var/log:/var/log
        restart: unless-stopped
"#
}

/// Returns the standard Traefik labels and network configuration snippet
pub fn traefik_service_yaml() -> &'static str {
    r#"
        labels:
            - "traefik.enable=true"
            - "traefik.http.routers.app.rule=Host(`${DOMAIN_NAME}`)"
            - "traefik.http.services.app.loadbalancer.server.port=80"
"#
}
// removed invalid YAML/config lines

/// Generate a full stack docker-compose.yml with selected default services
pub fn scaffold_stack_with_services(
    app: &AppServiceTemplate,
    include_watchtower: bool,
    include_promtail: bool,
    include_traefik: bool,
) -> String {
    let mut services = String::new();
    // Main app service
    services.push_str(&format!(
        "  {}:\n    image: nginx:latest\n    mac_address: {}\n",
        app.app_name, app.mac_address
    ));
    // Add selected defaults
    if include_watchtower {
        services.push_str(watchtower_service_yaml());
    }
    if include_promtail {
        services.push_str(promtail_service_yaml());
    }
    if include_traefik {
        services.push_str(traefik_service_yaml());
    }
    // Compose file header and networks
    services
}

/// Returns the path to the stack-level lxc-compose.yml file.
fn lxc_compose_path(stack_name: &str) -> String {
    format!("stacks/{}/lxc-compose.yml", stack_name)
}

fn load_lxc_compose(stack_name: &str) -> io::Result<Value> {
    let path = lxc_compose_path(stack_name);
    let raw = fs::read_to_string(path)?;
    serde_yaml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

fn save_lxc_compose(stack_name: &str, doc: &Value) -> io::Result<()> {
    let serialized = serde_yaml::to_string(doc)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(lxc_compose_path(stack_name), serialized)
}

/// Creates a default lxc-compose.yml for a stack when it does not exist.
///
/// This establishes a stable schema future features can rely on.
pub fn ensure_lxc_compose(stack_name: &str) -> io::Result<()> {
    let path = lxc_compose_path(stack_name);
    if Path::new(&path).exists() {
        return Ok(());
    }

    // Keep VMID as 0 by default so provisioning can set a real value later.
    let default_doc = format!(
        "version: 1\nstack_name: \"{}\"\nvmid: 0\nhostname: \"lxc-{}\"\nhwaddr: \"{}\"\ndeploy:\n  enabled: false\n  activated_at: null\n",
        stack_name,
        stack_name,
        generate_mac_address()
    );
    fs::write(path, default_doc)
}

/// Reads stack deploy.enabled from lxc-compose.yml.
///
/// Returns false when the key is absent.
pub fn is_stack_deploy_enabled(stack_name: &str) -> io::Result<bool> {
    let doc = load_lxc_compose(stack_name)?;

    Ok(doc
        .get("deploy")
        .and_then(|d| d.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false))
}

/// Sets stack deploy.enabled in lxc-compose.yml and writes the file back.
pub fn set_stack_deploy_enabled(stack_name: &str, enabled: bool) -> io::Result<()> {
    let mut doc = load_lxc_compose(stack_name)?;

    if !doc.is_mapping() {
        doc = Value::Mapping(Mapping::new());
    }

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    let deploy_key = Value::String("deploy".to_string());
    if !root.contains_key(&deploy_key) {
        root.insert(deploy_key.clone(), Value::Mapping(Mapping::new()));
    }

    let deploy_node = root.get_mut(&deploy_key).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing deploy block in lxc-compose",
        )
    })?;

    if !deploy_node.is_mapping() {
        *deploy_node = Value::Mapping(Mapping::new());
    }

    let deploy_map = deploy_node
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid deploy block"))?;

    deploy_map.insert(Value::String("enabled".to_string()), Value::Bool(enabled));

    if enabled {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        deploy_map.insert(
            Value::String("activated_at".to_string()),
            Value::String(now.to_string()),
        );
    } else {
        deploy_map.insert(Value::String("activated_at".to_string()), Value::Null);
    }

    save_lxc_compose(stack_name, &doc)
}

/// Ensures an app config mount entry exists in lxc-compose.yml.
///
/// Mount contract:
/// - source: /opt/appdata/<stack>/<app>-config
/// - target: /appdata/<app>-config
pub fn ensure_app_config_mount(stack_name: &str, app_name: &str) -> io::Result<()> {
    let mut doc = load_lxc_compose(stack_name)?;
    if !doc.is_mapping() {
        doc = Value::Mapping(Mapping::new());
    }

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    let mounts_key = Value::String("mounts".to_string());
    if !root.contains_key(&mounts_key) {
        root.insert(mounts_key.clone(), Value::Sequence(Vec::new()));
    }

    let mounts_node = root
        .get_mut(&mounts_key)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing mounts block"))?;
    if !mounts_node.is_sequence() {
        *mounts_node = Value::Sequence(Vec::new());
    }

    let mounts = mounts_node
        .as_sequence_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid mounts block"))?;

    let mount_name = format!("{}-config", app_name);
    let already_present = mounts.iter().any(|entry| {
        entry
            .get("name")
            .and_then(Value::as_str)
            .map(|name| name == mount_name)
            .unwrap_or(false)
    });
    if already_present {
        return Ok(());
    }

    let mut mount_map = Mapping::new();
    mount_map.insert(Value::String("name".to_string()), Value::String(mount_name));
    mount_map.insert(
        Value::String("source".to_string()),
        Value::String(format!("/opt/appdata/{}/{}-config", stack_name, app_name)),
    );
    mount_map.insert(
        Value::String("target".to_string()),
        Value::String(format!("/appdata/{}-config", app_name)),
    );

    mounts.push(Value::Mapping(mount_map));
    save_lxc_compose(stack_name, &doc)
}

/// Removes an app config mount entry from lxc-compose.yml if present.
pub fn remove_app_config_mount(stack_name: &str, app_name: &str) -> io::Result<()> {
    let mut doc = load_lxc_compose(stack_name)?;
    let Some(root) = doc.as_mapping_mut() else {
        return Ok(());
    };

    let mounts_key = Value::String("mounts".to_string());
    let Some(mounts_node) = root.get_mut(&mounts_key) else {
        return Ok(());
    };
    let Some(mounts) = mounts_node.as_sequence_mut() else {
        return Ok(());
    };

    let mount_name = format!("{}-config", app_name);
    mounts.retain(|entry| {
        entry
            .get("name")
            .and_then(Value::as_str)
            .map(|name| name != mount_name)
            .unwrap_or(true)
    });

    save_lxc_compose(stack_name, &doc)
}

/// Returns only stacks that have deploy.enabled=true in lxc-compose.yml.
pub fn list_deploy_enabled_stacks(stacks: &[String]) -> Vec<String> {
    stacks
        .iter()
        .filter_map(|stack| {
            let enabled = ensure_lxc_compose(stack)
                .and_then(|_| is_stack_deploy_enabled(stack))
                .unwrap_or(false);
            if enabled { Some(stack.clone()) } else { None }
        })
        .collect()
}

/// Writes stack-level provisioning defaults used by HOST provisioning flows.
pub fn set_stack_provisioning_defaults(
    stack_name: &str,
    cpu_cores: u8,
    memory_mb: u32,
    disk_gb: u32,
) -> io::Result<()> {
    let mut doc = load_lxc_compose(stack_name)?;
    if !doc.is_mapping() {
        doc = Value::Mapping(Mapping::new());
    }

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    root.insert(
        Value::String("cores".to_string()),
        Value::Number((cpu_cores as u64).into()),
    );
    root.insert(
        Value::String("memory_mb".to_string()),
        Value::Number((memory_mb as u64).into()),
    );
    root.insert(
        Value::String("rootfs_size_gb".to_string()),
        Value::Number((disk_gb as u64).into()),
    );

    // Explicitly maintain manual activation policy.
    let deploy_key = Value::String("deploy".to_string());
    if !root.contains_key(&deploy_key) {
        root.insert(deploy_key.clone(), Value::Mapping(Mapping::new()));
    }
    let deploy = root
        .get_mut(&deploy_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid deploy block"))?;
    deploy.insert(Value::String("enabled".to_string()), Value::Bool(false));
    deploy.insert(Value::String("activated_at".to_string()), Value::Null);

    save_lxc_compose(stack_name, &doc)
}
