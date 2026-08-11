//! TUI snapshot tests (AR9/G1): render each screen from a fixed model against
//! ratatui's TestBackend with effects off, and assert on stable structure.
//! These run with no terminal and no network.

use homelab_client::tui::model::{azerty_tab_index, palette_matches, Model, Msg, Screen, Tab};
use homelab_client::tui::view;
use homelab_proto::{AppView, FleetState, HostView, StackView};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn fleet() -> FleetState {
    FleetState {
        host: HostView {
            name: "pve-01".into(),
            cpu_pct: 18,
            ram_pct: 66,
            disk_pct: 42,
            tls_fingerprint: "9F:2A:C4:1E:AB:CD".into(),
        },
        stacks: vec![
            StackView {
                name: "syncthing".into(),
                vmid: 110,
                hostname: "110-app-syncthing".into(),
                apps: vec![AppView {
                    name: "syncthing".into(),
                    running: true,
                    restarts: 0,
                }],
                drift: false,
                env_sealed: true,
                online: true,
            },
            StackView {
                name: "media".into(),
                vmid: 106,
                hostname: "106-app-media".into(),
                apps: vec![AppView {
                    name: "jellyfin".into(),
                    running: false,
                    restarts: 3,
                }],
                drift: true,
                env_sealed: false,
                online: true,
            },
        ],
    }
}

fn ready_model() -> Model {
    let mut m = Model::new();
    m.screen = Screen::Main;
    m.fx = homelab_client::tui::fx::FxLevel::Off; // deterministic snapshots
    m.fleet = Some(fleet());
    m.conn = homelab_client::tui::model::Conn::Up;
    m
}

fn render(model: &Model) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| view::draw(f, model)).unwrap();
    let buf = terminal.backend().buffer().clone();
    buf.content().iter().map(|c| c.symbol()).collect::<String>()
}

#[test]
fn dashboard_shows_host_and_fleet() {
    let m = ready_model();
    let out = render(&m);
    assert!(out.contains("HOST_MESH"), "no host panel");
    assert!(out.contains("LXC_MESH"), "no fleet panel");
    assert!(out.contains("110-app-syncthing"));
    assert!(out.contains("106-app-media"));
    assert!(out.contains("DATA_TRANSFERS"));
}

#[test]
fn stacks_tab_shows_detail_and_flags() {
    let mut m = ready_model();
    m.tab = Tab::Stacks;
    m.selected_stack = 1; // media: drifted, no env
    let out = render(&m);
    assert!(out.contains("STACK_REGISTRY"));
    assert!(out.contains("MANIFEST :: 106-app-media"));
    assert!(out.contains("APP_GRID"));
    assert!(out.contains("fails closed")); // no-env warning rendered
}

#[test]
fn logs_tab_has_source_selector() {
    let mut m = ready_model();
    m.tab = Tab::Logs;
    let out = render(&m);
    assert!(out.contains("LOG_STREAM"));
    assert!(out.contains("ALL"));
    assert!(out.contains("SYNCTHING"));
}

#[test]
fn doctor_tab_renders_checks() {
    let mut m = ready_model();
    m.tab = Tab::Doctor;
    m.doctor_text = vec![
        "doctor: Warn".into(),
        "  [Ok] host disk — 42% free".into(),
        "  [Fail] stack media env — no sealed .env".into(),
        "        ↳ provide media's .env".into(),
    ];
    let out = render(&m);
    assert!(out.contains("SELF_DIAGNOSIS"));
    assert!(out.contains("host disk"));
    assert!(out.contains("no sealed"));
}

#[test]
fn too_small_terminal_warns() {
    let m = ready_model();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal.draw(|f| view::draw(f, &m)).unwrap();
    let out: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(out.contains("TERMINAL TOO SMALL"));
}

// ── AZERTY key handling (Kenny uses a Belgian AZERTY keyboard) ──────────────

#[test]
fn azerty_symbols_map_to_tabs() {
    // Unshifted AZERTY number row: & é " '
    assert_eq!(azerty_tab_index('&'), Some(0));
    assert_eq!(azerty_tab_index('é'), Some(1));
    assert_eq!(azerty_tab_index('"'), Some(2));
    assert_eq!(azerty_tab_index('\''), Some(3));
    // Plain digits still work too.
    assert_eq!(azerty_tab_index('1'), Some(0));
    assert_eq!(azerty_tab_index('4'), Some(3));
    assert_eq!(azerty_tab_index('x'), None);
}

#[test]
fn azerty_e_acute_switches_to_stacks_tab() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut m = ready_model();
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('é'), KeyModifiers::NONE)),
    );
    assert_eq!(m.tab.index(), 1);
}

#[test]
fn palette_fuzzy_matches() {
    let m = palette_matches("doct");
    assert!(!m.is_empty());
    // "run doctor" and "go: doctor" both contain "doct".
    assert!(m.len() >= 2);
}
