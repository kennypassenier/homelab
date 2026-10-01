//! fix-94 (Kenny, triage 2026-09-27, crowdsec-home-ip: "Thuisadres
//! automatisch vrijstellen"): since fix-45 CrowdSec sees real visitor
//! addresses, the house's own public one among them (62.235.8.143, measured
//! 2026-09-27). The orchestrator reads the router's WAN address and keeps it
//! in a CrowdSec whitelist on the gateway, so a burst of the house's own
//! traffic can never ban the house.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::OpCtx;
use homelab_core::ops::fleetcheck::{Severity, alarming};
use homelab_core::ops::homeaddress::{
    evaluate_home_address, parse_public, sync_home_address, whitelist_yaml, whitelisted_address,
};
use homelab_core::ops::util::staging_path;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::HostState;

const NOW: u64 = 1_790_000_000;

/// app-knowledge (2026-09-30): the gateway stack file declares the whitelist.
const WHITELIST_FILE: &str =
    "/appdata/gateway/crowdsec-config/parsers/s02-enrich/homelab-home-address.yaml";

/// Host state as a gateway deploy leaves it: the repository's own gateway
/// stack file, whose `home_address_whitelist` names the file, the test and
/// the reload.
async fn seed_gateway(exec: &MockExecutor) {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/gateway/lxc-compose.yml"),
    )
    .unwrap();
    let m: homelab_core::manifest::StackManifest = serde_yaml::from_str(&text).unwrap();
    assert_eq!(
        m.home_address_whitelist.as_ref().map(|w| w.file.as_str()),
        Some(WHITELIST_FILE)
    );
    let mut st = HostState::default();
    st.stacks.insert(
        "gateway".into(),
        homelab_core::state::StackState {
            pushed_file_hashes: std::collections::BTreeMap::new(),
            applied_source: None,
            vmid: m.vmid,
            hostname: m.hostname.clone(),
            apps: Vec::new(),
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(m),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
        },
    );
    homelab_core::state::StateStore::new(exec, "/var/lib/homelab")
        .save(st)
        .await
        .unwrap();
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
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
/// A new address rewrites the file and reloads CrowdSec, and the log says
/// from what to what. The read reuses the device backup's credential file and
/// pin, keeps the credential out of argv, and keeps the router's interface
/// list out of the transcript.
#[tokio::test]
async fn a_changed_address_rewrites_the_whitelist_and_reloads_crowdsec() {
    let exec = MockExecutor::new();
    seed_gateway(&exec).await;
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.100"))),
    );
    exec.respond_always(
        "cdn-cgi/trace",
        CmdOutput::ok("fl=1\nh=cloudflare.com\nip=62.235.8.143\nts=1\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.contains("https://cloudflare.com/cdn-cgi/trace")),
        "cloudflare was never asked: {:#?}",
        calls
    );
    assert!(
        !calls.iter().any(|c| c.contains("10.10.10.1")),
        "the router is not asked any more: {:#?}",
        calls
    );

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
    seed_gateway(&exec).await;
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.143"))),
    );
    exec.respond_always(
        "cdn-cgi/trace",
        CmdOutput::ok("fl=1\nh=cloudflare.com\nip=62.235.8.143\nts=1\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
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
    seed_gateway(&exec).await;
    exec.respond_always(
        "cdn-cgi/trace",
        CmdOutput::ok("fl=1\nh=cloudflare.com\nip=62.235.8.143\nts=1\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls().iter().any(|c| c.contains("HUP")));
    assert!(
        sink.lines()
            .join("\n")
            .contains("62.235.8.143 whitelisted (none was before)")
    );
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
    seed_gateway(&exec).await;
    exec.respond_always(
        &format!("cat '{}'", WHITELIST_FILE),
        CmdOutput::ok(&whitelist_yaml(home("62.235.8.143"))),
    );
    for source in ["cdn-cgi/trace", "api.ipify.org"] {
        exec.respond_always(
            source,
            CmdOutput {
                stdout: String::new(),
                stderr: "curl: (22) The requested URL returned error: 403".into(),
                code: 22,
            },
        );
    }
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
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
    seed_gateway(&exec).await;
    let before = whitelist_yaml(home("62.235.8.100"));
    exec.respond_always(&format!("cat '{}'", WHITELIST_FILE), CmdOutput::ok(&before));
    exec.respond_always(
        "cdn-cgi/trace",
        CmdOutput::ok("fl=1\nh=cloudflare.com\nip=62.235.8.143\nts=1\n"),
    );
    exec.respond_always(
        "crowdsec -c /etc/crowdsec/config.yaml -t",
        CmdOutput::failed(1, "failed to load parser"),
    );
    let sink = VecSink::new();
    let j = NullJournal;

    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
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

/// fix-156: Cloudflare's trace and ipify's bare answer both read; a private
/// or garbled answer is refused, never whitelisted.
/// covers: fix-156
#[test]
fn fix_156_a_public_service_answer_is_read_and_checked() {
    assert_eq!(
        parse_public("fl=1\nh=cloudflare.com\nip=62.235.8.143\nts=1\n").unwrap(),
        home("62.235.8.143")
    );
    assert_eq!(
        parse_public("62.235.8.143\n").unwrap(),
        home("62.235.8.143")
    );
    assert!(
        parse_public("ip=192.168.1.10")
            .unwrap_err()
            .contains("not a public")
    );
    assert!(parse_public("<html>blocked</html>").is_err());
}

/// fix-156: when Cloudflare does not answer, ipify does.
/// covers: fix-156
#[tokio::test]
async fn fix_156_ipify_answers_when_cloudflare_does_not() {
    let exec = MockExecutor::new();
    seed_gateway(&exec).await;
    exec.respond_always(&format!("cat '{}'", WHITELIST_FILE), CmdOutput::ok(""));
    exec.respond_always(
        "cdn-cgi/trace",
        CmdOutput {
            stdout: String::new(),
            stderr: "timeout".into(),
            code: 28,
        },
    );
    exec.respond_always("api.ipify.org", CmdOutput::ok("62.235.8.143"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = sync_home_address(&ctx(&exec, &sink, &j)).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        saved_state(&exec).home_address.as_deref(),
        Some("62.235.8.143")
    );
}

/// fix-156: no address at all is broken (the house can be banned and the
/// dashboard refuses it); a last known one is only noted.
/// covers: fix-156
#[test]
fn fix_156_no_address_is_broken_a_last_known_one_is_noted() {
    use homelab_core::ops::fleetcheck::Severity;
    let none = HostState {
        home_address_error: Some("timeout".into()),
        home_address_checked: NOW,
        ..Default::default()
    };
    assert_eq!(evaluate_home_address(&none)[0].severity, Severity::Broken);
    let kept = HostState {
        home_address: Some("62.235.8.143".into()),
        ..none
    };
    assert_eq!(evaluate_home_address(&kept)[0].severity, Severity::Noted);
}
