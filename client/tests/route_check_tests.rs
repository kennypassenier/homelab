//! fix-92 (expert panel, routes-outside-repo-unvalidated; Kenny 2026-09-27:
//! "alles-in-repo-plus-controle"): `homelab plan`, `deploy` and `apply` hold
//! every route in the repository against every other before anything is
//! sent — one owner per hostname, and every backend either a stack's own
//! address or declared external.

use std::path::{Path, PathBuf};
use std::process::Command;

fn stacks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks")
}

/// The repository as it stands passes. It held 26 hostnames through
/// 2026-09-28 (25 measured on the gateway plus admin.kp-soft.dev for the
/// dashboard on CT 120, stacks/admin); retiring Homepage, Uptime Kuma,
/// GoAccess and Grafana (2026-10-01, stacks/home, stacks/uptime,
/// stacks/gateway/goaccess, the Grafana app under stacks/metrics) dropped
/// their four routes, leaving 22.
/// covers: fix-92
#[test]
fn the_routes_in_the_repository_pass_and_cover_every_public_hostname() {
    let problems = homelab_client::spec::fleet_route_problems(&stacks()).unwrap();
    assert!(problems.is_empty(), "{:?}", problems);
    let (_, routes) = homelab_client::spec::fleet_routes(&stacks()).unwrap();
    let mut hosts: Vec<String> = routes
        .iter()
        .flat_map(|r| homelab_client::routes::hostnames(&r.content).unwrap())
        .collect();
    hosts.sort();
    hosts.dedup();
    assert_eq!(hosts.len(), 22, "{:?}", hosts);
    for h in [
        "admin.kp-soft.dev",
        "almanac.kp-soft.dev",
        "kyu.kp-soft.dev",
        "ha.kp-soft.dev",
        "opn.kp-soft.dev",
        "prox.kp-soft.dev",
    ] {
        assert!(hosts.iter().any(|x| x == h), "{} missing", h);
    }
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn stack(base: &Path, name: &str, vmid: u16, host: &str) {
    let dir = base.join(name);
    write(
        &dir.join("lxc-compose.yml"),
        &format!(
            "stack_name: {name}\nvmid: {vmid}\nhostname: {vmid}-app-{name}\n\
             network: {{ip: 10.10.10.{ip}/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}}\n\
             resources: {{cores: 1, memory_mb: 512, swap_mb: 256, disk_gb: 4, storage: local-lvm}}\n\
             lxc: {{template: 'clone:998', unprivileged: true, features: 'nesting=1', protection: false, gpu: false, vpn: false}}\n\
             boot: {{onboot: true, order: 50}}\nstorage: []\napps: [app]\n\
             gateway_route:\n  filename: {vmid}-app-{name}.yml\n",
            ip = vmid - 100
        ),
    );
    write(&dir.join("app/docker-compose.yml"), "services: {}\n");
    write(
        &dir.join("traefik-routes.yml"),
        &format!(
            "http:\n  routers:\n    r:\n      rule: \"Host(`{host}`)\"\n      service: s\n  services:\n    s:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.{}:80\"\n",
            vmid - 100
        ),
    );
}

/// `homelab plan` refuses a stack whose hostname another stack already
/// routes, and names both files — before anything reaches the host.
/// covers: fix-92
#[test]
fn plan_refuses_a_hostname_another_stack_already_routes() {
    let tmp = std::env::temp_dir().join(format!("homelab-fix92-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let base = tmp.join("stacks");
    stack(&base, "media", 106, "fin.kp-soft.dev");
    stack(&base, "copy", 150, "fin.kp-soft.dev");
    let out = Command::new(env!("CARGO_BIN_EXE_homelab"))
        .arg("plan")
        .arg(base.join("copy"))
        .env_remove("HOMELAB_TOKEN")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{}", text);
    assert!(
        text.contains("fin.kp-soft.dev")
            && text.contains("106-app-media.yml")
            && text.contains("150-app-copy.yml"),
        "{}",
        text
    );
}

/// A route file that is not YAML is a problem, not a silent pass: a file the
/// check cannot read is one whose hostnames it never compared.
/// covers: fix-92
#[test]
fn a_route_file_that_does_not_parse_is_refused() {
    let tmp = std::env::temp_dir().join(format!("homelab-fix92-yaml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let base = tmp.join("stacks");
    stack(&base, "media", 106, "fin.kp-soft.dev");
    write(&base.join("media/traefik-routes.yml"), "http: [unclosed\n");
    let problems = homelab_client::spec::fleet_route_problems(&base);
    let _ = std::fs::remove_dir_all(&tmp);
    let problems = problems.unwrap();
    assert_eq!(problems.len(), 1, "{:?}", problems);
    assert!(problems[0].contains("106-app-media.yml"), "{}", problems[0]);
}

/// Hostnames come from every router rule: a second router on the same host
/// (almanac's `/metrics` block) repeats it, both names of the older
/// `Host(a, b)` form count, and `HostSNI` is not a hostname.
/// covers: fix-92
#[test]
fn hostnames_are_read_from_every_router_rule() {
    let content = "http:\n  routers:\n    a:\n      rule: \"Host(`Almanac.kp-soft.dev`)\"\n    b:\n      rule: \"Host(`almanac.kp-soft.dev`) && PathPrefix(`/metrics`)\"\n    c:\n      rule: \"Host(`x.kp-soft.dev`, `y.kp-soft.dev`)\"\ntcp:\n  routers:\n    m:\n      rule: \"HostSNI(`*`)\"\n";
    assert_eq!(
        homelab_client::routes::hostnames(content).unwrap(),
        vec![
            "almanac.kp-soft.dev",
            "almanac.kp-soft.dev",
            "x.kp-soft.dev",
            "y.kp-soft.dev"
        ]
    );
}

/// checks-link (Kenny, 2026-09-30: "een link naar die toepassing in de
/// notificatie"): a manual check carries its application's address, read
/// from the router whose service is that app. The repository's own media
/// stack: Sonarr is son.kp-soft.dev, and an app with no router has none.
#[test]
fn checks_link_a_manual_check_carries_the_address_of_its_app() {
    let spec = homelab_client::spec::build_spec(&stacks().join("media")).unwrap();
    assert_eq!(
        spec.checks["sonarr"].url.as_deref(),
        Some("https://son.kp-soft.dev")
    );
    assert_eq!(
        spec.checks["jellyfin"].url.as_deref(),
        Some("https://fin.kp-soft.dev")
    );
    let addresses = homelab_client::routes::service_addresses(
        "http:\n  routers:\n    a:\n      rule: \"Host(`A.example`) && PathPrefix(`/x`)\"\n      service: app@file\n    b:\n      rule: \"Host(`b.example`)\"\n      service: app\n",
    )
    .unwrap();
    assert_eq!(
        addresses["app"], "https://a.example",
        "first router wins, provider dropped"
    );
}

// ── tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30) ──────
//
// The plain backend address behind a routed hostname — Traefik's own
// forwarding table, read the same way `service_addresses` already reads
// the display address, but following through to `loadBalancer.servers`
// instead of stopping at `https://<host>/`.

/// A router whose `Host(...)` matches resolves to its service's first
/// server url.
#[test]
fn backend_for_host_matches_a_router_and_its_service() {
    let content = "http:\n  routers:\n    jobtracker:\n      rule: \"Host(`job.kp-soft.dev`)\"\n      service: jobtracker\n  services:\n    jobtracker:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.16:8787\"\n";
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "job.kp-soft.dev"),
        Some("http://10.10.10.16:8787".to_string())
    );
    // Case-insensitive, like every other reader of Host(...).
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "JOB.kp-soft.dev"),
        Some("http://10.10.10.16:8787".to_string())
    );
}

/// No router in the file names the host: no backend, not an error.
#[test]
fn backend_for_host_none_when_no_router_matches() {
    let content = "http:\n  routers:\n    a:\n      rule: \"Host(`a.kp-soft.dev`)\"\n      service: a\n  services:\n    a:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.1:80\"\n";
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "b.kp-soft.dev"),
        None
    );
}

/// Several hosts and routers in one file: each resolves to its own
/// service's backend, not the first router's.
#[test]
fn backend_for_host_picks_the_right_router_among_several() {
    let content = "http:\n  routers:\n    a:\n      rule: \"Host(`a.kp-soft.dev`)\"\n      service: svc-a\n    b:\n      rule: \"Host(`b.kp-soft.dev`)\"\n      service: svc-b\n  services:\n    svc-a:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.1:8080\"\n    svc-b:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.2:9090\"\n";
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "a.kp-soft.dev"),
        Some("http://10.10.10.1:8080".to_string())
    );
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "b.kp-soft.dev"),
        Some("http://10.10.10.2:9090".to_string())
    );
}

/// A service with an `@provider` suffix (as `service_addresses` already
/// strips) and a service with no servers at all: the first has a backend,
/// the second has none.
#[test]
fn backend_for_host_strips_provider_and_handles_no_servers() {
    let content = "http:\n  routers:\n    a:\n      rule: \"Host(`a.kp-soft.dev`)\"\n      service: svc@file\n    b:\n      rule: \"Host(`b.kp-soft.dev`)\"\n      service: empty\n  services:\n    svc:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.1:8080\"\n    empty:\n      loadBalancer:\n        servers: []\n";
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "a.kp-soft.dev"),
        Some("http://10.10.10.1:8080".to_string())
    );
    assert_eq!(
        homelab_client::routes::backend_for_host(content, "b.kp-soft.dev"),
        None
    );
}

// ── tile-watch probe resolution (owner decision "Afgeleid uit de tegels",
// 2026-09-30) ───────────────────────────────────────────────────────────
//
// `build_spec` fills each tile's `probe`: an explicit own-IP url as-is, a
// Traefik-hostname tile resolved through the stack's own route file, or
// neither — and says why in a note when it resolves nothing.

#[test]
fn tile_probe_own_ip_url_as_is_routed_hostname_via_backend_and_unresolved_noted() {
    let tmp = std::env::temp_dir().join(format!("homelab-tile-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dir = tmp.join("x");
    write(
        &dir.join("lxc-compose.yml"),
        "stack_name: x\nvmid: 150\nhostname: 150-app-x\n\
         network: {ip: 10.10.10.50/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}\n\
         resources: {cores: 1, memory_mb: 512, swap_mb: 256, disk_gb: 4, storage: local-lvm}\n\
         lxc: {template: 'clone:998', unprivileged: true, features: 'nesting=1', protection: false, gpu: false, vpn: false}\n\
         boot: {onboot: true, order: 50}\nstorage: []\napps: [app]\n\
         gateway_route:\n  filename: 150-app-x.yml\n\
         tiles:\n  \
           own-ip:\n    name: Own\n    group: G\n    url: \"http://10.10.10.50:8080/\"\n  \
           x.kp-soft.dev:\n    name: Routed\n    group: G\n  \
           nowhere.kp-soft.dev:\n    name: Unresolved\n    group: G\n",
    );
    write(&dir.join("app/docker-compose.yml"), "services: {}\n");
    write(
        &dir.join("traefik-routes.yml"),
        "http:\n  routers:\n    r:\n      rule: \"Host(`x.kp-soft.dev`)\"\n      service: s\n  services:\n    s:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.50:9090\"\n",
    );
    let spec = homelab_client::spec::build_spec(&dir).unwrap();
    let tiles = &spec.manifest.tiles;
    assert_eq!(
        tiles["own-ip"].probe.as_deref(),
        Some("http://10.10.10.50:8080/"),
        "an explicit own-IP url is used as-is"
    );
    assert_eq!(
        tiles["x.kp-soft.dev"].probe.as_deref(),
        Some("http://10.10.10.50:9090"),
        "a routed hostname resolves through the stack's own traefik-routes.yml"
    );
    assert_eq!(
        tiles["nowhere.kp-soft.dev"].probe, None,
        "no route in this stack forwards nowhere.kp-soft.dev to anything"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
