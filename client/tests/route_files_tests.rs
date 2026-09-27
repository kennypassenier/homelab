//! fix-91 (expert panel, routes-outside-repo-unvalidated; Kenny 2026-09-27:
//! "alles-in-repo-plus-controle"): every route file on the gateway comes from
//! the repository.
//!
//! Measured on CT 104 the same day: four files there were written by hand
//! and named nowhere in the repository — five public hostnames, among them
//! the Proxmox and OPNsense login pages. They are declared now, and the first
//! deploy has to write them exactly as they are: the hashes below are
//! `sha256sum` of the live files, read with
//! `pct exec 104 -- cat /appdata/gateway/traefik-config/routes/<file>`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn stacks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks")
}

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

fn routes_of(stack: &str) -> Vec<homelab_core::routes::RouteDecl> {
    homelab_client::spec::route_files(&stacks().join(stack))
        .unwrap_or_else(|e| panic!("{}: {}", stack, e))
}

/// The four hand-written files are declared by the stack they belong to,
/// under the name they have on the gateway, byte for byte.
/// covers: fix-91
#[test]
fn the_four_hand_written_routes_are_declared_byte_for_byte() {
    let live = [
        (
            "almanac",
            "112-app-almanac.yml",
            "58dea0b1b28ba220b1ea697a29c864450bcb09ab5aedf111c89d93ad7111bd12",
        ),
        (
            "kyu",
            "manual-kyu.yml",
            "5ea7603ea6cf6b7ce034598710945405b04351d443e1f9c03e19b763c1f9762a",
        ),
        (
            "gateway",
            "manual-homeassistant.yml",
            "4b3fe3c81aa5a652445a79aa689504899b3016594f62c01a53fcc5cb76803574",
        ),
        (
            "gateway",
            "manual-routes.yml",
            "45c36dc59b7136fdad2db3097d70544b225f765c73357eccf302f18c7416ec50",
        ),
    ];
    for (stack, file, want) in live {
        let routes = routes_of(stack);
        let decl = routes
            .iter()
            .find(|r| r.filename == file)
            .unwrap_or_else(|| panic!("{} does not declare {}", stack, file));
        assert_eq!(decl.stack, stack);
        assert_eq!(
            sha(&decl.content),
            want,
            "{} differs from the live file",
            file
        );
    }
}

/// Home Assistant, OPNsense and Proxmox are not stacks homelab manages; the
/// routes to them say so, target by target. kyu's route goes to its own
/// container and declares nothing external.
/// covers: fix-91
#[test]
fn routes_to_what_homelab_does_not_manage_are_declared_external() {
    let gateway = routes_of("gateway");
    let ext = |file: &str| {
        gateway
            .iter()
            .find(|r| r.filename == file)
            .map(|r| r.external.clone())
            .unwrap()
    };
    assert_eq!(
        ext("manual-homeassistant.yml"),
        vec!["http://10.10.10.2:8123"]
    );
    assert_eq!(
        ext("manual-routes.yml"),
        vec!["https://10.10.5.1", "https://10.10.5.250:8006"]
    );
    assert!(ext("104-app-gateway.yml").is_empty());
    assert!(routes_of("kyu").iter().all(|r| r.external.is_empty()));
    assert!(routes_of("almanac").iter().all(|r| r.external.is_empty()));
}

/// An extra route travels in the deploy spec as a route, never as a file
/// shipped into the container.
/// covers: fix-91
#[test]
fn an_extra_route_is_sent_as_a_route_and_not_into_the_container() {
    let tmp = std::env::temp_dir().join(format!("homelab-fix91-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dir = tmp.join("xr");
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::create_dir_all(dir.join("routes")).unwrap();
    std::fs::write(dir.join("app/docker-compose.yml"), "services: {}\n").unwrap();
    std::fs::write(dir.join("routes/manual-xr.yml"), "http: {}\n").unwrap();
    std::fs::write(
        dir.join("lxc-compose.yml"),
        "\
stack_name: xr
vmid: 150
hostname: 150-app-xr
network: {ip: 10.10.10.50/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}
resources: {cores: 1, memory_mb: 512, swap_mb: 256, disk_gb: 4, storage: local-lvm}
lxc: {template: 'clone:998', unprivileged: true, features: 'nesting=1', protection: false, gpu: false, vpn: false}
boot: {onboot: true, order: 50}
storage: []
apps: [app]
extra_routes:
  - filename: manual-xr.yml
",
    )
    .unwrap();
    let spec = homelab_client::spec::build_spec(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(spec.extra_routes.len(), 1);
    assert_eq!(spec.extra_routes[0].filename, "manual-xr.yml");
    assert_eq!(spec.extra_routes[0].content, "http: {}\n");
    assert_eq!(spec.extra_routes[0].gateway_vmid, 104);
    assert!(
        spec.files.iter().all(|f| !f.path.starts_with("routes/")),
        "{:?}",
        spec.files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}
