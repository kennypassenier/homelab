//! fix-94 (Kenny, triage 2026-09-27, crowdsec-home-ip: "Thuisadres
//! automatisch vrijstellen"): since fix-45 CrowdSec sees real visitor
//! addresses, the house's own public one among them (62.235.8.143, measured
//! 2026-09-27). The orchestrator reads the router's WAN address and keeps it
//! in a CrowdSec whitelist on the gateway, so a burst of the house's own
//! traffic can never ban the house.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::devicebackup::DeviceBackup;
use homelab_core::ops::fleetcheck::{alarming, Severity};
use homelab_core::ops::homeaddress::{
    evaluate_home_address, router, sync_home_address, wan_address, whitelist_yaml,
    whitelisted_address, WHITELIST_FILE,
};
use homelab_core::ops::util::staging_path;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::HostState;

const NOW: u64 = 1_790_000_000;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
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

/// The device-backup entry host.toml carries on pve today (read 2026-09-27):
/// its credential and pin are what the address read reuses.
fn opnsense() -> DeviceBackup {
    DeviceBackup {
        name: "opnsense".into(),
        url: "https://10.10.10.1/api/core/backup/download/this".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
        filename: "config.xml".into(),
        pin: Some("sha256//oZKgUOWR56fT3HYG68aGVn7s1saleArMf75StP1KaUE=".into()),
        ca_file: None,
    }
}

/// The shape `GET /api/interfaces/overview/interfacesInfo` returned on
/// 2026-09-27, cut down to the fields that matter.
fn interfaces(wan: &str) -> String {
    format!(
        r#"{{"total":3,"rowCount":3,"current":1,"rows":[
{{"identifier":"lan","description":"LAN","device":"vtnet0","ipv4":[{{"ipaddr":"10.10.5.1/24"}}],"addr4":"10.10.5.1/24","addr6":""}},
{{"identifier":"wan","description":"WAN","device":"vtnet1","ipv4":[{{"ipaddr":"{wan}"}}],"addr4":"{wan}","addr6":""}},
{{"identifier":"","description":"Unassigned Interface","device":"enc0","ipv4":[],"addr4":null,"addr6":null}}
]}}"#
    )
}

fn home(addr: &str) -> std::net::Ipv4Addr {
    addr.parse().unwrap()
}

fn saved_state(exec: &MockExecutor) -> HostState {
    serde_json::from_str(
        &exec
            .file("/var/lib/homelab/state.json")
            .expect("state was not saved"),
    )
    .unwrap()
}

/// covers: fix-94
///
/// The router's own view of its WAN interface is the address the house
/// appears as on the internet. Anything else is refused rather than
/// whitelisted: a private or carrier-grade address there means a modem in
/// front of the router holds the public one, and whitelisting the private
/// one would exempt nobody.
#[test]
fn the_wan_address_is_read_from_the_routers_interface_list() {
    assert_eq!(
        wan_address(&interfaces("62.235.8.143/21")),
        Ok(home("62.235.8.143"))
    );

    for private in ["192.168.1.2/24", "100.64.3.9/10", "10.0.0.2/8"] {
        let err = wan_address(&interfaces(private)).unwrap_err();
        assert!(err.contains("not a public address"), "{}: {}", private, err);
    }

    let no_wan = r#"{"rows":[{"identifier":"lan","addr4":"10.10.5.1/24"}]}"#;
    assert!(wan_address(no_wan)
        .unwrap_err()
        .contains("no WAN interface"));

    let no_address = r#"{"rows":[{"identifier":"wan","addr4":null,"ipv4":[]}]}"#;
    assert!(wan_address(no_address)
        .unwrap_err()
        .contains("has no IPv4 address"));

    assert!(wan_address("<html>login</html>")
        .unwrap_err()
        .contains("not JSON"));
}

/// covers: fix-94
///
/// The whitelist file is the memory: what it names is the last known
/// address, so it has to read back what it was written with.
#[test]
fn the_whitelist_names_the_address_and_reads_it_back() {
    let yaml = whitelist_yaml(home("62.235.8.143"));
    assert!(yaml.contains("name: homelab/home-address"), "{}", yaml);
    assert!(yaml.contains("    - \"62.235.8.143\""), "{}", yaml);
    assert_eq!(whitelisted_address(&yaml), Some(home("62.235.8.143")));
    assert_eq!(whitelisted_address(""), None);
    // The static file with the private ranges is not ours to read as one.
    assert_eq!(
        whitelisted_address("whitelist:\n  cidr:\n    - \"10.0.0.0/8\"\n"),
        None
    );
}

/// covers: fix-94
///
/// The router is the device-backup entry named `opnsense`: the address read
/// reuses its credential rather than bringing a new secret.
#[test]
fn the_router_is_the_opnsense_device_backup() {
    let other = DeviceBackup {
        name: "switch".into(),
        ..opnsense()
    };
    assert_eq!(router(&[other.clone(), opnsense()]), Some(&opnsense()));
    assert_eq!(router(&[other]), None);
    assert_eq!(router(&[]), None);
}

/// covers: fix-94
///
/// A new address rewrites the file and reloads CrowdSec, and the log says
/// from what to what. The read reuses the device backup's credential file and
/// pin, keeps the credential out of argv, and keeps the router's interface
/// list out of the transcript.
#[tokio::test]
async fn a_changed_address_rewrites_the_whitelist_and_reloads_crowdsec() {
    let exec = MockExecutor::new();
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.100"))),
    );
    exec.respond_always(
        "interfacesInfo",
        CmdOutput::ok(&interfaces("62.235.8.143/21")),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j), &opnsense()).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    let curl = calls
        .iter()
        .find(|c| c.contains("interfacesInfo"))
        .expect("the router was never asked");
    assert!(
        curl.contains("-K /var/lib/homelab/secrets/opnsense-backup.conf"),
        "{}",
        curl
    );
    assert!(
        curl.contains("--pinnedpubkey sha256//oZKgUOWR56fT3HYG68aGVn7s1saleArMf75StP1KaUE="),
        "{}",
        curl
    );
    assert!(
        curl.ends_with("https://10.10.10.1/api/interfaces/overview/interfacesInfo"),
        "{}",
        curl
    );
    assert!(!curl.contains(" -u "), "{}", curl);

    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pct push 104") && c.contains(WHITELIST_FILE)),
        "the whitelist was not written: {:#?}",
        calls
    );
    let pushed = exec
        .file(&staging_path(104, WHITELIST_FILE))
        .expect("nothing was staged");
    assert_eq!(pushed, whitelist_yaml(home("62.235.8.143")));
    let test = calls
        .iter()
        .position(|c| c.contains("crowdsec -c /etc/crowdsec/config.yaml -t"))
        .expect("the new whitelist was not tested before the reload");
    let hup = calls
        .iter()
        .position(|c| c.contains("docker kill --signal=HUP crowdsec"))
        .expect("CrowdSec was not reloaded");
    assert!(test < hup, "tested after the reload: {:#?}", calls);

    let lines = sink.lines().join("\n");
    assert!(
        lines.contains("changed from 62.235.8.100 to 62.235.8.143"),
        "{}",
        lines
    );

    let st = saved_state(&exec);
    assert_eq!(st.home_address.as_deref(), Some("62.235.8.143"));
    assert_eq!(st.home_address_error, None);
    assert_eq!(st.home_address_checked, NOW);
}

/// covers: fix-94
///
/// The nightly check runs every night; a reload every night would be noise
/// and a needless risk to the one service whose absence answers 403 to the
/// whole house.
#[tokio::test]
async fn an_unchanged_address_touches_nothing() {
    let exec = MockExecutor::new();
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.143"))),
    );
    exec.respond_always(
        "interfacesInfo",
        CmdOutput::ok(&interfaces("62.235.8.143/21")),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j), &opnsense()).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    assert!(
        !calls.iter().any(|c| c.starts_with("pct push")),
        "{:#?}",
        calls
    );
    assert!(!calls.iter().any(|c| c.contains("HUP")), "{:#?}", calls);
    assert!(sink.lines().join("\n").contains("62.235.8.143 unchanged"));
    assert_eq!(
        saved_state(&exec).home_address.as_deref(),
        Some("62.235.8.143")
    );
}

/// covers: fix-94
///
/// First run: no whitelist file yet. It is written and the log says the
/// house was not exempt before.
#[tokio::test]
async fn the_first_run_writes_the_whitelist() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "interfacesInfo",
        CmdOutput::ok(&interfaces("62.235.8.143/21")),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j), &opnsense()).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls().iter().any(|c| c.contains("HUP")));
    assert!(sink
        .lines()
        .join("\n")
        .contains("62.235.8.143 whitelisted (none was before)"));
}

/// covers: fix-94
///
/// An address that cannot be read keeps the last known one: nothing is
/// written, nothing removed, CrowdSec is not touched, the operation does not
/// fail (a failed operation notifies, and this is not worth waking anyone
/// for), and the fleet check carries a noted finding that says so.
#[tokio::test]
async fn an_unreadable_address_keeps_the_last_known_one() {
    let exec = MockExecutor::new();
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.143"))),
    );
    exec.respond_always(
        "interfacesInfo",
        CmdOutput {
            stdout: r#"{"status":403,"message":"Forbidden"}"#.into(),
            stderr: "curl: (22) The requested URL returned error: 403".into(),
            code: 22,
        },
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j), &opnsense()).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    assert!(
        !calls.iter().any(|c| c.starts_with("pct push")),
        "{:#?}",
        calls
    );
    assert!(!calls.iter().any(|c| c.contains("rm ")), "{:#?}", calls);
    assert!(!calls.iter().any(|c| c.contains("HUP")), "{:#?}", calls);
    let lines = sink.lines().join("\n");
    assert!(lines.contains("keeps 62.235.8.143"), "{}", lines);

    let st = saved_state(&exec);
    assert_eq!(st.home_address.as_deref(), Some("62.235.8.143"));
    assert!(
        st.home_address_error
            .as_deref()
            .is_some_and(|e| e.contains("403")),
        "{:?}",
        st.home_address_error
    );

    let findings = evaluate_home_address(&st);
    assert_eq!(findings.len(), 1, "{:?}", findings);
    assert_eq!(findings[0].severity, Severity::Noted);
    assert!(findings[0].what.contains("62.235.8.143"), "{:?}", findings);
    assert!(alarming(&findings).is_empty());
}

/// covers: fix-94
///
/// CrowdSec exits on a reload it cannot load, and the bouncer then answers
/// 403 to everything. A whitelist its own configuration test refuses is put
/// back to what it was, and no reload is sent.
#[tokio::test]
async fn a_whitelist_crowdsec_refuses_is_put_back() {
    let exec = MockExecutor::new();
    let before = whitelist_yaml(home("62.235.8.100"));
    exec.respond_always(&format!("cat '{}'", WHITELIST_FILE), CmdOutput::ok(&before));
    exec.respond_always(
        "interfacesInfo",
        CmdOutput::ok(&interfaces("62.235.8.143/21")),
    );
    exec.respond_always(
        "crowdsec -c /etc/crowdsec/config.yaml -t",
        CmdOutput::failed(1, "failed to load parser"),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j), &opnsense()).await;
    assert!(!report.ok);

    let calls = exec.calls();
    assert!(!calls.iter().any(|c| c.contains("HUP")), "{:#?}", calls);
    let pushes = calls
        .iter()
        .filter(|c| c.starts_with("pct push 104") && c.contains(WHITELIST_FILE))
        .count();
    assert_eq!(pushes, 2, "written, then put back: {:#?}", calls);
    let staged = exec.file(&staging_path(104, WHITELIST_FILE)).unwrap();
    assert_eq!(staged, before, "the previous whitelist was not restored");
    assert_eq!(
        saved_state(&exec).home_address.as_deref(),
        Some("62.235.8.100")
    );
}

/// covers: fix-94
///
/// Nothing recorded (never checked, or checked fine) is nothing to report.
#[test]
fn a_readable_address_is_no_finding() {
    let st = HostState {
        home_address: Some("62.235.8.143".into()),
        home_address_checked: NOW,
        ..Default::default()
    };
    assert!(evaluate_home_address(&st).is_empty());
    assert!(evaluate_home_address(&HostState::default()).is_empty());
}
