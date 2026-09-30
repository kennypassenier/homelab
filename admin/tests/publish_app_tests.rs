//! feat-publish-1: an app's hostname and port, turned into `gateway_route`
//! (or `extra_routes`) plus the Traefik router/service/loadBalancer
//! fragment, and optionally a tile — all through `stackedit::changes`, the
//! same plan-then-commit path every other form uses.

use homelab_admin::core::stackedit::{changes, parse_manifest, StackEdit, StackTexts};
use homelab_admin::core::stackedit_publish::{
    gateway_route_state, new_route_file, primary_filename, publish_problems, valid_hostname,
    PublishAppEdit,
};
use homelab_admin::core::stackedit_tiles::TileFields;

const BASE: &str = "stack_name: x\nvmid: 150\nhostname: 150-app-x\nnetwork:\n  ip: 10.10.10.50/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\nresources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 0\n  disk_gb: 8\n  storage: local-lvm\nlxc:\n  template: clone:996\n  unprivileged: true\n  features: nesting=1\n  protection: true\nboot:\n  onboot: true\napps: [sonarr]\n";

fn texts() -> StackTexts {
    let mut t = StackTexts::new();
    t.insert("lxc-compose.yml".to_string(), BASE.to_string());
    t
}

fn edit() -> PublishAppEdit {
    PublishAppEdit {
        app: "sonarr".to_string(),
        hostname: "sonarr.kp-soft.dev".to_string(),
        port: 8989,
        external: false,
        separate_file: false,
        tile: None,
    }
}

// ── pure helpers ─────────────────────────────────────────────────────────

#[test]
fn hostname_validation() {
    assert!(valid_hostname("sonarr.kp-soft.dev"));
    assert!(!valid_hostname(""));
    assert!(!valid_hostname("localhost"));
    assert!(!valid_hostname("-bad.kp-soft.dev"));
    assert!(!valid_hostname("bad-.kp-soft.dev"));
    assert!(!valid_hostname("has space.kp-soft.dev"));
}

#[test]
fn the_primary_filename_matches_what_the_client_independently_derives() {
    // client/src/spec.rs::declared_routes checks gateway_route.filename
    // against exactly this shape, and refuses to deploy otherwise (F115).
    assert_eq!(primary_filename(104, "media"), "104-app-media.yml");
}

#[test]
fn the_route_fragment_is_readable_by_client_routes() {
    let text = new_route_file("sonarr", "sonarr.kp-soft.dev", "10.10.10.50", 8989);
    let hosts = homelab_client::routes::hostnames(&text).unwrap();
    assert_eq!(hosts, vec!["sonarr.kp-soft.dev".to_string()]);
    let backends = homelab_client::routes::backends(&text).unwrap();
    assert_eq!(backends, vec!["http://10.10.10.50:8989".to_string()]);
    let addrs = homelab_client::routes::service_addresses(&text).unwrap();
    assert_eq!(
        addrs.get("sonarr").map(String::as_str),
        Some("https://sonarr.kp-soft.dev")
    );
}

#[test]
fn gateway_route_state_reads_the_raw_keys_the_manifest_ignores() {
    assert_eq!(gateway_route_state(BASE), (false, 0));
    let with_route =
        format!("{BASE}gateway_route:\n  filename: 104-app-x.yml\n  gateway_vmid: 104\n");
    assert_eq!(gateway_route_state(&with_route), (true, 0));
    let with_extra = format!(
        "{with_route}extra_routes:\n  - filename: a.yml\n    gateway_vmid: 104\n  - filename: b.yml\n    gateway_vmid: 104\n"
    );
    assert_eq!(gateway_route_state(&with_extra), (true, 2));
}

#[test]
fn problems_catch_a_bad_port_or_hostname() {
    let mut e = edit();
    e.hostname = "not a host".to_string();
    e.port = 0;
    let problems = publish_problems(&e);
    assert!(
        problems.iter().any(|p| p.contains("hostname")),
        "{problems:?}"
    );
    assert!(problems.iter().any(|p| p.contains("port")), "{problems:?}");
}

// ── through stackedit::changes ───────────────────────────────────────────

#[test]
fn the_first_publish_of_a_stack_becomes_the_primary_route() {
    let out = changes("x", &texts(), &StackEdit::PublishApp(edit()), None).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    let manifest = out
        .iter()
        .find(|c| c.path == "stacks/x/lxc-compose.yml")
        .unwrap();
    let new = manifest.new.as_ref().unwrap();
    assert!(new.contains("gateway_route:"), "{new}");
    assert!(new.contains("104-app-x.yml"), "{new}");
    let route = out
        .iter()
        .find(|c| c.path == "stacks/x/traefik-routes.yml")
        .unwrap();
    assert!(route.old.is_none());
    let hosts = homelab_client::routes::hostnames(route.new.as_ref().unwrap()).unwrap();
    assert_eq!(hosts, vec!["sonarr.kp-soft.dev".to_string()]);
    // The manifest still parses and gained nothing else.
    let m = parse_manifest(new).unwrap();
    assert_eq!(m.apps, vec!["sonarr".to_string()]);
}

#[test]
fn a_second_app_extends_the_same_route_file_instead_of_a_new_gateway_route() {
    let mut t = texts();
    let with_route = format!(
        "{BASE}apps: [sonarr, radarr]\ngateway_route:\n  filename: 104-app-x.yml\n  gateway_vmid: 104\n"
    );
    t.insert("lxc-compose.yml".to_string(), with_route);
    t.insert(
        "traefik-routes.yml".to_string(),
        new_route_file("sonarr", "sonarr.kp-soft.dev", "10.10.10.50", 8989),
    );
    let mut e = edit();
    e.app = "radarr".to_string();
    e.hostname = "radarr.kp-soft.dev".to_string();
    e.port = 7878;
    let out = changes("x", &t, &StackEdit::PublishApp(e), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].path, "stacks/x/traefik-routes.yml");
    let new = out[0].new.as_ref().unwrap();
    let hosts = homelab_client::routes::hostnames(new).unwrap();
    assert!(
        hosts.contains(&"sonarr.kp-soft.dev".to_string()),
        "{hosts:?}"
    );
    assert!(
        hosts.contains(&"radarr.kp-soft.dev".to_string()),
        "{hosts:?}"
    );
}

#[test]
fn separate_file_forces_an_extra_routes_entry() {
    let mut t = texts();
    let with_route =
        format!("{BASE}gateway_route:\n  filename: 104-app-x.yml\n  gateway_vmid: 104\n");
    t.insert("lxc-compose.yml".to_string(), with_route);
    t.insert(
        "traefik-routes.yml".to_string(),
        new_route_file("sonarr", "sonarr.kp-soft.dev", "10.10.10.50", 8989),
    );
    let mut e = edit();
    e.separate_file = true;
    let out = changes("x", &t, &StackEdit::PublishApp(e), None).unwrap();
    let manifest = out
        .iter()
        .find(|c| c.path == "stacks/x/lxc-compose.yml")
        .unwrap();
    assert!(
        manifest.new.as_ref().unwrap().contains("extra_routes:"),
        "{manifest:?}"
    );
    assert!(
        out.iter()
            .any(|c| c.path == "stacks/x/routes/sonarr-kp-soft-dev.yml"),
        "{out:?}"
    );
}

#[test]
fn a_tile_is_created_in_the_same_commit_when_asked() {
    let mut e = edit();
    e.tile = Some(TileFields {
        name: "Sonarr".to_string(),
        group: "Media".to_string(),
        order: None,
        description: None,
        url: None,
        reading: None,
        watch_every: None,
        down_after: None,
    });
    let out = changes("x", &texts(), &StackEdit::PublishApp(e), None).unwrap();
    let manifest = out
        .iter()
        .find(|c| c.path == "stacks/x/lxc-compose.yml")
        .unwrap();
    let m = parse_manifest(manifest.new.as_ref().unwrap()).unwrap();
    assert_eq!(m.tiles["sonarr.kp-soft.dev"].name, "Sonarr");
}

#[test]
fn an_app_the_stack_does_not_have_is_refused() {
    let mut e = edit();
    e.app = "not-an-app".to_string();
    let err = changes("x", &texts(), &StackEdit::PublishApp(e), None).unwrap_err();
    assert!(err.why.contains("is not an app of this stack"), "{err:?}");
}
