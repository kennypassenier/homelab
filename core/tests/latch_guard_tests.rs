//! dashboard-latch (Kenny, 2026-09-29, form "Latch": "latch in ct120, latch
//! moet als default geinstalleerd worden in de golden images"): the deploy
//! guard installs latch on a container that lacks it, from latch-rs's signed
//! release, and the golden template gets it through the same guard.
//!
//! The fixtures are the real v2.6.0 `SHA256SUMS` and its minisign signature
//! (verified with the ecosystem key 2026-09-29), so the signature check in
//! these tests is the production one, not a stand-in.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::guards::{ensure_latch, LATCH_ASSET, LATCH_BIN, LATCH_REPO};
use homelab_core::ops::native::newest_signed_release;
use homelab_core::sink::VecSink;

const SUMS: &str = include_str!("fixtures/latch-v2.6.0/SHA256SUMS");
const SIG: &str = include_str!("fixtures/latch-v2.6.0/SHA256SUMS.minisig");
const LINUX_SHA: &str = "6c65d2cc8509bbdaa13a4de0649278974f111b103fb31f0d83cf56051f2ebc19";

fn asset(tag: &str, name: &str) -> String {
    format!(
        r#"{{"name":"{name}","browser_download_url":"https://github.com/kennypassenier/latch-rs/releases/download/{tag}/{name}"}}"#
    )
}

/// The newest release uploaded but not signed yet (the window between the
/// CI upload and Kenny's signature), then the signed one before it.
fn releases_json() -> String {
    format!(
        r#"[
  {{"tag_name":"v2.7.0","draft":false,"prerelease":false,"assets":[{},{}]}},
  {{"tag_name":"v2.6.0","draft":false,"prerelease":false,"assets":[{},{},{}]}}
]"#,
        asset("v2.7.0", LATCH_ASSET),
        asset("v2.7.0", "SHA256SUMS"),
        asset("v2.6.0", LATCH_ASSET),
        asset("v2.6.0", "SHA256SUMS"),
        asset("v2.6.0", "SHA256SUMS.minisig"),
    )
}

const PROBE: &str = "test -x /usr/local/bin/latch";

/// A container without latch, and GitHub answering with the real release.
fn missing_latch(exec: &MockExecutor) {
    exec.respond_always(PROBE, CmdOutput::failed(3, ""));
    exec.respond_always(
        "api.github.com/repos/kennypassenier/latch-rs/releases",
        CmdOutput::ok(&releases_json()),
    );
    // The signature first: its URL also contains ".../SHA256SUMS".
    exec.respond_always("/v2.6.0/SHA256SUMS.minisig", CmdOutput::ok(SIG));
    exec.respond_always("/v2.6.0/SHA256SUMS", CmdOutput::ok(SUMS));
    exec.respond_always(
        "-o '/var/lib/homelab/staged/latch",
        CmdOutput::ok(&format!("{LINUX_SHA}\n")),
    );
    exec.respond_always("GLIBC_2", CmdOutput::ok("need=2.39 have=2.41\n"));
}

fn index_of(calls: &[String], needle: &str) -> usize {
    calls
        .iter()
        .position(|c| c.contains(needle))
        .unwrap_or_else(|| panic!("no call with {needle:?} in {calls:#?}"))
}

#[test]
fn dashboard_latch_the_constants_name_the_signed_release() {
    assert_eq!(LATCH_REPO, "kennypassenier/latch-rs");
    assert_eq!(LATCH_ASSET, "latch-x86_64-unknown-linux-gnu");
    assert_eq!(LATCH_BIN, "/usr/local/bin/latch");
    // The fixture is what GitHub serves, signed with the ecosystem key.
    homelab_core::release_sig::verify_sums(SUMS, SIG).unwrap();
}

#[test]
fn dashboard_latch_an_unsigned_newest_release_yields_to_the_newest_signed_one() {
    let r = newest_signed_release(&releases_json(), LATCH_ASSET).unwrap();
    assert_eq!(r.tag, "v2.6.0");
    assert!(r
        .asset_url
        .ends_with("/v2.6.0/latch-x86_64-unknown-linux-gnu"));
    assert!(r.sig_url.unwrap().ends_with("/v2.6.0/SHA256SUMS.minisig"));
}

#[test]
fn dashboard_latch_drafts_prereleases_and_unsigned_only_lists_are_refused() {
    let drafts = format!(
        r#"[{{"tag_name":"v2.8.0","draft":true,"prerelease":false,"assets":[{},{},{}]}},
            {{"tag_name":"v2.8.0-rc1","draft":false,"prerelease":true,"assets":[{},{},{}]}},
            {{"tag_name":"v2.7.0","draft":false,"prerelease":false,"assets":[{},{}]}}]"#,
        asset("v2.8.0", LATCH_ASSET),
        asset("v2.8.0", "SHA256SUMS"),
        asset("v2.8.0", "SHA256SUMS.minisig"),
        asset("v2.8.0-rc1", LATCH_ASSET),
        asset("v2.8.0-rc1", "SHA256SUMS"),
        asset("v2.8.0-rc1", "SHA256SUMS.minisig"),
        asset("v2.7.0", LATCH_ASSET),
        asset("v2.7.0", "SHA256SUMS"),
    );
    let why = newest_signed_release(&drafts, LATCH_ASSET).unwrap_err();
    assert!(why.contains("no signed release"), "{why}");
    let why =
        newest_signed_release(r#"{"message":"API rate limit exceeded"}"#, LATCH_ASSET).unwrap_err();
    assert!(why.contains("rate limit"), "{why}");
}

#[tokio::test]
async fn dashboard_latch_present_latch_is_left_alone_and_nothing_is_downloaded() {
    let exec = MockExecutor::new();
    // Default mock: `test -x` succeeds, so latch is there.
    let sink = VecSink::new();
    let installed = ensure_latch(&exec, &sink, 118).await.unwrap();
    assert!(!installed);
    assert_eq!(exec.calls_containing("api.github.com").len(), 0);
    assert_eq!(exec.calls_containing("curl").len(), 0);
    assert_eq!(exec.calls_containing(LATCH_BIN).len(), 1, "only the probe");
    // git is what latch drives; it is ensured like every other package.
    assert_eq!(exec.calls_containing("command -v git").len(), 1);
}

#[tokio::test]
async fn dashboard_latch_missing_latch_is_verified_before_it_is_installed() {
    let exec = MockExecutor::new();
    missing_latch(&exec);
    let sink = VecSink::new();
    let installed = ensure_latch(&exec, &sink, 120).await.unwrap();
    assert!(installed);
    let calls = exec.calls();
    let sig = index_of(&calls, "/v2.6.0/SHA256SUMS.minisig");
    let download = index_of(&calls, "-o '/var/lib/homelab/staged/latch");
    let push = index_of(&calls, "pct push 120");
    let glibc = index_of(&calls, "GLIBC_2");
    let swap = index_of(&calls, "mv -f");
    assert!(sig < download, "signature checked before the download");
    assert!(download < push, "checksum checked before the push");
    assert!(push < glibc && glibc < swap, "{calls:#?}");
    let push_call = &calls[push];
    assert!(push_call.contains("--perms 0755"), "{push_call}");
    // Pushed beside the real path, never over it: a latch that cannot run
    // on this container must not replace one that can.
    assert!(
        push_call.contains("/usr/local/bin/latch.homelab-new"),
        "{push_call}"
    );
    assert!(calls[swap].contains(LATCH_BIN), "{}", calls[swap]);
    // The host's staging copy goes after the push.
    let cleanup = index_of(&calls, "rm -f /var/lib/homelab/staged/latch/latch-120");
    assert!(push < cleanup, "{calls:#?}");
    assert!(sink.lines().iter().any(|l| l.contains("latch v2.6.0")));
}

#[tokio::test]
async fn dashboard_latch_a_bad_signature_fails_the_step_and_installs_nothing() {
    let exec = MockExecutor::new();
    exec.respond_first(
        "/v2.6.0/SHA256SUMS",
        CmdOutput::ok(&SUMS.replace("6c65", "7c65")),
    );
    exec.respond_first("/v2.6.0/SHA256SUMS.minisig", CmdOutput::ok(SIG));
    missing_latch(&exec);
    let sink = VecSink::new();
    let e = ensure_latch(&exec, &sink, 120).await.unwrap_err();
    assert!(format!("{e}").contains("ecosystem signature"), "{e}");
    assert_eq!(
        exec.calls_containing("-o '/var/lib/homelab/staged").len(),
        0
    );
    assert_eq!(exec.ran("pct", &["push"]), 0);
}

#[tokio::test]
async fn dashboard_latch_an_unsigned_release_list_fails_the_step() {
    let exec = MockExecutor::new();
    exec.respond_first(
        "api.github.com/repos/kennypassenier/latch-rs/releases",
        CmdOutput::ok(&format!(
            r#"[{{"tag_name":"v2.7.0","draft":false,"prerelease":false,"assets":[{},{}]}}]"#,
            asset("v2.7.0", LATCH_ASSET),
            asset("v2.7.0", "SHA256SUMS")
        )),
    );
    missing_latch(&exec);
    let sink = VecSink::new();
    let e = ensure_latch(&exec, &sink, 120).await.unwrap_err();
    assert!(format!("{e}").contains("no signed release"), "{e}");
    assert_eq!(exec.ran("pct", &["push"]), 0);
}

#[tokio::test]
async fn dashboard_latch_a_download_that_does_not_match_the_signed_list_is_refused() {
    let exec = MockExecutor::new();
    exec.respond_first(
        "-o '/var/lib/homelab/staged/latch",
        CmdOutput::ok(&"0".repeat(64)),
    );
    missing_latch(&exec);
    let sink = VecSink::new();
    let e = ensure_latch(&exec, &sink, 120).await.unwrap_err();
    assert!(format!("{e}").contains("CHECKSUM MISMATCH"), "{e}");
    assert_eq!(exec.ran("pct", &["push"]), 0);
    assert!(!exec
        .calls_containing("rm -f /var/lib/homelab/staged/latch/latch-120")
        .is_empty());
}

#[tokio::test]
async fn dashboard_latch_a_container_whose_glibc_is_too_old_keeps_no_latch() {
    let exec = MockExecutor::new();
    exec.respond_first("GLIBC_2", CmdOutput::ok("need=2.39 have=2.36\n"));
    missing_latch(&exec);
    let sink = VecSink::new();
    let e = ensure_latch(&exec, &sink, 120).await.unwrap_err();
    assert!(format!("{e}").contains("glibc 2.39"), "{e}");
    assert_eq!(exec.calls_containing("mv -f").len(), 0);
    assert!(!exec
        .calls_containing("rm -f /usr/local/bin/latch.homelab-new")
        .is_empty());
}

#[tokio::test]
async fn dashboard_latch_a_probe_that_cannot_answer_fails_rather_than_downloads() {
    let exec = MockExecutor::new();
    exec.respond_always(PROBE, CmdOutput::failed(255, "container not running"));
    let sink = VecSink::new();
    let e = ensure_latch(&exec, &sink, 120).await.unwrap_err();
    assert!(format!("{e}").contains("latch"), "{e}");
    assert_eq!(exec.calls_containing("curl").len(), 0);
}

#[tokio::test]
async fn dashboard_latch_the_deploy_guard_installs_latch() {
    let exec = MockExecutor::new();
    missing_latch(&exec);
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 120, false, None)
        .await
        .unwrap();
    assert_eq!(
        exec.calls_containing("pct push 120")
            .iter()
            .filter(|c| c.contains("latch"))
            .count(),
        1
    );
    // A failed verification fails the guard, as a failed apt install does.
    let exec = MockExecutor::new();
    exec.respond_first(
        "-o '/var/lib/homelab/staged/latch",
        CmdOutput::ok(&"0".repeat(64)),
    );
    missing_latch(&exec);
    let r = homelab_core::ops::guards::apply(&exec, &sink, 120, false, None).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn dashboard_latch_the_golden_template_bakes_latch_through_the_guard() {
    use homelab_core::ops::template::{build_template, TemplateCfg};
    use homelab_core::ops::OpCtx;
    use homelab_core::runner::NullJournal;
    let exec = MockExecutor::new();
    exec.respond_always("pct config 999", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    missing_latch(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let ctx = OpCtx {
        exec: &exec,
        sink: &sink,
        journal: &journal,
        safety: homelab_core::safety::SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    };
    let report = build_template(&ctx, &TemplateCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let swap = index_of(&calls, "mv -f /usr/local/bin/latch.homelab-new");
    assert!(calls[swap].contains("pct exec 999"), "{}", calls[swap]);
    // Baked before the template is generalized and frozen.
    assert!(swap < index_of(&calls, "pct template 999"));
    // template-cleanup (Kenny, 2026-09-29): the fleet ships logs with Alloy
    // since 2026-09-02; the template no longer pulls promtail.
    assert!(
        calls.iter().all(|c| !c.contains("promtail")),
        "the template still pulls promtail"
    );
}
