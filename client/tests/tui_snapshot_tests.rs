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
                enabled: true,
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
                enabled: true,
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
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
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
            checks: Default::default(),
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
        storage_paths: Vec::new(),
        storage_idx: 0,
        storage_no_data: Vec::new(),
        disk_typing: false,
    });
    let out = render(&m);
    assert!(out.contains("STACK_FORGE :: STEP 1/5"));
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
        storage_paths: Vec::new(),
        storage_idx: 0,
        storage_no_data: Vec::new(),
        disk_typing: false,
    });
    let out = render(&m);
    assert!(out.contains("STEP 3/5"));
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
    // Synthetic preset -> the generic compose generation path. The presets
    // directory itself is the real one: scaffolding without it used to fall
    // back to a hand-written promtail config, which is how a third shape of
    // that file came to exist. It now refuses instead, so a test that wants
    // the generic path must still give it somewhere real to read the core
    // apps from.
    let presets = synthetic_presets();
    let synth = presets.iter().find(|p| p.name == "syncthing").unwrap();
    let real_presets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    scaffold_stack(
        &tmp,
        &real_presets,
        &StackParams {
            name: "x",
            vmid: 121,
            ram_mb: 512,
            cores: 2,
            disk_gb: 8,
            swap_mb: None,
            no_data_paths: &[],
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
            no_data_paths: &[],
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
            no_data_paths: &[],
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
            no_data_paths: &[],
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
            no_data_paths: &[],
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
                enabled: true,
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
            no_data_paths: &[],
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

#[test]
fn v8_config_race_regression_rpc_exit_rule() {
    use homelab_client::rpc_can_exit;
    // The bug: exiting on RpcDone while the Config payload frame was still
    // in flight. The rule: a payload-carrying RPC may only exit after BOTH.
    assert!(
        !rpc_can_exit(true, false, true),
        "must wait for the payload"
    );
    assert!(rpc_can_exit(true, true, true));
    assert!(rpc_can_exit(false, false, true), "plain RPCs exit on done");
    assert!(!rpc_can_exit(false, false, false));
}

/// The document in the repository is the one somebody opens when the host is
/// gone — and until 2026-09-01 it had never been regenerated after the
/// generator was fixed. It still listed the gateway as "LEGACY (v1 manifest,
/// not deployable)", still said restic repos were per stack, and predated
/// five of the thirteen stacks. Every fix landed in the generator and none of
/// them in the artefact.
///
/// So the artefact is checked, not the generator. A stack file that changes
/// without the runbook being regenerated fails here, which is the only moment
/// anybody would notice before the worst day.
/// covers: F152
#[test]
fn the_committed_runbook_matches_a_fresh_generation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let out = std::env::temp_dir().join(format!("homelab-dr-{}.md", std::process::id()));
    homelab_client::spec::generate_runbook(&root.join("stacks"), out.to_str().unwrap()).unwrap();
    let fresh = std::fs::read_to_string(&out).unwrap();
    let committed = std::fs::read_to_string(root.join("docs/DR_RUNBOOK.md")).unwrap();
    let _ = std::fs::remove_file(&out);
    assert_eq!(
        fresh, committed,
        "docs/DR_RUNBOOK.md is stale — run `homelab runbook docs/DR_RUNBOOK.md` \
         and read the diff before committing it"
    );
}

/// Every host operation is either reachable from the TUI or listed here as
/// deliberately command-line-only.
///
/// Kenny's rule, 2026-09-01: "de TUI moet van alle features van dit project
/// gebruik kunnen maken." Measured that evening, the TUI could send nine of
/// the host's twenty-five operations; the other sixteen existed only on the
/// command line, which meant opening a terminal while already sitting in the
/// interface built for the job (F156).
///
/// A list would go stale the first time somebody added a command, so this is
/// a test instead: a new `Command` variant fails the suite until it is either
/// wired into the TUI or written down here with a reason. Neither answer is
/// wrong; leaving the question unanswered is.
const CLI_ONLY: &[(&str, &str)] = &[
    (
        "Ping",
        "the client's own connection probe, not an operation",
    ),
    (
        "Status",
        "one-line version print; the TUI shows it in the header already",
    ),
    (
        "PruneOrphans",
        "removes files after somebody has read the list the deploy printed — the \
         same reasoning as DestroyStack: deleting should mean leaving the \
         comfortable interface and typing the stack name out (Kenny, form H2b)",
    ),
    (
        "ListManualChecks",
        "G17: the answering happens at the keyboard right after a deploy, and the \
         asking happens in the nightly notification that already reaches Kenny — a \
         screen he has to go and find is the thing form I2 was against",
    ),
    (
        "AnswerManualCheck",
        "G17: same reason as ListManualChecks — answering is one line, and putting \
         it in the TUI would mean navigating to it to say 'yes the picture is fine'",
    ),
    (
        "DestroyStack",
        "deliberate friction: destroying a container should mean leaving the \
         comfortable interface and typing it out (Kenny's C2 gate)",
    ),
    (
        "BackupHostMeta",
        "runs nightly on its own; there is no moment where an operator wants it \
         by hand from a stack view",
    ),
    (
        "ApplyResources",
        "hot-apply of cores/RAM — a later round (form T1)",
    ),
    ("PatchFleet", "apt across the whole fleet — a later round"),
    ("ZfsReplicate", "runs in the nightly plan — a later round"),
    (
        "ListTemplates",
        "golden-template maintenance, a few times a year — a later round",
    ),
    (
        "BuildTemplate",
        "golden-template maintenance — a later round",
    ),
    (
        "ForgetStack",
        "housekeeping after a rename; rare and easy to do wrong — a later round",
    ),
];

/// covers: F156
#[test]
fn every_host_operation_is_reachable_or_deliberately_not() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let proto = std::fs::read_to_string(root.join("proto/src/lib.rs")).unwrap();
    // Brace-counted, not "the first \n}\n": half these variants are structs
    // with bodies of their own, and stopping at the first closing brace found
    // fifteen of twenty-eight — a parser that silently reads part of the file
    // is the same failure this test exists to catch.
    let body = {
        let i = proto.find("pub enum Command").expect("Command enum");
        let rest = &proto[i..];
        let open = rest.find('{').expect("enum body");
        let mut depth = 0i32;
        let mut end = rest.len();
        for (k, c) in rest.char_indices().skip(open) {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
                _ => {}
            }
        }
        &rest[open..end]
    };
    let variants: Vec<String> = body
        .lines()
        .filter_map(|l| {
            let t = l.strip_prefix("    ")?;
            let name: String = t.chars().take_while(|c| c.is_alphanumeric()).collect();
            // The delimiter may be a space away: `DestroyStack {` is a
            // variant, and looking only at the character immediately after the
            // name found fifteen of twenty-eight.
            let after = t[name.len()..].trim_start().chars().next()?;
            let starts_upper = name.chars().next()?.is_uppercase();
            (starts_upper && matches!(after, '{' | '(' | ',')).then_some(name)
        })
        .collect();
    assert!(
        variants.len() > 20,
        "the enum parser found only {} variants — the shape changed and this \
         check would pass on nothing",
        variants.len()
    );

    // Everything the TUI sends.
    let mut tui_src = String::new();
    fn walk(dir: &std::path::Path, out: &mut String) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push_str(&std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    walk(&root.join("client/src/tui"), &mut tui_src);

    let mut unreachable = Vec::new();
    for v in &variants {
        let sent = tui_src.contains(&format!("Command::{}", v));
        let excused = CLI_ONLY.iter().any(|(n, _)| n == v);
        if !sent && !excused {
            unreachable.push(v.clone());
        }
    }
    assert!(
        unreachable.is_empty(),
        "these host operations can only be reached from the command line and \
         are not written down as such — wire them into the TUI or add them to \
         CLI_ONLY with a reason: {:?}",
        unreachable
    );

    // And the excuse list may not rot: an entry for a command the TUI now
    // sends, or for one that no longer exists, is a comment pretending to be
    // a decision.
    let stale: Vec<&str> = CLI_ONLY
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| {
            !variants.iter().any(|v| v == n) || tui_src.contains(&format!("Command::{}", n))
        })
        .collect();
    assert!(
        stale.is_empty(),
        "CLI_ONLY still excuses commands that are gone or now reachable: {:?}",
        stale
    );
}

/// A stack can be created without the TUI, and the result is the wizard's.
///
/// F136: for twenty-one CLI commands there was no way to scaffold a stack —
/// only the interactive wizard could. So every stack made outside the TUI was
/// hand-written, which is how three stack files came to claim live vmids.
///
/// The test that matters is not that `homelab new` produces files, it is that
/// it produces the SAME files: one scaffolder, one preset catalogue, one set
/// of defaults. Two paths that drift apart would be worse than one path.
/// covers: F136
#[test]
fn a_stack_scaffolded_without_the_wizard_matches_the_wizard() {
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    let d = homelab_client::scaffold::StackDefaults::default();

    let mk = |dir: &std::path::Path, no_data: &[String]| {
        homelab_client::scaffold::scaffold_stack(
            dir,
            &presets_dir,
            &homelab_client::scaffold::StackParams {
                name: "twin",
                vmid: 151,
                ram_mb: syncthing.meta.ram_mb,
                cores: syncthing.meta.cores.unwrap_or(2),
                disk_gb: syncthing.meta.disk_gb.unwrap_or(8),
                swap_mb: Some(d.swap_for(syncthing.meta.ram_mb)),
                preset: Some(syncthing),
                no_data_paths: no_data,
            },
        )
        .unwrap()
    };

    let tmp = std::env::temp_dir().join(format!("homelab-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (a, b) = (tmp.join("cli"), tmp.join("wiz"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    let cli = mk(&a, &[]);
    let wiz = mk(&b, &[]);
    let names = |s: &homelab_client::scaffold::Scaffolded, root: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> = s
            .files
            .iter()
            .map(|f| {
                std::path::Path::new(f)
                    .strip_prefix(root)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&cli, &a),
        names(&wiz, &b),
        "the two paths must scaffold the same set of files"
    );
    for f in names(&cli, &a) {
        assert_eq!(
            std::fs::read_to_string(a.join(&f)).unwrap(),
            std::fs::read_to_string(b.join(&f)).unwrap(),
            "{} differs between the two paths",
            f
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The wizard can reach `no_data`, and its preview cannot drift from what it
/// scaffolds.
///
/// Kenny, form B4b: "de TUI moet van alle features van dit project gebruik
/// kunnen maken". The flag exists because an undeclared empty directory
/// stopped the gateway's whole backup; a flag only reachable by editing YAML
/// would let the wizard keep producing exactly that stack.
///
/// The second half is the part worth a test: the wizard has to ASK about the
/// paths before anything is written, so it previews them from the same
/// templates the scaffold copies. If those two ever disagree, the answer
/// lands on a path that does not exist and the flag silently does nothing.
/// covers: F154, F155
#[test]
fn the_wizard_can_declare_an_app_that_keeps_nothing() {
    let tmp = std::env::temp_dir().join(format!("homelab-nodata-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let stacks = tmp.join("stacks");
    std::fs::create_dir_all(&stacks).unwrap();
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();

    // What the wizard would show, before writing anything.
    let preview = homelab_client::scaffold::preview_appdata_paths(
        &presets_dir,
        Some(syncthing),
        "hollow",
        150,
    );
    assert!(
        !preview.is_empty(),
        "the wizard has nothing to ask about, so the step would be skipped"
    );

    let declared = vec![preview[0].clone()];
    let s = homelab_client::scaffold::scaffold_stack(
        &stacks,
        &presets_dir,
        &homelab_client::scaffold::StackParams {
            name: "hollow",
            vmid: 150,
            ram_mb: 512,
            cores: 1,
            disk_gb: 4,
            swap_mb: None,
            preset: Some(syncthing),
            no_data_paths: &declared,
        },
    )
    .unwrap();

    let manifest = std::fs::read_to_string(s.dir.join("lxc-compose.yml")).unwrap();
    let m: homelab_core::manifest::StackManifest = serde_yaml::from_str(&manifest).unwrap();

    // Preview and result must describe the same set of paths.
    let written: Vec<String> = m.storage.iter().map(|x| x.host_path.clone()).collect();
    assert_eq!(
        written, preview,
        "the wizard asked about one set of paths and the scaffold wrote another"
    );

    // And the answer reached the manifest, on that path and no other.
    for mount in &m.storage {
        assert_eq!(
            mount.no_data,
            declared.contains(&mount.host_path),
            "no_data landed on the wrong path: {}",
            mount.host_path
        );
    }
    // The declared one then has no restic repository at all.
    let owners: Vec<String> = homelab_core::ops::backup::owner_groups(&m)
        .into_iter()
        .map(|(o, _)| o)
        .collect();
    assert!(
        owners.len() < m.storage.len(),
        "a declared-empty app must drop out of the repository list: {:?}",
        owners
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// covers: F150, F151
#[test]
fn h17_runbook_generator_structural_snapshot() {
    // E7: the total-loss document must regenerate correctly from a fixture
    // stacks dir — a format change that garbles it should fail HERE, not on
    // the worst day.
    let tmp = std::env::temp_dir().join(format!("homelab-runbook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let stacks = tmp.join("stacks");
    std::fs::create_dir_all(&stacks).unwrap();
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = homelab_client::scaffold::scan_presets(&presets_dir);
    let syncthing = presets.iter().find(|p| p.name == "syncthing").unwrap();
    for (name, vmid) in [("alpha", 150u16), ("beta", 151u16)] {
        homelab_client::scaffold::scaffold_stack(
            &stacks,
            &presets_dir,
            &homelab_client::scaffold::StackParams {
                name,
                vmid,
                ram_mb: 512,
                cores: 1,
                disk_gb: 4,
                swap_mb: None,
                no_data_paths: &[],
                preset: Some(syncthing),
            },
        )
        .unwrap();
    }
    // One legacy (unparseable) stack dir must be listed, not crash the doc.
    std::fs::create_dir_all(stacks.join("old-v1")).unwrap();
    std::fs::write(stacks.join("old-v1/lxc-compose.yml"), "not: [valid v2").unwrap();

    // A native stack: no compose apps at all, systemd units instead. It is
    // the case the document got wrong for every one of them.
    std::fs::create_dir_all(stacks.join("gamma")).unwrap();
    std::fs::write(
        stacks.join("gamma/lxc-compose.yml"),
        concat!(
            "stack_name: gamma\n",
            "vmid: 152\n",
            "hostname: 152-app-gamma\n",
            "native_only: true\n",
            "network:\n  ip: 10.10.10.152/24\n  gateway: 10.10.10.1\n",
            "  bridge: vmbr0\n  vlan: 10\n",
            "resources:\n  cores: 1\n  memory_mb: 256\n  swap_mb: 128\n  disk_gb: 2\n",
            "lxc:\n  template: \"clone:998\"\n",
            "boot:\n  onboot: true\n",
            "apps: []\n",
        ),
    )
    .unwrap();

    let out = tmp.join("DR.md");
    let n = homelab_client::spec::generate_runbook(&stacks, out.to_str().unwrap()).unwrap();
    assert_eq!(n, 3);
    let doc = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "## Layer 0",
        "## Layer 1",
        "## Layer 3",
        "### alpha (vmid 150)",
        "### beta (vmid 151)",
        "hostname `150-app-alpha`",
        "LEGACY",
        "homelab-backups/<app>-config",
        "## Full-host rebuild order",
    ] {
        assert!(doc.contains(needle), "runbook lost section: {}", needle);
    }
    // The line somebody actually copies in a disaster. The prose above it
    // said per owning app for months while this said per stack, and the test
    // asserted the wrong half — which is how a fixed document stayed broken
    // in the one place it gets used.
    assert!(
        !doc.contains("homelab-backups/<stack>-config"),
        "the copy-pasteable export must not contradict the sentence above it"
    );
    // And a native stack must not be told to deploy compose apps it has none
    // of.
    assert!(
        doc.contains("`homelab adopt stacks/gamma`"),
        "a native stack needs its own recovery path"
    );
    assert!(
        !doc.contains("`homelab deploy stacks/gamma`"),
        "and must not be told to run the compose path"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn d6_plan_diff_skip_update_and_line_previews() {
    use homelab_client::tui::model::build_plan_lines;
    use homelab_proto::{DeploySpec, FileBlob};
    let mk = |path: &str, content: &str| FileBlob {
        path: path.into(),
        content: content.into(),
        mode: None,
    };
    let mut m = homelab_proto::StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: "test".into(),
        vmid: 108,
        hostname: "108-app-test".into(),
        network: homelab_proto::NetworkSpec {
            ip: "10.10.10.8/24".into(),
            gateway: "g".into(),
            bridge: "b".into(),
            vlan: None,
        },
        resources: homelab_proto::ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 0,
            disk_gb: 4,
            storage: "s".into(),
        },
        lxc: homelab_proto::LxcSpec {
            template: "t".into(),
            unprivileged: true,
            features: String::new(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: homelab_proto::BootSpec {
            onboot: true,
            order: None,
        },
        storage: vec![],
        apps: vec!["appa".into(), "appb".into()],
    };
    m.hostname = "108-app-test".into();
    let spec = DeploySpec {
        manifest: m,
        files: vec![
            mk(
                "appa/docker-compose.yml",
                "services:\n  appa:\n    image: x:2\n",
            ),
            mk(
                "appb/docker-compose.yml",
                "services:\n  appb:\n    image: y:1\n",
            ),
        ],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let applied = vec![
        mk(
            "appa/docker-compose.yml",
            "services:\n  appa:\n    image: x:1\n",
        ),
        mk(
            "appb/docker-compose.yml",
            "services:\n  appb:\n    image: y:1\n",
        ),
        mk("gone/docker-compose.yml", "services: {}\n"),
    ];
    let lines = build_plan_lines(&spec, Some(&applied));
    let text: Vec<String> = lines.iter().map(|(c, l)| format!("{}{}", c, l)).collect();
    let joined = text.join("\n");
    // Unchanged app → SKIP; changed app → UPDATE with the exact line diff.
    assert!(
        joined.contains("SKIP     test/appb (no changes)"),
        "{}",
        joined
    );
    assert!(joined.contains("UPDATE   test/appa"));
    assert!(joined.contains("+      + image: x:2"));
    assert!(joined.contains("-      - image: x:1"));
    // A file removed from intent shows as REMOVE.
    assert!(joined.contains("REMOVE   gone/docker-compose.yml"));
    // No-changes spec: everything SKIP, nothing +/~ per app.
    let all_same = build_plan_lines(&spec, Some(&spec.files));
    let s2 = all_same
        .iter()
        .map(|(c, l)| format!("{}{}", c, l))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(s2.contains("SKIP     test/appa"));
    assert!(s2.contains("SKIP     test/appb"));
    assert!(!s2.contains("UPDATE   test/appa"));
    // CREATE plan (no applied): apps are ADD.
    let create = build_plan_lines(&spec, None);
    let s3 = create
        .iter()
        .map(|(c, l)| format!("{}{}", c, l))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(s3.contains("CREATE"));
    assert!(s3.contains("ADD      test/appa"));
}

#[test]
fn h19_every_palette_action_reaches_a_real_handler() {
    // G3's promise: no orphaned actions. Every palette entry must have an id
    // the dispatcher knows.
    //
    // The arm list used to be copied into this test by hand, which is the
    // same two-lists-that-drift shape the whole of 2026-09-01 was spent
    // removing — and it drifted on the first change: six new actions were
    // wired into the dispatcher and the copy here still held eleven. So the
    // dispatcher source is read instead of mirrored.
    use homelab_client::tui::model::PALETTE;
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/model.rs"),
    )
    .unwrap();
    for action in PALETTE {
        assert!(
            src.contains(&format!("\"{}\" =>", action.id)),
            "palette action '{}' ({}) has no dispatcher arm",
            action.id,
            action.label
        );
    }
}

#[test]
fn b6_update_badge_and_u_key_flow() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut m = ready_model();
    m.host_version = "2.6.0".into();
    // No release known → no badge, U does nothing.
    let out = render(&m);
    assert!(!out.contains("HOST UPDATE"));
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
    );
    assert!(m.release_update_requested.is_none());
    // Newer release arrives via the side channel → badge shows.
    homelab_client::tui::model::update(&mut m, Msg::ReleaseTag(Some("v9.9.9".into())));
    let out = render(&m);
    assert!(out.contains("HOST UPDATE v9.9.9"), "badge must appear");
    // U opens the focus window and requests staging.
    homelab_client::tui::model::update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
    );
    assert_eq!(m.release_update_requested.as_deref(), Some("v9.9.9"));
    assert!(m.focus.is_some());
    let out = render(&m);
    assert!(out.contains("UPDATE HOST"), "focus window title");
    // Same-version release → no badge (never prompt for a sidegrade).
    let mut m2 = ready_model();
    m2.host_version = "2.6.0".into();
    homelab_client::tui::model::update(&mut m2, Msg::ReleaseTag(Some("v2.6.0".into())));
    assert!(m2.host_update_available().is_none());
}

// ── The version gate (2026-08-31) ───────────────────────────────────────────

/// A client newer than the host loses whatever the host does not know about.
/// Serde drops an unknown field without a word, so the deploy succeeds and
/// silently does less than it was asked: a host one release behind ignored
/// the `data_mounts` block, the downloader came up without its disks, and 73
/// torrents went to `missingFiles`.
#[test]
fn a_host_that_is_behind_is_recognised_as_behind() {
    use homelab_client::version::{mutates, older};
    use homelab_proto::Command;

    assert!(older("3.15.0", "3.16.0"), "one minor behind is behind");
    assert!(older("3.15.9", "3.16.0"));
    assert!(older("2.9.9", "3.0.0"));
    assert!(!older("3.16.0", "3.16.0"), "equal is not behind");
    assert!(!older("3.17.0", "3.16.0"), "ahead is not behind");
    assert!(
        older("v3.15.0", "3.16.0"),
        "a leading v must not confuse it"
    );
    assert!(
        !older("nonsense", "3.16.0"),
        "an unreadable version must not block work — that would be worse than the bug"
    );

    // Read-only commands stay usable against an older host, precisely so the
    // mismatch can be diagnosed.
    assert!(!mutates(&Command::Ping));
    assert!(!mutates(&Command::Status));
    assert!(!mutates(&Command::Doctor));
    assert!(mutates(&Command::ForgetStack { stack: "x".into() }));

    // The remedy must never be blocked by the guard that names it. The first
    // live run refused `homelab release-update` against the very host it was
    // meant to replace, and told the operator to run what it had refused.
    assert!(!mutates(&Command::SelfUpdateHost {
        binary_b64: String::new(),
    }));
}

/// The host binary grew 132 KB past the link's 16 MiB default between v3.19.0
/// and v3.20.0, and `release-update` answered "Connection reset by peer" —
/// which names the network. The ceiling had never been written down, so there
/// was nothing to read. Both sides now carry the same number, and the client
/// refuses a payload that cannot arrive instead of watching it fail.
/// covers: F122
#[test]
fn a_payload_the_link_cannot_carry_is_refused_with_a_reason() {
    use homelab_client::version::{too_large, MAX_WS_FRAME};
    assert!(
        too_large(MAX_WS_FRAME).is_none(),
        "exactly at the ceiling is fine"
    );
    let why = too_large(MAX_WS_FRAME + 1).expect("one byte over must be refused");
    assert!(
        why.contains("this is a limit, not a network fault"),
        "{}",
        why
    );
    assert!(
        why.contains("64 MiB"),
        "the ceiling must be in the message: {}",
        why
    );
    // The size that actually failed tonight now fits with room to spare.
    assert!(too_large(16_861_920).is_none());
}

/// The runbook must name the repositories restic actually writes to.
///
/// It derived them from the stack name and printed `media-config`; the media
/// stack's data lives in `jellyfin-config`, `sonarr-config`, `radarr-config`,
/// `prowlarr-config`, `bazarr-config` and `seerr-config`. That document is
/// read exactly once — when everything else is gone — and it would have said
/// the backups were not there.
#[test]
fn the_runbook_names_the_repositories_restic_actually_uses() {
    use homelab_core::manifest::*;
    let mut m = StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: "media".into(),
        vmid: 106,
        hostname: "106-app-media".into(),
        network: NetworkSpec {
            ip: "10.10.10.6/24".into(),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: ResourceSpec {
            cores: 4,
            memory_mb: 4096,
            swap_mb: 512,
            disk_gb: 80,
            storage: "local-lvm".into(),
        },
        lxc: LxcSpec {
            template: "clone:997".into(),
            unprivileged: false,
            features: "nesting=1,keyctl=1".into(),
            protection: false,
            gpu: true,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: Vec::new(),
        apps: vec!["jellyfin".into(), "sonarr".into()],
    };
    for app in ["jellyfin", "sonarr"] {
        m.storage.push(MountSpec {
            host_path: format!("/appdata/media/{}-config", app),
            mount_point: format!("/appdata/media/{}-config", app),
            no_data: false,
            no_backup: None,
            host_owner_uid: Some(1000),
            app: Some(app.into()),
        });
    }
    let repos = homelab_core::ops::backup::owner_groups(&m)
        .iter()
        .map(|(o, _)| o.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        repos,
        vec!["jellyfin".to_string(), "sonarr".to_string()],
        "one repository per owning app — this is what backup.rs writes to"
    );
    assert!(
        !repos.contains(&"media".to_string()),
        "there is no media-config repository, and the runbook must not name one"
    );
}

/// The template a new stack is scaffolded from must be one the fleet actually
/// uses.
///
/// It was not. The default said `clone:999` — the v1 golden image — while ten
/// of the eleven live stacks clone 998 and the two privileged ones clone 997,
/// both v3. Every stack created from scratch would have started two
/// generations behind on the runaway guards, the log caps and
/// unattended-upgrades that the golden build bakes in, and nothing would have
/// said so: the eleven that exist carry their template by hand, so the default
/// is exercised only by a stack nobody has made yet.
///
/// This test is the thing that notices when the next generation lands and the
/// default is left behind.
#[test]
fn the_scaffold_default_template_is_one_the_fleet_uses() {
    let stacks = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut in_use: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&stacks).unwrap().flatten() {
        let f = entry.path().join("lxc-compose.yml");
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("template:") {
                in_use.push(v.trim().trim_matches('"').to_string());
            }
        }
    }
    assert!(
        !in_use.is_empty(),
        "no stack declares a template — this test has stopped measuring anything"
    );
    let default = homelab_client::scaffold::StackDefaults::default().template;
    assert!(
        in_use.contains(&default),
        "the scaffold default is '{}' but the fleet uses {:?} — a new stack \
         would be built from a template nothing else trusts",
        default,
        {
            let mut u = in_use.clone();
            u.sort();
            u.dedup();
            u
        }
    );
}

/// T7: every preset that ships in this repository must scaffold into a stack
/// that is actually deployable — not just the two the other tests happen to
/// name. A preset is DATA, so adding one is a file edit that recompiles
/// nothing and therefore passes every existing test by default; this is the
/// only thing standing between "I dropped a directory in presets/" and a
/// wizard entry that produces a broken stack.
///
/// What it checks per preset: the manifest and every compose file parse as
/// YAML, no `__PLACEHOLDER__` survives substitution anywhere in the tree, and
/// the manifest's app list matches the directories on disk.
#[test]
fn every_shipped_preset_scaffolds_a_valid_stack() {
    use homelab_client::scaffold::{scaffold_stack, scan_presets, StackParams};
    let presets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets");
    let presets = scan_presets(&presets_dir);
    assert!(
        presets.len() >= 8,
        "loaded {} presets from disk — the catalog did not load",
        presets.len()
    );

    for (i, preset) in presets.iter().enumerate() {
        if preset.dir.is_none() {
            // `custom` has no app directories by design: it is the empty start.
            continue;
        }
        let tmp = std::env::temp_dir().join(format!(
            "homelab-catalog-{}-{}-{}",
            std::process::id(),
            i,
            preset.name
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let stack = format!("t-{}", preset.name);
        scaffold_stack(
            &tmp,
            &presets_dir,
            &StackParams {
                name: &stack,
                vmid: 140,
                ram_mb: preset.meta.ram_mb.max(256),
                cores: 1,
                disk_gb: 8,
                swap_mb: None,
                no_data_paths: &[],
                preset: Some(preset),
            },
        )
        .unwrap_or_else(|e| panic!("preset '{}' does not scaffold: {}", preset.name, e));

        let manifest_path = tmp.join(&stack).join("lxc-compose.yml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("preset '{}': no manifest: {}", preset.name, e));
        serde_yaml::from_str::<serde_yaml::Value>(&manifest)
            .unwrap_or_else(|e| panic!("preset '{}': manifest is not YAML: {}", preset.name, e));

        // Every app directory the preset carries must appear in the manifest,
        // and every app in the manifest must exist on disk. A mismatch means
        // the deploy starts a directory that is not there, or leaves one
        // behind that never starts.
        for app in &preset.apps {
            assert!(
                manifest.contains(&format!("- {}", app)),
                "preset '{}': app '{}' is on disk but not in the manifest",
                preset.name,
                app
            );
        }

        // Walk the whole scaffolded tree: no placeholder may survive, and
        // every compose file must parse.
        let mut compose_files = 0;
        let mut stack_dir = vec![tmp.join(&stack)];
        while let Some(dir) = stack_dir.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack_dir.push(p);
                    continue;
                }
                let Ok(body) = std::fs::read_to_string(&p) else {
                    continue;
                };
                // `__path__` is a real promtail key, not a placeholder.
                let leftovers: Vec<&str> = body
                    .split_whitespace()
                    .filter(|w| {
                        w.starts_with("__") && w.ends_with("__") && *w != "__path__" && w.len() > 4
                    })
                    .collect();
                assert!(
                    leftovers.is_empty(),
                    "preset '{}': {:?} still holds placeholders {:?}",
                    preset.name,
                    p.file_name().unwrap(),
                    leftovers
                );
                if p.file_name().unwrap() == "docker-compose.yml" {
                    compose_files += 1;
                    serde_yaml::from_str::<serde_yaml::Value>(&body).unwrap_or_else(|e| {
                        panic!("preset '{}': {:?} is not YAML: {}", preset.name, p, e)
                    });
                }
            }
        }
        assert!(
            compose_files >= 1,
            "preset '{}' scaffolded no compose file at all",
            preset.name
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

/// T7: the Recyclarr configuration carries two absences that are DECISIONS,
/// and both of them look like something a helpful hand would fill in.
///
///   * No `quality_definition` block — the size caps (150 MB/min for movies,
///     100 MB/min for episodes, 250 MB/min for 2160p) are already set and
///     measured in both applications. Recyclarr can only scale the whole
///     TRaSH size table by one ratio, so letting it near this replaces three
///     measured numbers with one guess.
///   * No raw `trash_id` hashes. An earlier draft of that file had four of
///     them written from memory rather than looked up. A wrong hash scores
///     the wrong thing silently; a block on the wrong release group surfaces
///     months later as "why do I never get this show".
///
/// Neither absence is visible by reading the file, which is why it is a test.
#[test]
fn the_recyclarr_preset_keeps_its_two_deliberate_absences() {
    let cfg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../presets/recyclarr/recyclarr/recyclarr.yml");
    let body = std::fs::read_to_string(&cfg).expect("recyclarr preset config");

    let live: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !live.contains("quality_definition"),
        "recyclarr.yml sets quality_definition — that overwrites the measured \
         size caps R2/R3/R11 with a single scaling ratio"
    );

    // A TRaSH id is 32 lowercase hex characters. Outside a comment, one can
    // only have come from memory: the resolved ones are pasted at deploy time
    // from `recyclarr list custom-formats`.
    for line in live.lines() {
        let squashed: String = line.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let mut run = 0usize;
        for c in squashed.chars() {
            if c.is_ascii_hexdigit() && !c.is_ascii_uppercase() {
                run += 1;
                assert!(
                    run < 32,
                    "recyclarr.yml carries a raw trash_id on an active line — \
                     resolve it with `recyclarr list custom-formats` instead: {}",
                    line.trim()
                );
            } else {
                run = 0;
            }
        }
    }
}

/// T49: a stack file and the directories beside it have to agree.
///
/// Both halves of the disagreement are silent. An app in `apps:` with no
/// directory makes the deploy start a `/opt/<stack>/<app>` that holds
/// nothing; a directory missing from `apps:` is never started at all and
/// looks, from every side, exactly like a service that is simply down. The
/// seeder added to the uptime stack is one edit away from either.
///
/// The relative bind mounts are checked for the same reason: `./seed.py`
/// resolves at `docker compose up` time on the container, so a file that
/// never travelled shows up as a container restart loop hours later, not as
/// a failed deploy.
///
/// covers: F167
#[test]
fn every_stack_manifest_agrees_with_the_directories_beside_it() {
    let stacks = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut checked = 0;
    for entry in std::fs::read_dir(&stacks).unwrap().flatten() {
        let dir = entry.path();
        let manifest_path = dir.join("lxc-compose.yml");
        if !manifest_path.is_file() {
            continue;
        }
        let stack = dir.file_name().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(&manifest_path).unwrap();
        let manifest: serde_yaml::Value = serde_yaml::from_str(&raw)
            .unwrap_or_else(|e| panic!("stack '{}': manifest is not YAML: {}", stack, e));

        let declared: Vec<String> = manifest["apps"]
            .as_sequence()
            .map(|s| {
                s.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        // A native-service stack has no app directories by design.
        if declared.is_empty() {
            continue;
        }
        checked += 1;

        // Directories that hold nothing are not app directories. An empty one
        // is what git leaves behind when an app is dropped — it cannot track
        // an empty directory, so `stacks/productivity/vikunja/` survived the
        // commit that removed Vikunja and existed only in the working tree.
        // Failing on that would make this test disagree with itself between a
        // fresh clone and a working copy.
        let on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter(|e| {
                std::fs::read_dir(e.path())
                    .map(|mut rd| rd.next().is_some())
                    .unwrap_or(false)
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        for app in &declared {
            assert!(
                on_disk.contains(app),
                "stack '{}' declares app '{}' with no directory beside it — \
                 the deploy would start an empty /opt/{}/{}",
                stack,
                app,
                stack,
                app
            );
            let compose = dir.join(app).join("docker-compose.yml");
            assert!(
                compose.is_file(),
                "stack '{}': app '{}' has no docker-compose.yml",
                stack,
                app
            );
            // Every `./file` bind must exist, or it arrives as a directory
            // on the container and the service restart-loops.
            let body = std::fs::read_to_string(&compose).unwrap();
            for line in body.lines() {
                let t = line.trim();
                let Some(rest) = t.strip_prefix("- ./") else {
                    continue;
                };
                let Some((rel, _)) = rest.split_once(':') else {
                    continue;
                };
                // `./data` and friends are created by the deploy; only files
                // the repository is supposed to carry are checked.
                if !rel.contains('.') {
                    continue;
                }
                assert!(
                    dir.join(app).join(rel).exists(),
                    "stack '{}': {}/docker-compose.yml binds './{}' but that \
                     file is not in the repository — it would land on the \
                     container as an empty directory",
                    stack,
                    app,
                    rel
                );
            }
        }
        // A stack may run compose apps and native systemd units side by side
        // (the validator allows a mount owned by either), and this test used
        // to know only about `apps` — so a mixed stack failed here with a
        // message about an app that was never meant to be one.
        let natives: Vec<String> = manifest
            .get("natives")
            .and_then(|v| v.as_sequence())
            .map(|s| {
                s.iter()
                    .filter_map(|x| x.as_str().map(|t| t.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        for found in &on_disk {
            assert!(
                declared.contains(found) || natives.contains(found),
                "stack '{}' has a directory '{}' that is in neither its apps nor its \
                 natives list — it is never started, which is indistinguishable from \
                 being down",
                stack,
                found
            );
        }
    }
    assert!(checked >= 10, "only {} stacks checked", checked);
}

// ── T71: the native-service family, reachable from the TUI ─────────────────

/// The two shapes a native stack takes on this fleet are both read.
///
/// A stack with one service keeps its `service.yml` at the top
/// (`stacks/almanac/`); a stack with several gives each unit its own
/// directory (`stacks/kyu/kyu-runner/`). Reading only one shape would make
/// CT 109's three services look like one, and the two that vanished would be
/// the two nobody backs up.
#[test]
fn every_native_service_in_the_repository_is_found_with_its_unit_file() {
    let stacks = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");

    let almanac = homelab_client::spec::native_services(&stacks.join("almanac"));
    assert_eq!(almanac.len(), 1, "almanac has one service: {:?}", almanac);
    assert_eq!(almanac[0].0.unit, "almanac");

    let kyu = homelab_client::spec::native_services(&stacks.join("kyu"));
    let units: Vec<&str> = kyu.iter().map(|(m, _)| m.unit.as_str()).collect();
    assert_eq!(
        units,
        vec!["http-switchboard", "kyu", "kyu-runner"],
        "CT 109 holds three services and all three must be found"
    );

    // The unit file is what makes the service exist; a rebuild without it
    // produces a container with the program and nothing to run it.
    for (m, unit_file) in kyu.iter().chain(almanac.iter()) {
        let body = unit_file
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no .service file in the repository", m.unit));
        assert!(
            body.contains(&m.binary),
            "{}'s unit must exec the binary its service.yml declares",
            m.unit
        );
    }

    // Every native service must be installable from a verified release.
    // Measured 2026-09-02: three of four could, and the hub could not (F168)
    // — which mattered because it is the one the other two on CT 109 talk
    // to. The kyu project published v2.1.0 with `kyu` + `SHA256SUMS` the same
    // night, so the answer is now all four. Asserted as a count so that a
    // service silently LOSING its release source fails here rather than at a
    // rebuild, when it is too late to notice.
    let all: Vec<_> = kyu.iter().chain(almanac.iter()).collect();
    let without: Vec<&str> = all
        .iter()
        .filter(|(m, _)| m.release_repo.is_none())
        .map(|(m, _)| m.unit.as_str())
        .collect();
    assert!(
        without.is_empty(),
        "these native services cannot be installed from a release: {:?}",
        without
    );
}

/// A native stack must not be sent a compose operation.
///
/// The same key means the same intent everywhere in this interface — back
/// this stack up — but a native stack has no compose to act on, and
/// `BackupStack` on one would walk `/appdata` mounts that hold none of its
/// state. Before T71 the TUI simply had no native path at all: four of the
/// five C7 verbs were command-line only, each excused separately as "a later
/// round", which is the same round deferred three times.
#[test]
fn a_native_stack_gets_the_native_operation_from_the_same_key() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use homelab_client::tui::model::{update, Msg};
    use homelab_proto::Command;

    let mut m = ready_model();
    m.tab = homelab_client::tui::model::Tab::Stacks;
    // The repository's own kyu stack: native_only, three services.
    // Absolute: the test process runs in client/, and a relative stack path
    // would resolve to nothing and queue nothing, which passes for the wrong
    // reason.
    let kyu_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/kyu");
    m.local_stacks = vec![("kyu".into(), kyu_dir.clone())];
    let fleet = m.fleet.as_mut().unwrap();
    fleet.stacks.clear();
    fleet.stacks.push(StackView {
        name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        apps: Vec::new(),
        drift: false,
        applied_hash: String::new(),
        env_sealed: true,
        online: true,
        enabled: true,
    });
    m.selected_stack = 0;

    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT)),
    );
    assert!(
        matches!(m.outbox.first(), Some(Command::BackupNative { stack }) if stack == "kyu"),
        "a native stack must get BackupNative, not BackupStack: {:?}",
        m.outbox.first()
    );
    m.outbox.clear();
    m.focus = None;
    // Esc: an operation window stays open until it is closed, and while it
    // is open the keys belong to it.
    m.focus = None;

    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::SHIFT)),
    );
    assert!(
        matches!(m.outbox.first(), Some(Command::UpdateNative { stack }) if stack == "kyu"),
        "and UpdateNative, not UpdateStack: {:?}",
        m.outbox.first()
    );
    m.outbox.clear();
    m.focus = None;

    // A: adopt every service the stack declares — one command each, because
    // adoption verifies one unit against one service.yml.
    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT)),
    );
    assert_eq!(
        m.outbox.len(),
        3,
        "CT 109's three services each need their own adoption: {:?}",
        m.outbox
    );
    assert!(m
        .outbox
        .iter()
        .all(|c| matches!(c, Command::AdoptService(_))));
    m.outbox.clear();
    m.focus = None;

    // I: the download cannot happen on the event loop, so the key only
    // records the request. All three of CT 109's services are asked for now
    // — the hub published a verifiable release on 2026-09-02 (F168), and
    // before that it was skipped here for want of one.
    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT)),
    );
    assert!(
        m.outbox.is_empty(),
        "no command may be queued before the binary is fetched and verified"
    );
    let requested: Vec<&str> = m
        .native_install_requested
        .iter()
        .map(|(s, _)| s.unit.as_str())
        .collect();
    assert_eq!(
        requested,
        vec!["http-switchboard", "kyu", "kyu-runner"],
        "every service that declares a release source is requested; one that \
         does not is skipped rather than attempted"
    );
}

/// T69: a waiting step reaches the operator where they are already looking,
/// and both keys say what they do.
///
/// Kenny's form H1: "twee knoppen die het ofwel toelaten ofwel stoppen".
/// The case that raised it: a service check reporting a DELIBERATE drop —
/// routes 29 → 28 after a route was removed on purpose — where the honest
/// answer is allow rather than a failed deploy and an incident nobody
/// needed.
#[test]
fn a_waiting_step_is_answerable_from_the_window_the_operator_is_reading() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use homelab_client::tui::model::{update, Msg, PendingAsk};
    use homelab_proto::Command;

    let mut m = ready_model();
    m.focus = Some(homelab_client::tui::model::Focus {
        title: "DEPLOY gateway :: vmid 104".into(),
        feed: Vec::new(),
        scroll: 0,
        done: false,
        ok: false,
        result: String::new(),
    });
    m.pending_ask = Some(PendingAsk {
        id: 7,
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "routes went 29 → 28".into(),
        if_allowed: "de uitrol gaat door en legt het nieuwe aantal vast".into(),
        if_stopped: "de uitrol faalt en bundelt een incident".into(),
    });

    // It is drawn where the operator is already reading — over the feed, not
    // somewhere else on the screen.
    let out = render(&m);
    assert!(out.contains("service checks"), "the step must be named");
    assert!(out.contains("routes went 29"), "and what happened");
    assert!(
        out.contains("toelaten") && out.contains("stoppen"),
        "both keys visible: {}",
        out
    );
    assert!(
        out.contains("de uitrol gaat door"),
        "each key says what it DOES, not just its label (D82)"
    );
    assert!(
        out.contains("onbeheerd"),
        "and that not answering is its own outcome"
    );

    // `a` allows and sends the answer with the question's own id.
    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(
        matches!(
            m.outbox.first(),
            Some(Command::Answer { id: 7, allow: true })
        ),
        "{:?}",
        m.outbox.first()
    );
    assert!(m.pending_ask.is_none(), "the question stops being shown");

    // `s` stops.
    m.outbox.clear();
    m.pending_ask = Some(PendingAsk {
        id: 8,
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "routes went 29 → 28".into(),
        if_allowed: "door".into(),
        if_stopped: "stop".into(),
    });
    update(
        &mut m,
        Msg::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
    );
    assert!(
        matches!(
            m.outbox.first(),
            Some(Command::Answer {
                id: 8,
                allow: false
            })
        ),
        "{:?}",
        m.outbox.first()
    );
}

/// covers: F212
///
/// G20 of the Phase-7 gate. The CLI is a hand-rolled string match, and its
/// usage text is a separate list of `println!`s — two lists that nothing held
/// against each other. Eight verbs were in the first and not the second,
/// `install-native` and `release-update` among them: the command that
/// installs one of Kenny's own services, and the command that updates the
/// host. A verb nobody can discover is a verb that does not exist.
#[test]
fn every_cli_verb_appears_in_the_usage_text() {
    let src = include_str!("../src/main.rs");

    // The verbs the matcher answers to.
    let mut verbs: Vec<String> = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('"') {
            if let Some(name) = rest.split('"').next() {
                if t.contains("=>")
                    && !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
                {
                    verbs.push(name.to_string());
                }
            }
        }
    }
    verbs.sort();
    verbs.dedup();
    assert!(
        verbs.len() > 25,
        "the verb scan broke, not the help: found {:?}",
        verbs
    );

    // The usage block, which starts at the version line.
    let start = src
        .find("— usage:")
        .expect("the usage block moved; this test parses it");
    let usage = &src[start..];
    let usage = &usage[..usage.find("env: HOMELAB_HOST").unwrap_or(usage.len())];

    // `enable`/`disable` are documented as one line; accept either spelling.
    let missing: Vec<&String> = verbs
        .iter()
        .filter(|v| !usage.contains(&format!("homelab {}", v)) && !usage.contains(v.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these commands exist and are documented nowhere: {:?}",
        missing
    );
}

/// covers: F215
///
/// G18 of the Phase-7 gate. Sixteen of the fleet's forty-six apps carried no
/// `checks.yml` at all, and `shortcomings()` deliberately does not nag about
/// a service without checks — so their absence was silent by design.
///
/// Ten of the sixteen were promtail, which is the path every other
/// observation travels through. A deploy that silently breaks the log
/// shipper was verified green on every stack in the fleet.
#[test]
fn every_app_either_has_checks_or_is_named_as_deliberately_without() {
    // Apps that genuinely have nothing worth checking beyond "it is there".
    // Named rather than inferred, so adding one is a decision somebody makes
    // on purpose.
    const NO_CHECKS_BY_DECISION: &[&str] = &[
        // Sidecars whose only job is to render a file another app produced.
        "goaccess-report",
        // The pull-through cache mirrors: four containers of the same image,
        // and the registry stack's own checks already ask whether the cache
        // answers.
        "cache-dockerhub",
        "cache-ghcr",
        "cache-gcr",
        "cache-lscr",
    ];

    let mut missing = Vec::new();
    let Ok(stacks) = std::fs::read_dir("../stacks") else {
        panic!("the stacks tree moved");
    };
    let mut seen = 0;
    for stack in stacks.flatten() {
        if !stack.path().is_dir() {
            continue;
        }
        let Ok(apps) = std::fs::read_dir(stack.path()) else {
            continue;
        };
        for app in apps.flatten() {
            let p = app.path();
            if !p.is_dir() || !p.join("docker-compose.yml").exists() {
                continue;
            }
            seen += 1;
            let name = app.file_name().to_string_lossy().to_string();
            if NO_CHECKS_BY_DECISION.contains(&name.as_str()) {
                continue;
            }
            if !p.join("checks.yml").exists() {
                missing.push(format!("{}/{}", stack.file_name().to_string_lossy(), name));
            }
        }
    }
    assert!(seen > 30, "the app sweep broke: {} found", seen);
    missing.sort();
    assert!(
        missing.is_empty(),
        "these apps are deployed and verified by nothing — a deploy that \
         breaks them is reported green. Write a checks.yml, or add the app to \
         NO_CHECKS_BY_DECISION with a reason: {:?}",
        missing
    );
}
