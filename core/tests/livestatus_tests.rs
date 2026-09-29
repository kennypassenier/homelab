//! feat-platform-2 · real status per container. The samples are real
//! output, read on pve on 2026-09-28 (pvesh trimmed to the fields used).

use homelab_core::executor::CmdOutput;
use homelab_core::mock::MockExecutor;
use homelab_core::ops::livestatus::{parse_apps, parse_guests, read, Target};

const PVESH: &str = r#"[{"cpu":0.0409371412846904,"id":"qemu/100","maxmem":4294967296,"mem":3241291776,"name":"100-infra-opnsense","status":"running","type":"qemu","uptime":5132737,"vmid":100},
{"cpu":0.012,"id":"lxc/106","maxmem":8589934592,"mem":2147483648,"name":"106-app-media","status":"running","type":"lxc","uptime":3600,"vmid":106},
{"cpu":0,"id":"lxc/119","maxmem":1073741824,"mem":0,"name":"119-app-drill","status":"stopped","type":"lxc","uptime":0,"vmid":119}]"#;

const MEDIA_PROBE: &str = "/opt/media/recyclarr|recyclarr|true|0
/opt/media/jellyfin|jellyfin|true|3
/opt/media/arr|sonarr|true|1
/opt/media/arr|radarr|false|7
/root/scratch|debug|true|0
";

#[test]
fn feat_platform_2_pvesh_gives_status_cpu_memory_and_uptime() {
    let g = parse_guests(PVESH).unwrap();
    assert_eq!(g.len(), 3);
    assert!(g[&106].running);
    assert!(!g[&119].running);
    assert_eq!(g[&100].cpu_permille, 41);
    assert_eq!(g[&106].mem_used_mb, 2048);
    assert_eq!(g[&106].mem_max_mb, 8192);
    assert_eq!(g[&106].uptime_s, 3600);
}

#[test]
fn feat_platform_2_apps_group_their_containers_and_sum_restarts() {
    let a = parse_apps("media", MEDIA_PROBE);
    assert_eq!(a.len(), 3, "{:?}", a);
    assert!(a["recyclarr"].running);
    assert_eq!(a["jellyfin"].restarts, 3);
    assert!(
        !a["arr"].running,
        "one stopped container makes the app not running"
    );
    assert_eq!(a["arr"].containers, 2);
    assert_eq!(a["arr"].restarts, 8);
    assert!(
        !a.contains_key("scratch"),
        "a container outside /opt/<stack>/ is no app"
    );
}

#[test]
fn feat_platform_2_garbage_is_an_error_not_an_empty_fleet() {
    assert!(parse_guests("not json").is_err());
}

#[tokio::test]
async fn feat_platform_2_a_reading_probes_only_running_guests_and_names_failures() {
    let exec = MockExecutor::new();
    exec.respond_always("pvesh get /cluster/resources", CmdOutput::ok(PVESH));
    exec.respond_always("pct exec 106", CmdOutput::ok(MEDIA_PROBE));
    let targets = vec![
        Target {
            vmid: 106,
            stack: "media".into(),
            units: vec![],
        },
        Target {
            vmid: 119,
            stack: "drill".into(),
            units: vec![],
        },
    ];
    let s = read(&exec, &targets, 42).await;
    assert_eq!(s.measured_at, 42);
    assert_eq!(s.apps[&106]["jellyfin"].restarts, 3);
    assert!(!s.apps.contains_key(&119), "a stopped guest is not probed");
    assert_eq!(exec.calls_containing("pct exec 119").len(), 0);

    let failing = MockExecutor::new();
    failing.respond_always(
        "pvesh",
        CmdOutput {
            stdout: String::new(),
            stderr: "no quorum".into(),
            code: 2,
        },
    );
    let s = read(&failing, &targets, 1).await;
    assert!(s.guests.is_empty());
    assert!(
        s.probe_errors[&0].contains("no quorum"),
        "{:?}",
        s.probe_errors
    );
}

/// fix-160: a native stack's app is its systemd unit. The probe used to know
/// only docker, so the admin dashboard (unit `admin`, no docker on CT 120)
/// read as "0 of 1 apps running" while `systemctl is-active admin` said
/// active (measured on pve, 2026-09-29 15:05).
#[test]
fn fix_160_a_native_unit_line_is_an_app() {
    let a = parse_apps("admin", "unit:admin|admin|true|2\n");
    assert!(a["admin"].running, "{:?}", a);
    assert_eq!(a["admin"].restarts, 2);
    let b = parse_apps("kyu", "unit:kyu|kyu|false|0\n/opt/kyu/x|x|true|0\n");
    assert!(!b["kyu"].running);
    assert!(b["x"].running, "docker apps still count beside units");
}

#[tokio::test]
async fn fix_160_the_probe_asks_systemd_about_every_native_unit() {
    const PVESH_120: &str = r#"[{"cpu":0.01,"id":"lxc/120","maxmem":1073741824,"mem":104857600,"name":"120-app-admin","status":"running","type":"lxc","uptime":600,"vmid":120}]"#;
    let exec = MockExecutor::new();
    exec.respond_always("pvesh get /cluster/resources", CmdOutput::ok(PVESH_120));
    exec.respond_always("120", CmdOutput::ok("unit:admin|admin|true|0\n"));
    let targets = vec![Target {
        vmid: 120,
        stack: "admin".into(),
        units: vec!["admin".into()],
    }];
    let s = read(&exec, &targets, 7).await;
    assert!(s.apps[&120]["admin"].running, "{:?}", s.apps);
    let calls = exec.calls_containing("120");
    assert_eq!(calls.len(), 1, "{:?}", calls);
    assert!(
        calls[0].contains("systemctl is-active admin"),
        "the probe names the unit: {}",
        calls[0]
    );
}
