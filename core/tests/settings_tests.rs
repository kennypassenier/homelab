//! app-settings-enforced-1 (Kenny, 2026-10-23: "zie dat fixes permanent zijn
//! en dat het admin dashboard dit kan enforcen"): an app's wanted settings
//! are put back in its own files, one key, nothing else touched, the app
//! stopped only when the entry says so and only when a write happens.

use homelab_core::checks::{AppSetting, ServiceChecks, SettingFormat};
use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::settings::{edit_text, enforce_app, register, wanted};
use homelab_core::sink::{Level, PipelineEvent, VecSink};
use homelab_core::state::HostState;

const CONF: &str = "/appdata/downloader/qbittorrent-config/qBittorrent/qBittorrent.conf";
const OLD: &str = "*.exe, *.bat, *.cmd, *.scr, *.lnk";
const NEW: &str = "*.exe, *.bat, *.cmd, *.scr, *.lnk, *.7z, *.iso, *.zipx, *.vbs, *.ps1, *.msi";

fn qbt(stop_app: bool) -> AppSetting {
    AppSetting {
        file: CONF.into(),
        format: SettingFormat::Ini,
        section: Some("BitTorrent".into()),
        key: r"Session\ExcludedFileNames".into(),
        value: NEW.into(),
        stop_app,
    }
}

fn conf(excluded: &str) -> String {
    format!(
        "[Application]\nFileLogger\\Enabled=true\n\n[BitTorrent]\nSession\\DefaultSavePath=/downloads\nSession\\ExcludedFileNames={excluded}\nSession\\Port=6881\n\n[Preferences]\nWebUI\\APIKey=secret\n"
    )
}

const RUNNING: &str = r#"{"Service":"qbittorrent","State":"running"}"#;

/// The only content staged on the host for a push: what would be written.
fn staged(m: &MockExecutor) -> Vec<String> {
    m.file_paths()
        .into_iter()
        .filter(|p| p.contains("push-staging-"))
        .filter_map(|p| m.file(&p))
        .collect()
}

#[test]
fn ini_key_is_replaced_in_place_and_every_other_line_is_untouched() {
    let before = conf(OLD);
    let after = edit_text(&before, &qbt(true)).expect("differs");
    assert_eq!(after, conf(NEW), "only the one line changed, byte for byte");
}

#[test]
fn equal_value_gives_no_edit() {
    assert_eq!(edit_text(&conf(NEW), &qbt(true)), None);
}

#[test]
fn crlf_endings_and_missing_final_newline_are_kept() {
    let before = "[BitTorrent]\r\nSession\\ExcludedFileNames=old\r\nSession\\Port=1";
    let after = edit_text(before, &qbt(false)).unwrap();
    assert_eq!(
        after,
        format!("[BitTorrent]\r\nSession\\ExcludedFileNames={NEW}\r\nSession\\Port=1")
    );
}

#[test]
fn a_missing_key_is_added_at_the_end_of_its_section() {
    let before = "[BitTorrent]\nSession\\Port=1\n\n[Preferences]\nA=b\n";
    let after = edit_text(before, &qbt(false)).unwrap();
    assert_eq!(
        after,
        format!(
            "[BitTorrent]\nSession\\Port=1\nSession\\ExcludedFileNames={NEW}\n\n[Preferences]\nA=b\n"
        )
    );
}

#[test]
fn a_missing_section_is_appended_and_the_same_key_elsewhere_is_ignored() {
    let before = "[Other]\nSession\\ExcludedFileNames=untouched\n";
    let after = edit_text(before, &qbt(false)).unwrap();
    assert_eq!(
        after,
        format!(
            "[Other]\nSession\\ExcludedFileNames=untouched\n\n[BitTorrent]\nSession\\ExcludedFileNames={NEW}\n"
        )
    );
}

#[test]
fn kv_replaces_or_appends_a_plain_line() {
    let s = AppSetting {
        file: "/appdata/x/.env".into(),
        format: SettingFormat::Kv,
        section: None,
        key: "LIMIT".into(),
        value: "5".into(),
        stop_app: false,
    };
    assert_eq!(
        edit_text("A=1\nLIMIT=3\n# LIMIT=9\nB=2\n", &s).unwrap(),
        "A=1\nLIMIT=5\n# LIMIT=9\nB=2\n"
    );
    assert_eq!(edit_text("A=1\nLIMIT=5\n", &s), None);
    assert_eq!(edit_text("A=1", &s).unwrap(), "A=1\nLIMIT=5\n");
}

#[test]
fn problems_name_an_unusable_entry() {
    let mut s = qbt(true);
    assert!(s.problems().is_empty());
    s.section = None;
    assert!(s.problems().iter().any(|p| p.contains("needs a section")));
    let mut k = qbt(true);
    k.format = SettingFormat::Kv;
    assert!(k.problems().iter().any(|p| p.contains("takes no section")));
    let mut f = qbt(true);
    f.file = "relative/../x".into();
    assert!(!f.problems().is_empty());
}

#[tokio::test]
async fn equal_value_writes_nothing_and_stops_nothing() {
    let m = MockExecutor::new();
    m.respond_always("[ -f", CmdOutput::ok(&conf(NEW)));
    let sink = VecSink::new();
    let r = enforce_app(&m, &sink, 105, "downloader", "qbittorrent", &[qbt(true)])
        .await
        .unwrap();
    assert!(r.put_back.is_empty());
    assert_eq!(r.matched, 1);
    assert_eq!(m.calls_containing("docker compose stop").len(), 0);
    assert_eq!(m.calls_containing("docker compose start").len(), 0);
    assert_eq!(m.ran("pct", &["push"]), 0, "nothing written");
    assert!(
        !sink.events().iter().any(|e| matches!(
            e,
            PipelineEvent::Line {
                level: Level::Warn | Level::Error | Level::Info,
                ..
            }
        )),
        "a match is a debug line at most"
    );
}

#[tokio::test]
async fn a_differing_value_stops_writes_one_key_starts_and_says_so() {
    let m = MockExecutor::new();
    m.respond_always("docker compose ps", CmdOutput::ok(RUNNING));
    // First read while running, second after the stop (the app rewrote its
    // file on exit; the old value is still there).
    m.enqueue("[ -f", CmdOutput::ok(&conf(OLD)));
    m.enqueue("[ -f", CmdOutput::ok(&conf(OLD)));
    let sink = VecSink::new();
    let r = enforce_app(&m, &sink, 105, "downloader", "qbittorrent", &[qbt(true)])
        .await
        .unwrap();
    assert_eq!(r.put_back, vec![r"Session\ExcludedFileNames".to_string()]);
    let calls = m.calls();
    let at = |needle: &str| calls.iter().position(|c| c.contains(needle));
    let stop = at("docker compose stop").expect("stopped");
    let push = at("pct push").expect("written");
    let start = at("docker compose start").expect("started");
    assert!(
        stop < push && push < start,
        "stop, write, start: {:?}",
        calls
    );
    assert_eq!(staged(&m), vec![conf(NEW)], "only that key changed");
    let warn: Vec<String> = sink
        .events()
        .into_iter()
        .filter_map(|e| match e {
            PipelineEvent::Line {
                level: Level::Warn,
                msg,
                ..
            } => Some(msg),
            _ => None,
        })
        .collect();
    assert_eq!(warn.len(), 1);
    assert!(warn[0].contains(r"put back Session\ExcludedFileNames in qbittorrent"));
    assert!(!warn[0].contains("*.7z"), "values never reach the notice");
}

#[tokio::test]
async fn the_app_is_stopped_only_when_the_entry_requires_it() {
    let m = MockExecutor::new();
    m.respond_always("docker compose ps", CmdOutput::ok(RUNNING));
    m.respond_always("[ -f", CmdOutput::ok(&conf(OLD)));
    let sink = VecSink::new();
    let r = enforce_app(&m, &sink, 105, "downloader", "qbittorrent", &[qbt(false)])
        .await
        .unwrap();
    assert_eq!(r.put_back.len(), 1);
    assert_eq!(m.calls_containing("docker compose stop").len(), 0);
    assert_eq!(m.calls_containing("docker compose start").len(), 0);
    assert_eq!(staged(&m), vec![conf(NEW)]);
}

#[tokio::test]
async fn an_app_that_was_not_running_is_written_but_not_started() {
    let m = MockExecutor::new();
    // compose answers with no running service.
    m.respond_always("docker compose ps", CmdOutput::ok(""));
    m.respond_always("[ -f", CmdOutput::ok(&conf(OLD)));
    let sink = VecSink::new();
    enforce_app(&m, &sink, 105, "downloader", "qbittorrent", &[qbt(true)])
        .await
        .unwrap();
    assert_eq!(m.calls_containing("docker compose stop").len(), 0);
    assert_eq!(m.calls_containing("docker compose start").len(), 0);
    assert_eq!(staged(&m), vec![conf(NEW)]);
}

#[tokio::test]
async fn a_file_that_is_not_there_yet_is_reported_not_created() {
    let m = MockExecutor::new();
    m.respond_always("[ -f", CmdOutput::failed(3, ""));
    let sink = VecSink::new();
    let r = enforce_app(&m, &sink, 105, "downloader", "qbittorrent", &[qbt(true)])
        .await
        .unwrap();
    assert_eq!(r.missing, vec![CONF.to_string()]);
    assert_eq!(m.ran("pct", &["push"]), 0);
    assert_eq!(m.calls_containing("docker compose stop").len(), 0);
}

#[test]
fn a_deploy_registers_the_settings_and_a_dropped_entry_leaves_the_state() {
    let mut state = HostState::default();
    let mut checks = std::collections::BTreeMap::new();
    checks.insert(
        "qbittorrent".to_string(),
        ServiceChecks {
            settings: vec![qbt(true)],
            ..Default::default()
        },
    );
    register(&mut state, "downloader", &checks);
    assert_eq!(wanted(&state, "downloader", "qbittorrent"), vec![qbt(true)]);
    register(&mut state, "other", &Default::default());
    assert_eq!(wanted(&state, "downloader", "qbittorrent").len(), 1);
    register(&mut state, "downloader", &Default::default());
    assert!(wanted(&state, "downloader", "qbittorrent").is_empty());
}

/// The entry in the real stack file parses and is usable.
#[test]
fn qbittorrents_stack_file_declares_its_exclusion_list() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks/downloader/qbittorrent/checks.yml"),
    )
    .unwrap();
    let sc: ServiceChecks = serde_yaml::from_str(&text).unwrap();
    assert_eq!(sc.settings.len(), 1);
    let s = &sc.settings[0];
    assert!(s.problems().is_empty(), "{:?}", s.problems());
    assert!(s.stop_app, "qBittorrent rewrites its config on exit");
    assert_eq!(s.key, r"Session\ExcludedFileNames");
    assert_eq!(s.value, NEW);
}
