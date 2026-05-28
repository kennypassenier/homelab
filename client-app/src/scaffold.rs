use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_yaml::{Mapping, Value};

pub struct StackConfig {
    pub stack_name: String,
    pub vmid: u32,
    pub hostname: String,
    pub hwaddr: String,
    pub deploy_enabled: bool,
    pub activated_at: Option<String>,
    pub bridge: String,
    pub ip_mode: String,
    pub reserved_ipv4: Option<String>,
    pub autostart: bool,
    pub startup_order: u32,
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub disk_gb: u32,
}

const DEFAULT_LXC_ROLE: &str = "app";

/// Canonical LXC name in the standard scheme: vmid-role-stack.
pub fn canonical_lxc_name(vmid: u32, stack_name: &str) -> String {
    format!("{}-{}-{}", vmid, DEFAULT_LXC_ROLE, stack_name)
}

/// Legacy alias kept for migration compatibility.
pub fn legacy_lxc_alias(stack_name: &str) -> String {
    format!("lxc-{}", stack_name)
}

/// Generates a deterministic locally-administered MAC address for a stack.
pub fn deterministic_mac_address(stack_name: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in stack_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    let bytes = [
        0x02,
        ((hash >> 32) & 0xff) as u8,
        ((hash >> 24) & 0xff) as u8,
        ((hash >> 16) & 0xff) as u8,
        ((hash >> 8) & 0xff) as u8,
        (hash & 0xff) as u8,
    ];

    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
    )
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

    let default_doc = format!(
        "version: 1\nstack_name: \"{}\"\nvmid: 0\nhostname: \"{}\"\nhwaddr: \"{}\"\n\ndeploy:\n  enabled: false\n  activated_at: null\n\nnetwork:\n  bridge: \"vmbr0\"\n  ip_mode: \"dhcp-reserved\"\n  reserved_ipv4: null\n\nboot:\n  autostart: true\n  order: 90\n\nresources:\n  cores: 2\n  memory_mb: 2048\n  disk_gb: 32\n",
        stack_name,
        canonical_lxc_name(0, stack_name),
        deterministic_mac_address(stack_name)
    );
    fs::write(path, default_doc)
}

pub fn read_stack_config(stack_name: &str) -> io::Result<StackConfig> {
    ensure_lxc_compose(stack_name)?;
    let doc = load_lxc_compose(stack_name)?;
    let root = doc
        .as_mapping()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    let vmid = mapping_u32(root, "vmid").unwrap_or(0);
    let hostname = mapping_string(root, "hostname")
        .unwrap_or_else(|| canonical_lxc_name(vmid, stack_name));
    let hwaddr =
        mapping_string(root, "hwaddr").unwrap_or_else(|| deterministic_mac_address(stack_name));
    let deploy_enabled = root
        .get(Value::String("deploy".to_string()))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::String("enabled".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let activated_at = root
        .get(Value::String("deploy".to_string()))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::String("activated_at".to_string())))
        .and_then(Value::as_str)
        .map(|v| v.to_string());

    let network = root
        .get(Value::String("network".to_string()))
        .and_then(Value::as_mapping);
    let bridge = network
        .and_then(|m| m.get(Value::String("bridge".to_string())))
        .and_then(Value::as_str)
        .unwrap_or("vmbr0")
        .to_string();
    let ip_mode = network
        .and_then(|m| m.get(Value::String("ip_mode".to_string())))
        .and_then(Value::as_str)
        .unwrap_or("dhcp-reserved")
        .to_string();
    let reserved_ipv4 = network
        .and_then(|m| m.get(Value::String("reserved_ipv4".to_string())))
        .and_then(Value::as_str)
        .map(|v| v.to_string());

    let boot = root
        .get(Value::String("boot".to_string()))
        .and_then(Value::as_mapping);
    let autostart = boot
        .and_then(|m| m.get(Value::String("autostart".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let startup_order = boot
        .and_then(|m| m.get(Value::String("order".to_string())))
        .and_then(Value::as_u64)
        .unwrap_or(90) as u32;

    let resources = root
        .get(Value::String("resources".to_string()))
        .and_then(Value::as_mapping);
    let cpu_cores = resources
        .and_then(|m| m.get(Value::String("cores".to_string())))
        .and_then(Value::as_u64)
        .or_else(|| {
            root.get(Value::String("cores".to_string()))
                .and_then(Value::as_u64)
        })
        .unwrap_or(2) as u8;
    let memory_mb = resources
        .and_then(|m| m.get(Value::String("memory_mb".to_string())))
        .and_then(Value::as_u64)
        .or_else(|| {
            root.get(Value::String("memory_mb".to_string()))
                .and_then(Value::as_u64)
        })
        .unwrap_or(2048) as u32;
    let disk_gb = resources
        .and_then(|m| m.get(Value::String("disk_gb".to_string())))
        .and_then(Value::as_u64)
        .or_else(|| {
            root.get(Value::String("rootfs_size_gb".to_string()))
                .and_then(Value::as_u64)
        })
        .unwrap_or(32) as u32;

    Ok(StackConfig {
        stack_name: stack_name.to_string(),
        vmid,
        hostname,
        hwaddr,
        deploy_enabled,
        activated_at,
        bridge,
        ip_mode,
        reserved_ipv4,
        autostart,
        startup_order,
        cpu_cores,
        memory_mb,
        disk_gb,
    })
}

pub fn save_stack_config(config: &StackConfig) -> io::Result<()> {
    let mut doc = load_lxc_compose(&config.stack_name).unwrap_or(Value::Mapping(Mapping::new()));
    if !doc.is_mapping() {
        doc = Value::Mapping(Mapping::new());
    }

    let root = doc
        .as_mapping_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lxc-compose root"))?;

    root.insert(
        Value::String("version".to_string()),
        Value::Number(1.into()),
    );
    root.insert(
        Value::String("stack_name".to_string()),
        Value::String(config.stack_name.clone()),
    );
    root.insert(
        Value::String("vmid".to_string()),
        Value::Number((config.vmid as u64).into()),
    );
    root.insert(
        Value::String("hostname".to_string()),
        Value::String(config.hostname.clone()),
    );
    root.insert(
        Value::String("hwaddr".to_string()),
        Value::String(config.hwaddr.clone()),
    );

    let mut deploy = Mapping::new();
    deploy.insert(
        Value::String("enabled".to_string()),
        Value::Bool(config.deploy_enabled),
    );
    deploy.insert(
        Value::String("activated_at".to_string()),
        config
            .activated_at
            .as_ref()
            .map(|v| Value::String(v.clone()))
            .unwrap_or(Value::Null),
    );
    root.insert(Value::String("deploy".to_string()), Value::Mapping(deploy));

    let mut network = Mapping::new();
    network.insert(
        Value::String("bridge".to_string()),
        Value::String(config.bridge.clone()),
    );
    network.insert(
        Value::String("ip_mode".to_string()),
        Value::String(config.ip_mode.clone()),
    );
    network.insert(
        Value::String("reserved_ipv4".to_string()),
        config
            .reserved_ipv4
            .as_ref()
            .map(|v| Value::String(v.clone()))
            .unwrap_or(Value::Null),
    );
    root.insert(
        Value::String("network".to_string()),
        Value::Mapping(network),
    );

    let mut boot = Mapping::new();
    boot.insert(
        Value::String("autostart".to_string()),
        Value::Bool(config.autostart),
    );
    boot.insert(
        Value::String("order".to_string()),
        Value::Number((config.startup_order as u64).into()),
    );
    root.insert(Value::String("boot".to_string()), Value::Mapping(boot));

    let mut resources = Mapping::new();
    resources.insert(
        Value::String("cores".to_string()),
        Value::Number((config.cpu_cores as u64).into()),
    );
    resources.insert(
        Value::String("memory_mb".to_string()),
        Value::Number((config.memory_mb as u64).into()),
    );
    resources.insert(
        Value::String("disk_gb".to_string()),
        Value::Number((config.disk_gb as u64).into()),
    );
    root.insert(
        Value::String("resources".to_string()),
        Value::Mapping(resources),
    );

    root.remove(Value::String("cores".to_string()));
    root.remove(Value::String("memory_mb".to_string()));
    root.remove(Value::String("rootfs_size_gb".to_string()));

    save_lxc_compose(&config.stack_name, &doc)
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
    autostart: bool,
    startup_order: u32,
) -> io::Result<()> {
    let mut config = read_stack_config(stack_name)?;
    config.cpu_cores = cpu_cores;
    config.memory_mb = memory_mb;
    config.disk_gb = disk_gb;
    config.autostart = autostart;
    config.startup_order = startup_order;
    config.deploy_enabled = false;
    config.activated_at = None;
    save_stack_config(&config)
}

fn mapping_string(root: &Mapping, key: &str) -> Option<String> {
    root.get(Value::String(key.to_string()))
        .and_then(Value::as_str)
        .map(|v| v.to_string())
}

fn mapping_u32(root: &Mapping, key: &str) -> Option<u32> {
    root.get(Value::String(key.to_string()))
        .and_then(Value::as_u64)
        .map(|v| v as u32)
}
