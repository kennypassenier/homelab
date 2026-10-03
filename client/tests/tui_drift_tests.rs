#![cfg(feature = "tui")]
//! fix-69 · the drift badge may not run programs on the TUI's thread.
//!
//! These tests put a fake `gh` first on PATH that records every call, so
//! they live in a file of their own: PATH is process-wide, and the other
//! suites must keep the real one.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use homelab_client::tui::backend::BackendEvent;
use homelab_client::tui::model::{Model, Msg, Screen, update};
use homelab_proto::{FleetState, HostView, ServerMsg, StackView};

struct Fixture {
    /// A copy of the repository's almanac stack: one native service with a
    /// release repository, so building its deploy spec asks `gh`.
    stack: PathBuf,
    /// Every `gh` call appends a line here.
    gh_log: PathBuf,
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

fn fixture() -> &'static Fixture {
    static F: OnceLock<Fixture> = OnceLock::new();
    F.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("homelab-fix69-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh_log = root.join("gh-calls");
        let gh = bin.join("gh");
        std::fs::write(
            &gh,
            format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 1\n", gh_log.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        // SAFETY: only reached through OnceLock::get_or_init, which runs
        // this closure on exactly one thread even under concurrent callers;
        // no other thread touches the environment while it runs.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", bin.display(), path))
        };
        let stack = root.join("almanac");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/almanac"),
            &stack,
        );
        Fixture { stack, gh_log }
    })
}

fn gh_calls() -> usize {
    std::fs::read_to_string(&fixture().gh_log)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn fleet(applied_hash: &str) -> ServerMsg {
    ServerMsg::State(Box::new(FleetState {
        status_measured_at: None,
        host: HostView {
            home_address: None,
            name: "pve-01".into(),
            cpu_pct: None,
            ram_pct: 0,
            disk_pct: 0,
            tls_fingerprint: String::new(),
            ram_total_mb: 0,
            ram_used_mb: 0,
            ram_committed_mb: 0,
            cores_total: 1,
            load1_x100: 0,
            disk_detail: None,
            uptime_s: None,
            release: None,
            guests_usage: None,
        },
        stacks: vec![StackView {
            applied_source: None,
            component_digests: Default::default(),
            native: false,
            usage: None,
            name: "almanac".into(),
            vmid: 112,
            hostname: "112-app-almanac".into(),
            apps: vec![],
            drift: false,
            applied_hash: applied_hash.into(),
            env_sealed: true,
            env_sealed_read: Some(true),
            online: true,
            enabled: true,
        }],
    }))
}

fn model() -> Model {
    let mut m = Model::new();
    m.screen = Screen::Main;
    m.local_stacks = vec![("almanac".into(), fixture().stack.clone())];
    m
}

/// covers: fix-69
///
/// Every fleet state (at start, on `r`, after a deploy) recomputed the drift
/// badge by building each local stack's full deploy spec inside `update()`:
/// `latch cat` per latch app and a `gh` release download per native service,
/// kyu alone about 71 MiB, while the screen could not redraw, and their
/// progress lines printed over the raw-mode screen
/// (tui-refresh-blocks-on-downloads, 2026-09-27).
#[test]
fn fix_69_a_fleet_state_runs_no_program_inside_update() {
    let mut m = model();
    let before = gh_calls();
    update(
        &mut m,
        Msg::Backend(BackendEvent::Server(fleet("0011223344556677"))),
    );
    assert_eq!(
        gh_calls(),
        before,
        "update() ran gh to draw a badge; the screen froze for the download"
    );
    assert_eq!(
        m.local_hash_requested,
        vec![("almanac".to_string(), fixture().stack.clone())],
        "the hash is asked for, to be computed off the UI thread"
    );
}

/// covers: fix-69
///
/// The hash the badge needs leaves the programs out, as the host's own hash
/// does, so computing it fetches none: the same hash `build_spec` gives,
/// without a single `gh` call.
#[test]
fn fix_69_the_badge_s_hash_fetches_no_release_and_matches_the_deploy_s() {
    let before = gh_calls();
    let (hash, _notes) = homelab_client::spec::local_intent_hash(&fixture().stack).unwrap();
    assert_eq!(gh_calls(), before, "computing the badge's hash asked gh");
    let spec = homelab_client::spec::build_spec(&fixture().stack).unwrap();
    assert!(
        gh_calls() > before,
        "the fixture must be one a deploy asks gh for"
    );
    assert_eq!(hash, homelab_core::manifest::intent_hash(&spec));
}

/// covers: fix-69
///
/// The answer folds into the badge, and what computing it had to say lands in
/// the model's log instead of on top of the screen.
#[test]
fn fix_69_the_computed_hash_sets_the_badge_and_its_lines_go_to_the_log() {
    let mut m = model();
    update(
        &mut m,
        Msg::Backend(BackendEvent::Server(fleet("0011223344556677"))),
    );
    update(
        &mut m,
        Msg::LocalHash {
            stack: "almanac".into(),
            hash: Ok("8899aabbccddeeff".into()),
            notes: vec!["[env] almanac <- latch".into()],
        },
    );
    assert!(m.fleet.as_ref().unwrap().stacks[0].drift);
    assert!(
        m.logs
            .iter()
            .any(|l| l.source == "LOCAL" && l.msg == "[env] almanac <- latch")
    );
    update(
        &mut m,
        Msg::LocalHash {
            stack: "almanac".into(),
            hash: Ok("0011223344556677".into()),
            notes: vec![],
        },
    );
    assert!(!m.fleet.as_ref().unwrap().stacks[0].drift);
}
