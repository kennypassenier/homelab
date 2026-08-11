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
            ram_used_mb: 12680,
            ram_committed_mb: 38400,
            cores_total: 12,
            load1_x100: 285,
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
                applied_hash: String::new(),
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
                applied_hash: String::new(),
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
            protection: false,
            gpu: false,
            vpn: false,
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
    use homelab_client::tui::model::{ResField, WizStep, Wizard};
    let mut m = ready_model();
    m.wizard = Some(Wizard {
        step: WizStep::Preset,
        preset_idx: 0,
        name: String::new(),
        ram: 512,
        cores: 2,
        disk: 8,
        swap: 512,
        swap_touched: false,
        vmid: 108,
        res_field: ResField::Ram,
        disk_typing: false,
    });
    let out = render(&m);
    assert!(out.contains("STACK_FORGE :: STEP 1/4"));
    assert!(out.contains("syncthing"));
    assert!(out.contains("jellyfin"));
}

#[test]
fn wizard_resources_step_shows_all_fields() {
    use homelab_client::tui::model::{ResField, WizStep, Wizard};
    let mut m = ready_model();
    m.wizard = Some(Wizard {
        step: WizStep::Resources,
        preset_idx: 0,
        name: "test".into(),
        ram: 2048,
        cores: 4,
        disk: 16,
        swap: 512,
        swap_touched: false,
        vmid: 108,
        res_field: ResField::Disk,
        disk_typing: false,
    });
    let out = render(&m);
    assert!(out.contains("STEP 3/4"));
    assert!(out.contains("RAM"));
    assert!(out.contains("CPU"));
    assert!(out.contains("DISK"));
    assert!(out.contains("VMID"));
    assert!(out.contains("SWAP")); // swap is its own editable field now
    assert!(out.contains("protection on")); // proxmox destroy-protection noted
}

#[test]
fn swap_formula_matches_legacy_tiers() {
    use homelab_client::scaffold::StackDefaults;
    let d = StackDefaults::default();
    // clamp(RAM/4, 512, 2048): container-appropriate, matches production.
    assert_eq!(d.swap_for(512), 512); // floor
    assert_eq!(d.swap_for(1024), 512);
    assert_eq!(d.swap_for(2048), 512);
    assert_eq!(d.swap_for(5120), 1280);
    assert_eq!(d.swap_for(8192), 2048); // ceiling
    assert_eq!(d.swap_for(16384), 2048); // never the old 4096
}

#[test]
fn scaffold_has_no_watchtower_and_manual_update_policy() {
    use homelab_client::scaffold::{scaffold_stack, synthetic_presets, StackParams};
    let tmp = std::env::temp_dir().join(format!("homelab-nowatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // Synthetic preset (no presets dir) -> generic compose generation path.
    let presets = synthetic_presets();
    let synth = presets.iter().find(|p| p.name == "syncthing").unwrap();
    scaffold_stack(
        &tmp,
        &tmp.join("no-presets-here"),
        &StackParams {
            name: "x",
            vmid: 121,
            ram_mb: 512,
            cores: 2,
            disk_gb: 8,
            swap_mb: None,
            preset: Some(synth),
        },
    )
    .unwrap();
    let compose = std::fs::read_to_string(tmp.join("x/syncthing/docker-compose.yml")).unwrap();
    assert!(
        !compose.contains("watchtower"),
        "watchtower must be gone (D9)"
    );
    assert!(compose.contains("com.homelab.update.policy=manual"));
    let manifest = std::fs::read_to_string(tmp.join("x/lxc-compose.yml")).unwrap();
    assert!(!manifest.contains("watchtower"));
    assert!(manifest.contains("swap_mb: 512")); // 1:1 for 512
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn scaffold_writes_a_deployable_stack() {
    use homelab_client::scaffold::scaffold_stack;
    let tmp = std::env::temp_dir().join(format!("homelab-scaffold-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // Real data-driven path: load the repo's presets/ directory.
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets
        .iter()
        .find(|p| p.name == "syncthing")
        .expect("syncthing preset on disk");
    assert!(
        syncthing.dir.is_some(),
        "must load from disk, not synthetic"
    );
    let s = scaffold_stack(
        &tmp,
        &presets_dir,
        &homelab_client::scaffold::StackParams {
            name: "demo",
            vmid: 120,
            ram_mb: 512,
            cores: 2,
            disk_gb: 8,
            swap_mb: None,
            preset: Some(syncthing),
        },
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

#[test]
fn settings_tab_renders_config_and_edits() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use homelab_proto::{Command, HostConfigView, ServerMsg};

    let mut m = ready_model();
    // Switching to SETTINGS requests the config from the host.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE)),
    );
    assert!(matches!(m.tab, Tab::Settings));
    assert!(m.outbox.iter().any(|c| matches!(c, Command::GetConfig)));

    // Config arrives → rendered with all fields + self-contained explanations.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Backend(homelab_client::tui::backend::BackendEvent::Server(
            ServerMsg::Config(Box::new(HostConfigView {
                backup_hour: Some(4),
                notify_webhook: None,
                retention: homelab_core::retention::default_tiers(),
            })),
        )),
    );
    let out = render(&m);
    assert!(out.contains("HOST_SETTINGS"));
    assert!(out.contains("04:00"));
    assert!(out.contains("every"), "tier rows visible");
    assert!(out.contains("forever"), "unbounded tier visible");
    assert!(out.contains("in sync with host"));

    // LEFT on the hour row edits the value and marks dirty; SHIFT+S saves.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
    );
    assert!(m.settings_dirty);
    let out = render(&m);
    assert!(out.contains("03:00"));
    assert!(out.contains("unsaved changes"));
    m.outbox.clear();
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT)),
    );
    assert!(m.outbox.iter().any(|c| matches!(c, Command::SetConfig(_))));
    assert!(!m.settings_dirty);
}

#[test]
fn settings_azerty_fifth_tab_key() {
    assert_eq!(azerty_tab_index('('), Some(4));
    assert_eq!(azerty_tab_index('5'), Some(4));
}

#[test]
fn preset_templates_substitute_and_apps_list_matches_dirs() {
    // The scaffolded stack from a disk preset must: substitute __STACK__/
    // __HOSTNAME__ everywhere, list the APP dir names in the manifest (a
    // stack named differently from its app must still start the right
    // /opt/<stack>/<app> dirs), and inject the _core promtail from disk.
    use homelab_client::scaffold::{scaffold_stack, scan_presets, StackParams};
    let tmp = std::env::temp_dir().join(format!("homelab-presetsub-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    // Stack name deliberately different from the app name.
    scaffold_stack(
        &tmp,
        &presets_dir,
        &StackParams {
            name: "vault-sync",
            vmid: 130,
            ram_mb: 512,
            cores: 1,
            disk_gb: 4,
            swap_mb: None,
            preset: Some(syncthing),
        },
    )
    .unwrap();
    let manifest = std::fs::read_to_string(tmp.join("vault-sync/lxc-compose.yml")).unwrap();
    // Apps list uses APP dir names, not the stack name (the old latent bug).
    assert!(manifest.contains("- syncthing"));
    assert!(manifest.contains("- promtail"));
    assert!(!manifest.contains("- vault-sync"));
    let compose =
        std::fs::read_to_string(tmp.join("vault-sync/syncthing/docker-compose.yml")).unwrap();
    assert!(compose.contains("vault-sync_net"), "network substituted");
    assert!(compose.contains("hostname: 130-app-vault-sync"));
    assert!(!compose.contains("__STACK__"), "no leftover placeholders");
    let ptcfg =
        std::fs::read_to_string(tmp.join("vault-sync/promtail/promtail-config.yml")).unwrap();
    assert!(ptcfg.contains("stack: vault-sync"));
    assert!(ptcfg.contains("host: 130-app-vault-sync"));
    // __path__ is a real promtail key, NOT a placeholder — must survive.
    assert!(ptcfg.contains("__path__"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn wizard_preset_step_lists_disk_presets() {
    let mut m = ready_model();
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    m.presets = homelab_client::scaffold::scan_presets(&presets_dir);
    assert!(m.presets.len() >= 6);
    assert_eq!(
        m.presets.last().unwrap().name,
        "custom",
        "custom sorts last"
    );
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
    );
    let out = render(&m);
    assert!(out.contains("syncthing"));
    assert!(out.contains("jellyfin"));
    assert!(out.contains("Media server"));
}

#[test]
fn manifest_storage_is_derived_from_compose_appdata_binds() {
    // Single source of truth: the /appdata/ bind in the preset's compose
    // must appear as a manifest storage entry (host dir created + chowned +
    // LXC-mounted at deploy). Without this, scaffolded stacks would write
    // config to the container rootfs — unbacked-up and lost on destroy.
    use homelab_client::scaffold::{scaffold_stack, scan_presets, StackParams};
    let tmp = std::env::temp_dir().join(format!("homelab-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    scaffold_stack(
        &tmp,
        &presets_dir,
        &StackParams {
            name: "vault",
            vmid: 131,
            ram_mb: 512,
            cores: 1,
            disk_gb: 4,
            swap_mb: None,
            preset: Some(syncthing),
        },
    )
    .unwrap();
    let manifest = std::fs::read_to_string(tmp.join("vault/lxc-compose.yml")).unwrap();
    assert!(manifest.contains("storage:"), "storage section generated");
    assert!(manifest.contains("host_path: /appdata/vault/syncthing-config"));
    assert!(manifest.contains("host_owner_uid: 101000"));
    // And the whole thing still validates as a deployable spec.
    let spec = homelab_client::spec::build_spec(&tmp.join("vault")).expect("build spec");
    homelab_core::manifest::validate(&spec).expect("valid");
    assert_eq!(spec.manifest.storage.len(), 1);
    assert_eq!(
        spec.manifest.storage[0].mount_point,
        "/appdata/vault/syncthing-config"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn b4_drift_flag_computed_from_applied_hash() {
    use homelab_client::tui::backend::BackendEvent;
    use homelab_proto::ServerMsg;
    // Local stack dir whose intent hash we know.
    let tmp = std::env::temp_dir().join(format!("homelab-drift-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    homelab_client::scaffold::scaffold_stack(
        &tmp,
        &presets_dir,
        &homelab_client::scaffold::StackParams {
            name: "driftcase",
            vmid: 140,
            ram_mb: 512,
            cores: 1,
            disk_gb: 4,
            swap_mb: None,
            preset: Some(syncthing),
        },
    )
    .unwrap();
    let spec = homelab_client::spec::build_spec(&tmp.join("driftcase")).unwrap();
    let real_hash = homelab_core::manifest::intent_hash(&spec);

    let mut m = ready_model();
    m.local_stacks = vec![("driftcase".into(), tmp.join("driftcase"))];
    let mk_fleet = |hash: &str| {
        ServerMsg::State(Box::new(FleetState {
            host: fleet().host,
            stacks: vec![StackView {
                name: "driftcase".into(),
                vmid: 140,
                hostname: "140-app-driftcase".into(),
                apps: vec![],
                drift: false,
                applied_hash: hash.to_string(),
                env_sealed: true,
                online: true,
            }],
        }))
    };
    // Applied hash matches the local dir → no drift.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Backend(BackendEvent::Server(mk_fleet(&real_hash))),
    );
    assert!(!m.fleet.as_ref().unwrap().stacks[0].drift);
    // Host applied something else → drift.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Backend(BackendEvent::Server(mk_fleet("deadbeef00112233"))),
    );
    assert!(m.fleet.as_ref().unwrap().stacks[0].drift);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn d11_bundle_round_trip_excludes_secrets_and_substitutes() {
    let tmp = std::env::temp_dir().join(format!("homelab-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let stacks = tmp.join("stacks");
    std::fs::create_dir_all(&stacks).unwrap();
    // Scaffold a source stack, then add a SECRET .env that must never travel.
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    homelab_client::scaffold::scaffold_stack(
        &stacks,
        &presets_dir,
        &homelab_client::scaffold::StackParams {
            name: "source",
            vmid: 150,
            ram_mb: 512,
            cores: 1,
            disk_gb: 4,
            swap_mb: None,
            preset: Some(syncthing),
        },
    )
    .unwrap();
    std::fs::write(
        stacks.join("source/syncthing/.env"),
        "API_KEY=supersecret123\n",
    )
    .unwrap();

    // Export → the bundle must not contain the secret.
    let bundle_path = tmp.join("source-bundle.yml");
    homelab_client::spec::export_bundle(&stacks.join("source"), bundle_path.to_str().unwrap())
        .unwrap();
    let bundle_raw = std::fs::read_to_string(&bundle_path).unwrap();
    assert!(
        !bundle_raw.contains("supersecret123"),
        "secrets never in a bundle"
    );
    assert!(bundle_raw.contains("bundle_version"));

    // Import as a different stack → identity fully substituted + valid.
    let dest = homelab_client::spec::import_bundle(&bundle_path, &stacks, "copy", 151).unwrap();
    let spec = homelab_client::spec::build_spec(&dest).unwrap();
    homelab_core::manifest::validate(&spec).unwrap();
    assert_eq!(spec.manifest.stack_name, "copy");
    assert_eq!(spec.manifest.vmid, 151);
    assert_eq!(spec.manifest.hostname, "151-app-copy");
    assert!(spec.manifest.network.ip.starts_with("10.10.10.51/"));
    assert_eq!(
        spec.manifest.storage[0].host_path,
        "/appdata/copy/syncthing-config"
    );
    let compose = std::fs::read_to_string(dest.join("syncthing/docker-compose.yml")).unwrap();
    assert!(compose.contains("copy_net"));
    assert!(!compose.contains("source_net"));
    assert!(spec.env.is_empty(), "no secrets came along");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn g4_shell_tab_sends_exec_and_shows_output() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use homelab_client::tui::backend::BackendEvent;
    use homelab_proto::{Command, RpcResponse, ServerMsg};

    let mut m = ready_model();
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('6'), KeyModifiers::NONE)),
    );
    assert!(matches!(m.tab, Tab::Shell));
    // Typing digits must NOT switch tabs while in the shell.
    for c in "uptime -p".chars() {
        homelab_client::tui::model::update(
            &mut m,
            Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
        );
    }
    assert!(matches!(m.tab, Tab::Shell));
    assert_eq!(m.shell_input, "uptime -p");
    m.outbox.clear();
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    );
    // ExecIn queued for the selected stack's vmid.
    assert!(m
        .outbox
        .iter()
        .any(|c| matches!(c, Command::ExecIn { vmid: 110, .. })));
    assert!(m.shell_waiting);
    // Host reply lands in the scrollback.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Backend(BackendEvent::Server(ServerMsg::RpcDone(RpcResponse {
            id: 9,
            ok: true,
            message: "exit 0\nup 4 hours".into(),
        }))),
    );
    assert!(!m.shell_waiting);
    let out = render(&m);
    assert!(out.contains("REMOTE_SHELL"));
    assert!(out.contains("up 4 hours"));
    assert!(
        out.contains("exec_enabled"),
        "self-contained explanation visible"
    );
}
