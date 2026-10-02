//! fix-211 (Kenny, 2026-10-02, decision "Host weigert enkel onbekende
//! velden"): `homelab_core::wire::unknown_fields` is the host-side half of
//! the fix — these tests exercise it against the real `DeploySpec` /
//! `StackManifest` shape rather than the toy structs in `core/src/wire.rs`'s
//! own unit tests, so a real nested field (`storage[n].<field>`) is proven
//! to be named correctly, and a real older-client payload (missing keys
//! entirely, never carrying extra ones) is proven to never be flagged.

use std::collections::BTreeMap;

use homelab_core::manifest::*;
use homelab_core::wire::unknown_fields;

fn manifest(vmid: u16, stack: &str) -> StackManifest {
    StackManifest {
        home_address_whitelist: None,
        tiles: Default::default(),
        log_files: Vec::new(),
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        on_demand: false,
        syslog_receivers: vec![],
        firewall: None,
        natives: Vec::new(),
        stack_name: stack.into(),
        vmid,
        hostname: format!("{}-app-{}", vmid, stack),
        network: NetworkSpec {
            ip: format!("10.10.10.{}/24", vmid - 100),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 256,
            disk_gb: 4,
            storage: "local-lvm".into(),
        },
        lxc: LxcSpec {
            timezone: "host".into(),
            template: "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst".into(),
            unprivileged: true,
            features: "nesting=1,keyctl=1".into(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![
            MountSpec {
                host_path: format!("/appdata/{}/{}-config", stack, stack),
                mount_point: format!("/appdata/{}/{}-config", stack, stack),
                no_data: false,
                no_backup: None,
                host_owner_uid: Some(101000),
                app: Some(stack.into()),
                postgres_check_image: None,
            },
            MountSpec {
                host_path: format!("/appdata/{}/{}-cache", stack, stack),
                mount_point: format!("/appdata/{}/{}-cache", stack, stack),
                no_data: false,
                no_backup: None,
                host_owner_uid: Some(101000),
                app: Some(stack.into()),
                postgres_check_image: None,
            },
        ],
        apps: vec![stack.into()],
    }
}

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        client_schema: CURRENT_CLIENT_SCHEMA,
        source: None,
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: manifest(vmid, stack),
        files: vec![FileBlob {
            path: format!("{}/docker-compose.yml", stack),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: BTreeMap::new(),
        extra_routes: Vec::new(),
        gateway_route: Some(GatewayRoute {
            gateway_vmid: 104,
            filename: format!("{}-app-{}.yml", vmid, stack),
            content: "http: {}\n".into(),
        }),
        checks: Default::default(),
    }
}

/// covers: fix-211
///
/// (1) a deploy carrying a field this build's `DeploySpec`/`MountSpec`
/// does not know is named by its full path, deep inside the second
/// `storage` entry, the same shape the 2026-08-31 incident's
/// `data_mounts` (a field that has since been added for real, at the top
/// level — this test uses a still-fictional one, `encrypt`, to stand in
/// for "a field a build from today does not have") took.
#[test]
fn fix_211_a_field_the_manifest_does_not_know_is_named_by_its_full_path() {
    let spec = spec(201, "downloader");
    let mut value = serde_json::to_value(&spec).unwrap();
    value["manifest"]["storage"][1]["encrypt"] = serde_json::json!(true);
    value["turbo_mode"] = serde_json::json!("fast");

    let unknown = unknown_fields(&value, &spec);
    assert_eq!(
        unknown,
        vec![
            "manifest.storage[1].encrypt".to_string(),
            "turbo_mode".to_string()
        ]
    );
}

/// covers: fix-211
///
/// (4) an older client's payload never carries a field this build doesn't
/// know; it carries FEWER fields than a current build would. Removing
/// `client_schema`, `secret_files` and `native_manifests` entirely (a
/// client built before fix-199/fix-146 added them, say) must still
/// deserialize via their `#[serde(default)]` and must never be reported
/// as carrying an unknown field — there is nothing in this payload the
/// current build doesn't recognise, only things it recognises and was
/// not told.
#[test]
fn fix_211_an_older_clients_payload_with_fields_missing_entirely_is_never_flagged() {
    let spec = spec(202, "kyu");
    let mut value = serde_json::to_value(&spec).unwrap();
    let obj = value.as_object_mut().unwrap();
    obj.remove("client_schema");
    obj.remove("secret_files");
    obj.remove("native_manifests");

    let typed: DeploySpec = serde_json::from_value(value.clone())
        .expect("an older client's spec, missing newer fields entirely, still deserializes");
    assert_eq!(typed.client_schema, 0, "the documented safe default");

    assert!(
        unknown_fields(&value, &typed).is_empty(),
        "a field missing entirely is the OLDER-client direction, never an unknown field"
    );
}

/// covers: fix-211
///
/// A payload with no extra field at all names nothing, the baseline every
/// other case in this file is contrasted against.
#[test]
fn fix_211_a_payload_with_only_known_fields_names_nothing() {
    let spec = spec(203, "media");
    let value = serde_json::to_value(&spec).unwrap();
    assert!(unknown_fields(&value, &spec).is_empty());
}
