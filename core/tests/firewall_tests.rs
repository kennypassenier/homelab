//! fix-88 (expert panel 2026-09-27, flat-vlan-no-east-west-control and
//! traefik-lan-host-header-bypass): a container's Proxmox firewall is
//! declared in its stack file, rendered by a pure function, written by the
//! deploy only when it changed, and held against pve by the fleet check.
//!
//! Kenny's answer "samenvoegen-firewall-per-container" (2026-09-27), with his
//! standing rule that nothing on the machines may be hand-made and unknown to
//! the repository. The one ruleset that existed, CT 116's, was exactly that:
//! written by hand on pve on 2026-09-20 and known to no file here.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::firewall::{self, fw_path, render};
use homelab_core::manifest::*;
use homelab_core::ops::fleetcheck::{evaluate_firewalls, BootFact, FirewallFact, Severity};
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::{PipelineEvent, VecSink};
use homelab_core::state::{HostState, StackState};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn fw(yaml: &str) -> FirewallSpec {
    serde_yaml::from_str(yaml).unwrap_or_else(|e| panic!("test declaration: {}", e))
}

/// CT 116's hand-made file, declared rule for rule. The comments are the
/// live file's own words, so the rendering can be compared byte for byte.
const KP_SOFT_116: &str = r#"
enabled: true
comment: |-
  /etc/pve/firewall/116.fw — CT 116 (kp-soft), the one container strangers
  reach. Stateful, applied on its veth, so it sees NEIGHBOUR traffic inside
  VLAN 10 that no OPNsense rule can (F183). Every rule below comes from a
  measurement of 2026-09-20: ss on the container (no neighbour connections),
  the containers' own configuration (almanac, registry cache), the gateway
  route (8080, 8787), the Prometheus targets (8081, 9100) and the Uptime Kuma
  ping. Kenny approved it on 2026-09-20 ("Klopt").
policy_in: DROP
policy_out: ACCEPT
management_open: OPNsense's own rule for CT 116 already closes the management network at the router
rules:
  - comment: "1 · pve's rescue address first: never from the container that strangers reach"
    dir: out
    action: DROP
    dest: 10.10.10.250
  - comment: "2 · the neighbours it really uses: almanac (JobTracker events), registry cache (docker pulls)"
    dir: out
    action: ACCEPT
    dest: 10.10.10.12
    proto: tcp
    dport: 8080
  - dir: out
    action: ACCEPT
    dest: 10.10.10.17
    proto: tcp
    dport: "5000:5003"
  - dir: out
    action: ACCEPT
    dest: 10.10.10.4
    proto: tcp
    dport: 3100
    note: Loki push (Alloy), added 2026-09-25
  - comment: |-
      3 · every other neighbour in VLAN 10 stays out of reach; what leaves the
          VLAN (DNS 10.10.5.1, internet) crosses the router, where OPNsense rules apply
    dir: out
    action: DROP
    dest: 10.10.10.0/24
  - comment: |-
      4 · who may reach the container: Traefik (site, JobTracker), Prometheus
          (cadvisor, node exporter), Uptime Kuma (ping), Kenny's desktop (ssh)
    dir: in
    action: ACCEPT
    source: 10.10.10.4
    proto: tcp
    dport: "8080,8787"
  - dir: in
    action: ACCEPT
    source: 10.10.10.13
    proto: tcp
    dport: "8081,9100"
  - dir: in
    action: ACCEPT
    source: 10.10.10.7
    proto: icmp
  - dir: in
    action: ACCEPT
    source: 10.10.10.10
    proto: tcp
    dport: 22
"#;

/// The first line every rendered file carries, so whoever opens it on pve
/// learns where it comes from before editing it by hand.
fn provenance(stack: &str) -> String {
    format!(
        "# Written by homelab from stacks/{}/lxc-compose.yml (fix-88): edit that file, not this one\n",
        stack
    )
}

/// The renderer reproduces the hand-made 116.fw byte for byte (the captured
/// copy was compared with pve's by `cmp` on 2026-09-27 and is identical),
/// plus one provenance line. A declaration can therefore take over a live
/// ruleset without changing a single rule.
/// covers: fix-88
#[test]
fn the_renderer_reproduces_the_hand_made_116_fw_byte_for_byte() {
    let live =
        std::fs::read_to_string(repo_root().join("captured/pve-host/firewall/116.fw")).unwrap();
    let got = render("kp-soft", &fw(KP_SOFT_116));
    assert_eq!(got, format!("{}{}", provenance("kp-soft"), live));
}

/// Without `management_open`, the renderer appends the management guard
/// after the declared rules: DNS to the router, nothing else on
/// 10.10.5.0/24. After, so a declared ACCEPT towards the management network
/// (pve-exporter to pve's API) is matched first and still passes.
/// covers: fix-88
#[test]
fn the_management_network_is_closed_after_the_declared_rules_unless_opened_with_a_reason() {
    let spec = fw(r#"
enabled: true
comment: demo
rules:
  - dir: out
    action: ACCEPT
    dest: 10.10.5.250
    proto: tcp
    dport: 8006
    note: pve API for pve-exporter
  - comment: Prometheus scrapes node exporter
    dir: in
    action: ACCEPT
    source: 10.10.10.13
    proto: tcp
    dport: 9100
"#);
    let want = format!(
        "{}\
# demo
[OPTIONS]
enable: 1
policy_in: DROP
policy_out: ACCEPT
log_level_in: nolog
log_level_out: nolog

[RULES]
OUT ACCEPT -dest 10.10.5.250 -p tcp -dport 8006 # pve API for pve-exporter
# Prometheus scrapes node exporter
IN ACCEPT -source 10.10.10.13 -p tcp -dport 9100
# management network: DNS to the router, nothing else (flat-vlan-no-east-west-control,
# traefik-lan-host-header-bypass, 2026-09-27)
OUT ACCEPT -dest 10.10.5.1 -p udp -dport 53
OUT ACCEPT -dest 10.10.5.1 -p tcp -dport 53
OUT DROP -dest 10.10.5.0/24
",
        provenance("demo")
    );
    assert_eq!(render("demo", &spec), want);

    let mut open = spec.clone();
    open.management_open = Some("this container is the management network's own".into());
    let got = render("demo", &open);
    assert!(
        !got.contains("10.10.5.0/24"),
        "an opened management network gets no guard:\n{}",
        got
    );
}

/// A declaration the renderer cannot honour is refused before anything is
/// written, each problem naming the rule and the field, with the remedy.
/// covers: fix-88
#[test]
fn a_declaration_proxmox_would_misread_is_refused_with_the_rule_named() {
    let spec = fw(r#"
enabled: true
management_open: short
rules:
  - dir: in
    action: ACCEPT
    source: 10.10.10.4/24
    proto: tcp
    dport: 80
  - dir: out
    action: ACCEPT
    dest: 10.10.10.17
    proto: icmp
    dport: 5000
  - dir: out
    action: ACCEPT
    dest: 10.10.10.300
  - dir: out
    action: ACCEPT
    dest: 10.10.10.4
    dport: 3100
  - dir: out
    action: ACCEPT
    dest: 10.10.10.4
    proto: tcp
    dport: "70000,5003:5000"
  - dir: out
    action: DROP
    dest: 10.10.10.250
    note: "two\nlines"
"#);
    let p = firewall::problems(&spec).join("\n");
    for needle in [
        "management_open",
        "rule 1",
        "10.10.10.4/24",
        "10.10.10.0/24",
        "rule 2",
        "icmp",
        "rule 3",
        "10.10.10.300",
        "rule 4",
        "without proto",
        "rule 5",
        "70000",
        "5003:5000",
        "rule 6",
        "note",
    ] {
        assert!(p.contains(needle), "missing '{}' in:\n{}", needle, p);
    }

    // The same problems stop the manifest as a whole.
    let mut m = serde_yaml::from_str::<StackManifest>(
        &std::fs::read_to_string(repo_root().join("stacks/registry/lxc-compose.yml")).unwrap(),
    )
    .unwrap();
    m.firewall = Some(spec);
    let err = validate_manifest(&m).expect_err("refused").to_string();
    assert!(err.contains("firewall rule 3"), "{}", err);

    // And a clean declaration passes.
    assert!(firewall::problems(&fw(KP_SOFT_116)).is_empty());
}

/// traefik-lan-host-header-bypass: Traefik answers a forged Host header with
/// the Proxmox and OPNsense logins (measured from CT 107, 2026-09-27). The
/// tunnel reaches port 80 over CT 104's own docker network, which never
/// crosses the container's veth, so on the gateway no rule may open port 80
/// to anyone, and nothing may be accepted by policy.
/// covers: fix-88
#[test]
fn the_gateway_may_not_open_port_80_to_its_neighbours() {
    let bad = fw(r#"
enabled: true
rules:
  - dir: in
    action: ACCEPT
    source: 10.10.10.7
    proto: tcp
    dport: "79:81"
  - dir: in
    action: ACCEPT
    source: 10.10.10.13
  - dir: in
    action: ACCEPT
    source: 10.10.10.13
    proto: tcp
    dport: 3000
"#);
    let p = firewall::gateway_problems(&bad).join("\n");
    assert!(p.contains("rule 1") && p.contains("rule 2"), "{}", p);
    assert!(!p.contains("rule 3"), "port 3000 is not port 80: {}", p);

    let mut open = bad.clone();
    open.rules.clear();
    open.policy_in = FwAction::Accept;
    assert!(!firewall::gateway_problems(&open).is_empty());
}

// ── the deploy ──────────────────────────────────────────────────────────────

fn manifest(vmid: u16, stack: &str, spec: Option<FirewallSpec>) -> StackManifest {
    StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        syslog_receivers: vec![],
        firewall: spec,
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
        storage: vec![],
        apps: vec![stack.into()],
    }
}

fn spec(vmid: u16, stack: &str, fwspec: Option<FirewallSpec>) -> DeploySpec {
    DeploySpec {
        source: None,
        native_manifests: Default::default(),
        native_binaries: Default::default(),
        manifest: manifest(vmid, stack, fwspec),
        files: vec![FileBlob {
            path: format!("{}/docker-compose.yml", stack),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        extra_routes: Vec::new(),
        checks: Default::default(),
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    }
}

const NET0_OFF: &str =
    "net0: name=eth0,bridge=vmbr0,firewall=0,gw=10.10.10.1,hwaddr=BC:24:11:BD:24:E3,ip=10.10.10.16/24,tag=10,type=veth\n";
const NET0_ON: &str =
    "net0: name=eth0,bridge=vmbr0,firewall=1,gw=10.10.10.1,hwaddr=BC:24:11:BD:24:E3,ip=10.10.10.16/24,tag=10,type=veth\n";

fn script_existing(exec: &MockExecutor, vmid: u16, stack: &str, net0: &str) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(&format!(
            "hostname: {vmid}-app-{stack}\nonboot: 1\nstartup: order=50\n{net0}"
        )),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok(&format!("{}\n", stack)),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
}

fn lines(sink: &VecSink) -> Vec<String> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            PipelineEvent::Line { msg, .. } => Some(msg),
            _ => None,
        })
        .collect()
}

fn writes_to(exec: &MockExecutor, path: &str) -> usize {
    exec.calls_containing(&format!("write_file {} ", path))
        .len()
}

/// Written only when the rendering differs from what pve holds, and the
/// transcript says which: unchanged, or exactly which lines came and went.
/// covers: fix-88
#[tokio::test]
async fn a_deploy_writes_the_firewall_only_when_it_changed_and_says_so() {
    let decl = fw(KP_SOFT_116);
    let path = fw_path(116);
    let rendered = render("kp-soft", &decl);

    // Identical on pve: no write.
    let exec = MockExecutor::new();
    script_existing(&exec, 116, "kp-soft", NET0_ON);
    exec.seed_file(&path, &rendered);
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(decl.clone())),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(writes_to(&exec, &path), 0, "{:?}", exec.calls());
    let log = lines(&sink).join("\n");
    assert!(
        log.contains("[firewall] /etc/pve/firewall/116.fw unchanged"),
        "{}",
        log
    );

    // One rule more in the stack file: written, and the new line is named.
    let mut more = decl.clone();
    more.rules.push(
        fw(r#"
enabled: true
rules:
  - dir: in
    action: ACCEPT
    source: 10.10.10.7
    proto: tcp
    dport: "8080,8787"
"#)
        .rules
        .remove(0),
    );
    let exec = MockExecutor::new();
    script_existing(&exec, 116, "kp-soft", NET0_ON);
    exec.seed_file(&path, &rendered);
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(more.clone())),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.file(&path).as_deref(),
        Some(render("kp-soft", &more).as_str())
    );
    assert_eq!(
        exec.file_mode(&path),
        Some(0o640),
        "pmxcfs accepts 0640 only"
    );
    let log = lines(&sink).join("\n");
    assert!(
        log.contains("[firewall] /etc/pve/firewall/116.fw written: +1 -0")
            && log.contains("+ IN ACCEPT -source 10.10.10.7 -p tcp -dport 8080,8787"),
        "{}",
        log
    );
    assert!(
        exec.calls_containing("--net0").is_empty(),
        "the NIC flag was already on: {:?}",
        exec.calls()
    );
}

/// A new container starts behind its firewall: the file is written before
/// the container exists and the NIC is created with `firewall=1`. Without
/// the flag Proxmox applies no rule at all — CT 116's flag was set by hand,
/// so a rebuild of it would have come up unprotected.
/// covers: fix-88
#[tokio::test]
async fn a_new_container_is_created_behind_its_firewall() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("kp-soft\n"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(fw(KP_SOFT_116))),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let write = calls
        .iter()
        .position(|c| c.starts_with(&format!("write_file {} ", fw_path(116))))
        .expect("the firewall file is written");
    let create = calls
        .iter()
        .position(|c| c.contains("pct create 116"))
        .expect("the container is created");
    assert!(write < create, "{:?}", calls);
    assert!(calls[create].contains("firewall=1"), "{}", calls[create]);

    // No declaration: the NIC keeps the flag it always had.
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("demo\n"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    let sink = VecSink::new();
    let report = deploy(&ctx(&exec, &sink, &NullJournal), &spec(116, "demo", None)).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec
        .calls_containing("pct create 116")
        .iter()
        .all(|c| c.contains("firewall=0")));
    assert_eq!(writes_to(&exec, &fw_path(116)), 0);
}

/// fix-154 (2026-09-28 07:09, the Debian 13 rebuild of kp-soft): the file
/// written before the container existed was gone once `pct clone` had made
/// CT 116 — Proxmox drops a vmid's firewall file when it creates that vmid —
/// so the one container strangers reach came up with `firewall=1` on its NIC
/// and no rules at all. The file is written again after the container is
/// created, and the transcript says so.
/// covers: fix-154
#[tokio::test]
async fn fix_154_the_firewall_is_written_again_after_the_container_is_created() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("kp-soft\n"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(fw(KP_SOFT_116))),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let create = calls
        .iter()
        .position(|c| c.contains("pct create 116"))
        .expect("the container is created");
    let after = calls[create..]
        .iter()
        .any(|c| c.starts_with(&format!("write_file {} ", fw_path(116))));
    assert!(after, "no firewall write after the create: {:?}", calls);
    assert!(
        lines(&sink)
            .iter()
            .any(|l| l.contains("[firewall]") && l.contains("after the container was created")),
        "{:?}",
        lines(&sink)
    );
}

/// An existing container whose NIC has the flag off gets it switched on, with
/// every other part of its net0 (the MAC address above all) kept as it was.
/// covers: fix-88
#[tokio::test]
async fn an_enabled_firewall_switches_the_nic_flag_on_and_keeps_the_rest_of_net0() {
    assert_eq!(
        firewall::net0_with_firewall(NET0_OFF.trim_start_matches("net0: ").trim()).as_deref(),
        Some(NET0_ON.trim_start_matches("net0: ").trim())
    );
    assert_eq!(
        firewall::net0_with_firewall(NET0_ON.trim_start_matches("net0: ").trim()),
        None
    );
    assert_eq!(
        firewall::net0_with_firewall("name=eth0,bridge=vmbr0,hwaddr=AA,ip=10.10.10.9/24")
            .as_deref(),
        Some("name=eth0,bridge=vmbr0,hwaddr=AA,ip=10.10.10.9/24,firewall=1")
    );

    let exec = MockExecutor::new();
    script_existing(&exec, 116, "kp-soft", NET0_OFF);
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(fw(KP_SOFT_116))),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let set = exec.calls_containing("--net0");
    assert_eq!(set.len(), 1, "{:?}", exec.calls());
    assert!(
        set[0].contains("pct set 116 --net0")
            && set[0].contains("firewall=1")
            && set[0].contains("hwaddr=BC:24:11:BD:24:E3"),
        "{}",
        set[0]
    );
}

/// `enabled: false` is a declaration kept for the rollout, not applied: no
/// file, no NIC change, and the transcript says so.
/// covers: fix-88
#[tokio::test]
async fn a_declaration_that_is_not_enabled_changes_nothing_on_pve() {
    let mut off = fw(KP_SOFT_116);
    off.enabled = false;
    let exec = MockExecutor::new();
    script_existing(&exec, 116, "kp-soft", NET0_OFF);
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(116, "kp-soft", Some(off)),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(writes_to(&exec, &fw_path(116)), 0);
    assert!(exec.calls_containing("--net0").is_empty());
    let log = lines(&sink).join("\n");
    assert!(log.contains("[firewall] declared, not enabled"), "{}", log);
}

/// On the gateway, a rule opening port 80 is refused before anything is
/// written, which is what closes the Host-header route to the logins.
/// covers: fix-88
#[tokio::test]
async fn the_gateway_deploy_refuses_a_firewall_that_opens_port_80() {
    let bad = fw(r#"
enabled: true
rules:
  - dir: in
    action: ACCEPT
    source: 10.10.10.7
    proto: tcp
    dport: 80
"#);
    let exec = MockExecutor::new();
    script_existing(&exec, 104, "gateway", NET0_ON);
    let sink = VecSink::new();
    let report = deploy(
        &ctx(&exec, &sink, &NullJournal),
        &spec(104, "gateway", Some(bad)),
    )
    .await;
    assert!(!report.ok);
    assert!(
        format!("{:?}", report.error).contains("port 80"),
        "{:?}",
        report.error
    );
    assert_eq!(writes_to(&exec, &fw_path(104)), 0);
}

// ── the fleet check ─────────────────────────────────────────────────────────

fn state_with(stack: &str, vmid: u16, spec: Option<FirewallSpec>) -> HostState {
    let m = manifest(vmid, stack, spec);
    let mut st = HostState::default();
    st.stacks.insert(
        stack.into(),
        StackState {
            applied_source: None,
            vmid,
            hostname: m.hostname.clone(),
            apps: m.apps.clone(),
            applied_at: 1,
            last_backup: 1,
            applied_hash: String::new(),
            manifest: Some(m),
            enabled: true,

            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
        },
    );
    st
}

fn boot(vmid: u16, net0: &str) -> BootFact {
    BootFact {
        vmid,
        hostname: String::new(),
        live: homelab_core::ops::reconcile::parse(net0),
    }
}

/// `homelab check` holds every managed container's `.fw` on pve against its
/// declaration: a hand edit, a missing file, a NIC with the flag off and a
/// file the repository does not declare are each a finding with a remedy;
/// declarations kept for the rollout are noted in one line.
/// covers: fix-88
#[test]
fn the_fleet_check_reports_a_firewall_file_that_differs_from_its_declaration() {
    let decl = fw(KP_SOFT_116);
    let rendered = render("kp-soft", &decl);
    let fact = |content: Option<&str>| FirewallFact {
        stack: "kp-soft".into(),
        vmid: 116,
        content: content.map(str::to_string),
    };

    let st = state_with("kp-soft", 116, Some(decl.clone()));
    // Equal, flag on: nothing to say.
    assert!(evaluate_firewalls(&st, &[fact(Some(&rendered))], &[boot(116, NET0_ON)]).is_empty());

    // A hand edit on pve.
    let edited = rendered.replace("-dport 22", "-dport 2222");
    let f = evaluate_firewalls(&st, &[fact(Some(&edited))], &[boot(116, NET0_ON)]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(
        f[0].what.contains("differs from its declaration") && f[0].what.contains("2222"),
        "{:?}",
        f[0]
    );

    // Declared but absent, and a NIC that applies nothing.
    let f = evaluate_firewalls(&st, &[fact(None)], &[boot(116, NET0_OFF)]);
    assert_eq!(f.len(), 2, "{:?}", f);
    assert!(f.iter().any(|x| x.what.contains("absent")));
    assert!(f.iter().any(|x| x.what.contains("firewall=0")));

    // A file the repository does not declare.
    let bare = state_with("kp-soft", 116, None);
    let f = evaluate_firewalls(&bare, &[fact(Some(&rendered))], &[]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("does not enable"), "{:?}", f[0]);

    // Declared for the rollout, not enabled, nothing on pve: one noted line.
    let mut off = decl.clone();
    off.enabled = false;
    let st = state_with("kp-soft", 116, Some(off));
    let f = evaluate_firewalls(&st, &[fact(None)], &[boot(116, NET0_OFF)]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Noted);
    assert!(f[0].what.contains("kp-soft"), "{:?}", f[0]);
}

/// The gatherer reads each recorded stack's file off pve, and says absent
/// when there is none.
/// covers: fix-88
#[tokio::test]
async fn the_facts_read_every_recorded_stacks_firewall_file() {
    use homelab_core::ops::facts::{gather_live_facts, FactsInputs};
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::json!({
            "schema_version": 1,
            "stacks": {
                "kp-soft": {"vmid": 116, "hostname": "116-app-kp-soft", "apps": ["kp-soft"], "applied_at": 1, "manifest": null},
                "kyu": {"vmid": 109, "hostname": "109-app-kyu", "apps": [], "applied_at": 1, "manifest": null}
            }
        })
        .to_string(),
    );
    exec.seed_file(&fw_path(116), "[OPTIONS]\nenable: 1\n");
    let inp = FactsInputs {
        watched_backups: vec![],
        kuma_monitors_file: None,
        state_dir: "/var/lib/homelab".into(),
        grafana_vmid: 104,
        loki_vmid: None,
        gateway_vmid: 104,
        gateway_routes_dir: "/appdata/gateway/traefik-config/routes".into(),
        no_touch: vec![100, 101],
        prometheus_url: None,
        loki_url: None,
        logs_window: "24h".into(),
        grafana_dashboards_dir: None,
        now_unix: 1_789_704_000,
        watched_fresh: true,
    };
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    let mut got: Vec<(u16, Option<String>)> = facts
        .firewalls
        .iter()
        .map(|f| (f.vmid, f.content.clone()))
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![(109, None), (116, Some("[OPTIONS]\nenable: 1\n".into()))]
    );
}
