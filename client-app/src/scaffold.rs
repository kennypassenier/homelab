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
    pub vlan_tag: Option<u16>,
    pub firewall: bool,
    pub ip_mode_v6: Option<String>,
    pub autostart: bool,
    pub startup_order: u32,
    pub cpu_cores: u8,
    pub cpu_limit: Option<f64>,
    pub cpu_units: u32,
    pub memory_mb: u32,
    pub swap_mb: u32,
    pub disk_gb: u32,
    pub rootfs_pool: String,
    pub host_storage_path: String,
    pub mount_point: String,
    pub appdata_backup: bool,
    pub appdata_read_only: bool,
    pub lxc_template: String,
    pub timezone: String,
    pub unprivileged: bool,
    pub features: Vec<String>,
    pub tun_device: Option<bool>, // None = auto-detect, Some(true) = force, Some(false) = disable
    pub tags: Vec<String>,
    pub protection: bool,
}

pub fn calculate_swap_mb(memory_mb: u32) -> u32 {
    if memory_mb < 2048 {
        memory_mb
    } else if memory_mb <= 8192 {
        2048
    } else {
        4096
    }
}

const DEFAULT_LXC_ROLE: &str = "app";
const DEFAULT_LXC_TEMPLATE: &str = "debian-12-standard 12.12-1 amd64";

/// Canonical LXC name in the standard scheme: vmid-role-stack.
pub fn canonical_lxc_name(vmid: u32, stack_name: &str) -> String {
    format!("{}-{}-{}", vmid, DEFAULT_LXC_ROLE, stack_name)
}

/// Derive reserved IPv4 from VMID using an adjustable prefix.
///
/// Mapping rule:
/// - host octet = vmid - 100
/// - valid host octet range: 1..=254
/// - prefix comes from STACK_IP_PREFIX (default: 10.10.10.)
pub fn derive_reserved_ipv4_from_vmid(vmid: u32) -> Option<String> {
    let host_octet = vmid.checked_sub(100)?;
    if !(1..=254).contains(&host_octet) {
        return None;
    }

    let mut prefix = std::env::var("STACK_IP_PREFIX").unwrap_or_else(|_| "10.10.10.".to_string());
    prefix = prefix.trim().to_string();
    if prefix.is_empty() {
        prefix = "10.10.10.".to_string();
    }
    if !prefix.ends_with('.') {
        prefix.push('.');
    }

    Some(format!("{}{}", prefix, host_octet))
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
        r#"version: 1
stack_name: "{}"
vmid: 0
hostname: "{}"
hwaddr: "{}"
tags: []

deploy:
  enabled: false
  activated_at: null

network:
  bridge: "vmbr0"
  ip_mode: "dhcp-reserved"
  reserved_ipv4: null
  vlan_tag: 10
  firewall: false
  ip_mode_v6: null

boot:
  autostart: true
  order: 90

resources:
  cores: 2
  cpu_limit: null
  cpu_units: 1024
  memory_mb: 2048
  swap_mb: 2048
  disk_gb: 32

storage:
  rootfs_pool: "local-lvm"
  host_path: "/opt/appdata/{}"
  mount_point: "/appdata"
  appdata_backup: true
  appdata_read_only: false

lxc:
  template: "{}"
  unprivileged: true
  timezone: "host"
  features:
    - "nesting=1"

hardware:
  tun_device: null

host_management:
  managed: true
  protection: false
"#,
        stack_name,
        canonical_lxc_name(0, stack_name),
        deterministic_mac_address(stack_name),
        stack_name,
        DEFAULT_LXC_TEMPLATE
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
    let hostname =
        mapping_string(root, "hostname").unwrap_or_else(|| canonical_lxc_name(vmid, stack_name));
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

    let vlan_tag = network
        .and_then(|m| m.get(Value::String("vlan_tag".to_string())))
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                v.as_u64().map(|n| n as u16)
            }
        });

    let firewall = network
        .and_then(|m| m.get(Value::String("firewall".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let ip_mode_v6 = network
        .and_then(|m| m.get(Value::String("ip_mode_v6".to_string())))
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                v.as_str().map(|s| s.to_string())
            }
        });

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
    let swap_mb = resources
        .and_then(|m| m.get(Value::String("swap_mb".to_string())))
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or_else(|| calculate_swap_mb(memory_mb));
    let disk_gb = resources
        .and_then(|m| m.get(Value::String("disk_gb".to_string())))
        .and_then(Value::as_u64)
        .or_else(|| {
            root.get(Value::String("rootfs_size_gb".to_string()))
                .and_then(Value::as_u64)
        })
        .unwrap_or(32) as u32;

    let cpu_limit = resources
        .and_then(|m| m.get(Value::String("cpu_limit".to_string())))
        .and_then(|v| if v.is_null() { None } else { v.as_f64() });

    let cpu_units = resources
        .and_then(|m| m.get(Value::String("cpu_units".to_string())))
        .and_then(Value::as_u64)
        .unwrap_or(1024) as u32;

    let storage = root
        .get(Value::String("storage".to_string()))
        .and_then(Value::as_mapping);
    let host_storage_path = storage
        .and_then(|m| m.get(Value::String("host_path".to_string())))
        .and_then(Value::as_str)
        .unwrap_or(&format!("/opt/appdata/{}", stack_name))
        .to_string();
    let mount_point = storage
        .and_then(|m| m.get(Value::String("mount_point".to_string())))
        .and_then(Value::as_str)
        .unwrap_or("/appdata")
        .to_string();

    let rootfs_pool = storage
        .and_then(|m| m.get(Value::String("rootfs_pool".to_string())))
        .and_then(Value::as_str)
        .unwrap_or("local-lvm")
        .to_string();

    let appdata_backup = storage
        .and_then(|m| m.get(Value::String("appdata_backup".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let appdata_read_only = storage
        .and_then(|m| m.get(Value::String("appdata_read_only".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let lxc_config = root
        .get(Value::String("lxc".to_string()))
        .and_then(Value::as_mapping);
    let lxc_template = lxc_config
        .and_then(|m| m.get(Value::String("template".to_string())))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_LXC_TEMPLATE)
        .to_string();
    let unprivileged = lxc_config
        .and_then(|m| m.get(Value::String("unprivileged".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let features = lxc_config
        .and_then(|m| m.get(Value::String("features".to_string())))
        .and_then(Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec!["nesting=1".to_string()]);

    let timezone = lxc_config
        .and_then(|m| m.get(Value::String("timezone".to_string())))
        .and_then(Value::as_str)
        .unwrap_or("host")
        .to_string();

    let hardware = root
        .get(Value::String("hardware".to_string()))
        .and_then(Value::as_mapping);
    let tun_device = hardware
        .and_then(|m| m.get(Value::String("tun_device".to_string())))
        .and_then(|v| {
            if v.is_null() {
                Some(None) // Explicitly null = auto-detect
            } else {
                v.as_bool().map(Some) // true/false = force/disable
            }
        })
        .unwrap_or(None); // Missing = auto-detect

    let tags: Vec<String> = root
        .get(Value::String("tags".to_string()))
        .and_then(Value::as_sequence)
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let protection = root
        .get(Value::String("host_management".to_string()))
        .and_then(Value::as_mapping)
        .and_then(|m| m.get(Value::String("protection".to_string())))
        .and_then(Value::as_bool)
        .unwrap_or(false);

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
        vlan_tag,
        firewall,
        ip_mode_v6,
        autostart,
        startup_order,
        cpu_cores,
        cpu_limit,
        cpu_units,
        memory_mb,
        swap_mb,
        disk_gb,
        rootfs_pool,
        host_storage_path,
        mount_point,
        appdata_backup,
        appdata_read_only,
        lxc_template,
        timezone,
        unprivileged,
        features,
        tun_device,
        tags,
        protection,
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

    let tags_seq: Vec<Value> = config
        .tags
        .iter()
        .map(|t| Value::String(t.clone()))
        .collect();
    root.insert(Value::String("tags".to_string()), Value::Sequence(tags_seq));

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
    network.insert(
        Value::String("vlan_tag".to_string()),
        config
            .vlan_tag
            .map(|v| Value::Number((v as u64).into()))
            .unwrap_or(Value::Null),
    );
    network.insert(
        Value::String("firewall".to_string()),
        Value::Bool(config.firewall),
    );
    network.insert(
        Value::String("ip_mode_v6".to_string()),
        config
            .ip_mode_v6
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
        Value::String("cpu_limit".to_string()),
        config
            .cpu_limit
            .map(|v| serde_yaml::to_value(v).unwrap_or(Value::Null))
            .unwrap_or(Value::Null),
    );
    resources.insert(
        Value::String("cpu_units".to_string()),
        Value::Number((config.cpu_units as u64).into()),
    );
    resources.insert(
        Value::String("memory_mb".to_string()),
        Value::Number((config.memory_mb as u64).into()),
    );
    resources.insert(
        Value::String("swap_mb".to_string()),
        Value::Number((config.swap_mb as u64).into()),
    );
    resources.insert(
        Value::String("disk_gb".to_string()),
        Value::Number((config.disk_gb as u64).into()),
    );
    root.insert(
        Value::String("resources".to_string()),
        Value::Mapping(resources),
    );

    let mut storage = Mapping::new();
    storage.insert(
        Value::String("rootfs_pool".to_string()),
        Value::String(config.rootfs_pool.clone()),
    );
    storage.insert(
        Value::String("host_path".to_string()),
        Value::String(config.host_storage_path.clone()),
    );
    storage.insert(
        Value::String("mount_point".to_string()),
        Value::String(config.mount_point.clone()),
    );
    storage.insert(
        Value::String("appdata_backup".to_string()),
        Value::Bool(config.appdata_backup),
    );
    storage.insert(
        Value::String("appdata_read_only".to_string()),
        Value::Bool(config.appdata_read_only),
    );
    root.insert(
        Value::String("storage".to_string()),
        Value::Mapping(storage),
    );

    let mut lxc_config = Mapping::new();
    lxc_config.insert(
        Value::String("template".to_string()),
        Value::String(config.lxc_template.clone()),
    );
    lxc_config.insert(
        Value::String("unprivileged".to_string()),
        Value::Bool(config.unprivileged),
    );
    lxc_config.insert(
        Value::String("timezone".to_string()),
        Value::String(config.timezone.clone()),
    );
    let features_seq: Vec<Value> = config
        .features
        .iter()
        .map(|f| Value::String(f.clone()))
        .collect();
    lxc_config.insert(
        Value::String("features".to_string()),
        Value::Sequence(features_seq),
    );
    root.insert(Value::String("lxc".to_string()), Value::Mapping(lxc_config));

    let mut hardware = Mapping::new();
    hardware.insert(
        Value::String("tun_device".to_string()),
        config
            .tun_device
            .map(|v| Value::Bool(v))
            .unwrap_or(Value::Null),
    );
    root.insert(
        Value::String("hardware".to_string()),
        Value::Mapping(hardware),
    );

    let mut host_mgmt = if root.contains_key(&Value::String("host_management".to_string())) {
        root.get(&Value::String("host_management".to_string()))
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_default()
    } else {
        Mapping::new()
    };
    host_mgmt.insert(Value::String("managed".to_string()), Value::Bool(true));
    host_mgmt.insert(
        Value::String("protection".to_string()),
        Value::Bool(config.protection),
    );
    root.insert(
        Value::String("host_management".to_string()),
        Value::Mapping(host_mgmt),
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
    vmid: u32,
    cpu_cores: u8,
    memory_mb: u32,
    disk_gb: u32,
    autostart: bool,
    startup_order: u32,
) -> io::Result<()> {
    let mut config = read_stack_config(stack_name)?;
    config.vmid = vmid;
    config.hostname = canonical_lxc_name(vmid, stack_name);
    config.ip_mode = "dhcp-reserved".to_string();
    config.reserved_ipv4 = derive_reserved_ipv4_from_vmid(vmid);
    config.cpu_cores = cpu_cores;
    config.memory_mb = memory_mb;
    config.swap_mb = calculate_swap_mb(memory_mb);
    config.disk_gb = disk_gb;
    config.autostart = autostart;
    config.startup_order = startup_order;
    config.deploy_enabled = false;
    config.activated_at = None;
    // Ensure defaults for new fields if not already set
    if config.vlan_tag.is_none() {
        config.vlan_tag = Some(10);
    }
    if config.rootfs_pool.is_empty() {
        config.rootfs_pool =
            std::env::var("LXC_STORAGE_POOL").unwrap_or_else(|_| "local-lvm".to_string());
    }
    if config.host_storage_path.is_empty() {
        config.host_storage_path = format!("/opt/appdata/{}", stack_name);
    }
    if config.mount_point.is_empty() {
        config.mount_point = "/appdata".to_string();
    }
    if config.lxc_template.is_empty() {
        config.lxc_template = DEFAULT_LXC_TEMPLATE.to_string();
    }
    if config.features.is_empty() {
        config.features = vec!["nesting=1".to_string()];
    }
    if config.timezone.is_empty() {
        config.timezone = "host".to_string();
    }
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
