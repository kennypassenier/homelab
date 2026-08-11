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
            ram_total_mb: 31744,
            ram_committed_mb: 21504,
            cores_total: 12,
            cores_committed: 11,
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
fn deploy_focus_window_renders_feed() {
    use homelab_client::tui::model::{Focus, LogRow};
    use homelab_proto::LogLevel;
    let mut m = ready_model();
    m.focus = Some(Focus {
        title: "DEPLOY syncthing :: vmid 110".into(),
        feed: vec![
            LogRow {
                level: LogLevel::Info,
                source: "HOST".into(),
                msg: "[sync][run ] provision container".into(),
            },
            LogRow {
                level: LogLevel::Debug,
                source: "HOST".into(),
                msg: "  pct create 110 …".into(),
            },
            LogRow {
                level: LogLevel::Info,
                source: "HOST".into(),
                msg: "[gate] syncthing :: running".into(),
            },
        ],
        scroll: 0,
        done: false,
        ok: false,
        result: String::new(),
    });
    let out = render(&m);
    assert!(out.contains("FOCUS :: DEPLOY syncthing"));
    assert!(out.contains("provision container"));
    assert!(out.contains("EXECUTING"));
}

#[test]
fn plan_modal_previews_changes() {
    use homelab_client::tui::model::Plan;
    use homelab_proto::{BootSpec, DeploySpec, LxcSpec, NetworkSpec, ResourceSpec, StackManifest};
    let mut m = ready_model();
    let manifest = StackManifest {
        stack_name: "syncthing".into(),
        vmid: 110,
        hostname: "110-app-syncthing".into(),
        network: NetworkSpec {
            ip: "10.10.10.10/24".into(),
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
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![],
        apps: vec!["syncthing".into()],
    };
    m.plan = Some(Plan {
        stack: "syncthing".into(),
        lines: vec![
            (' ', "plan:".into()),
            (
                '~',
                "  UPDATE   110-app-syncthing (already provisioned)".into(),
            ),
            (' ', "  ✓ no-touch list protects 100-107,111,201-203".into()),
        ],
        spec: Box::new(DeploySpec {
            manifest,
            files: vec![],
            env: Default::default(),
            gateway_route: None,
        }),
    });
    let out = render(&m);
    assert!(out.contains("CHANGE_PLAN :: syncthing"));
    assert!(out.contains("UPDATE"));
    assert!(out.contains("execute deploy"));
}

#[test]
fn wizard_renders_preset_step() {
    use homelab_client::tui::model::{WizStep, Wizard};
    let mut m = ready_model();
    m.wizard = Some(Wizard {
        step: WizStep::Preset,
        preset_idx: 0,
        name: String::new(),
    });
    let out = render(&m);
    assert!(out.contains("STACK_FORGE :: STEP 1/3"));
    assert!(out.contains("syncthing"));
    assert!(out.contains("jellyfin"));
}

#[test]
fn scaffold_writes_a_deployable_stack() {
    use homelab_client::scaffold::scaffold_stack;
    let tmp = std::env::temp_dir().join(format!("homelab-scaffold-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let s = scaffold_stack(
        &tmp,
        "demo",
        120,
        512,
        Some(("syncthing", "syncthing/syncthing:latest")),
    )
    .expect("scaffold");
    // Manifest + app compose + promtail compose + promtail config.
    assert!(s.files.iter().any(|f| f.ends_with("lxc-compose.yml")));
    assert!(s
        .files
        .iter()
        .any(|f| f.ends_with("syncthing/docker-compose.yml")));
    assert!(s
        .files
        .iter()
        .any(|f| f.ends_with("promtail/docker-compose.yml")));
    // The scaffolded manifest passes the same validator the host uses (D10).
    let spec = homelab_client::spec::build_spec(&tmp.join("demo")).expect("build spec");
    homelab_core::manifest::validate(&spec).expect("scaffolded stack must be valid");
    assert_eq!(spec.manifest.hostname, "120-app-demo");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn palette_fuzzy_matches() {
    let m = palette_matches("doct");
    assert!(!m.is_empty());
    // "run doctor" and "go: doctor" both contain "doct".
    assert!(m.len() >= 2);
}
