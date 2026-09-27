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

/// The repository as it stands passes, and it now accounts for all 25
/// public hostnames measured on the gateway on 2026-09-27 (Traefik's API:
/// 26 file routers, 25 hostnames).
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
    assert_eq!(hosts.len(), 25, "{:?}", hosts);
    for h in [
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
