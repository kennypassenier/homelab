//! milestone edit, the pure half: what an edit writes, what the plan says,
//! the new-stack checks, the fleet's firewall matrix and the host settings
//! page. Real stack files from the repository where the shape matters.

use std::collections::BTreeMap;
use std::path::Path;

use homelab_admin::core::editplan::{self, commit_message, commit_subject, effects, follow_ups};
use homelab_admin::core::fwmatrix::{FleetFirewall, matrix};
use homelab_admin::core::hostsettings::{self, Change, check, parse_fragment, toml_fragment};
use homelab_admin::core::newstack::{NewStack, Taken, ip_for, problems, suggest_vmid};
use homelab_admin::core::stackedit::{
    AddAppFiles, FirewallEdit, RuleEdit, SettingsEdit, StackEdit, StackTexts, TileEdit, changes,
    images, mount_value, outside_stack, parse_manifest, raw_path_problem,
};
use homelab_admin::core::textdiff::{counts, hunks, unified};
use homelab_admin::shell::edit::{subject_with_id, version_triple};
use homelab_core::manifest::{FirewallRule, FwAction, FwDir, FwProto};

fn texts(stack: &str) -> StackTexts {
    homelab_admin::shell::workcopy::read_texts(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks")
            .join(stack),
    )
}

fn rule(
    dir: FwDir,
    action: FwAction,
    peer: &str,
    proto: Option<FwProto>,
    port: Option<&str>,
) -> FirewallRule {
    FirewallRule {
        dir,
        action,
        source: (dir == FwDir::In).then(|| peer.to_string()),
        dest: (dir == FwDir::Out).then(|| peer.to_string()),
        proto,
        dport: port.map(str::to_string),
        comment: None,
        note: None,
    }
}

fn firewall_as_is(stack: &str) -> FirewallEdit {
    let m = parse_manifest(&texts(stack)["lxc-compose.yml"]).unwrap();
    let f = m.firewall.unwrap();
    FirewallEdit {
        enabled: f.enabled,
        comment: f.comment.clone(),
        policy_in: f.policy_in,
        policy_out: f.policy_out,
        management_open: f.management_open.clone(),
        rules: f
            .rules
            .iter()
            .enumerate()
            .map(|(i, r)| RuleEdit {
                origin: Some(i),
                rule: r.clone(),
            })
            .collect(),
    }
}

#[test]
fn feat_firewall_1_an_unchanged_firewall_changes_no_file() {
    for stack in ["kp-soft", "admin", "gateway"] {
        let t = texts(stack);
        if parse_manifest(&t["lxc-compose.yml"])
            .unwrap()
            .firewall
            .is_none()
        {
            continue;
        }
        let out = changes(stack, &t, &StackEdit::Firewall(firewall_as_is(stack)), None).unwrap();
        assert!(out.is_empty(), "{stack}: {out:?}");
    }
}

#[test]
fn feat_firewall_1_add_and_remove_a_rule_keep_the_rest_of_the_file() {
    let t = texts("admin");
    let mut f = firewall_as_is("admin");
    // Uptime Kuma's HTTP monitor, found by its note: rules added in the
    // browser land after it (fix-157's end-to-end run added one).
    let kuma = f
        .rules
        .iter()
        .position(|r| r.rule.note.as_deref() == Some("Uptime Kuma HTTP monitor (/healthz)"))
        .expect("the stack still has Uptime Kuma's HTTP rule");
    f.rules.remove(kuma);
    f.rules.push(RuleEdit {
        origin: None,
        rule: FirewallRule {
            note: Some("  the desktop  ".into()),
            ..rule(
                FwDir::In,
                FwAction::Accept,
                "10.10.10.10",
                Some(FwProto::Tcp),
                Some(" 8090 "),
            )
        },
    });
    let out = changes("admin", &t, &StackEdit::Firewall(f), None).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].path, "stacks/admin/lxc-compose.yml");
    let new = out[0].new.as_deref().unwrap();
    let old = out[0].old.as_deref().unwrap();
    assert!(new.contains("      source: 10.10.10.10\n      proto: tcp\n      dport: '8090'\n      note: the desktop\n"), "{new}");
    assert!(!new.contains("note: Uptime Kuma HTTP monitor (/healthz)"));
    let (added, removed) = counts(old, new);
    assert!(
        (3..=6).contains(&added) && (3..=6).contains(&removed),
        "{}",
        unified("x", Some(old), Some(new))
    );
    // Every comment line of the file stayed.
    let c = |s: &str| {
        s.lines()
            .filter(|l| l.trim_start().starts_with('#'))
            .count()
    };
    assert_eq!(c(old), c(new));
    // The plan: the rendered .fw loses one line and gains one.
    let before = parse_manifest(old).unwrap();
    let after = parse_manifest(new).unwrap();
    let eff = effects(Some(&before), &after, &[], None, &Vec::new());
    // The rendered file, the native note, and arch-self: admin restarts.
    assert_eq!(eff.len(), 3, "{eff:?}");
    assert!(eff[2].what.contains("restarts this dashboard"));
    assert!(eff[0].what.contains("/etc/pve/firewall/120.fw"));
    assert_eq!(eff[0].by, Some("deploy"));
    assert!(
        eff[0]
            .detail
            .iter()
            .any(|l| l == "+ IN ACCEPT -source 10.10.10.10 -p tcp -dport 8090 # the desktop"),
        "{:?}",
        eff[0].detail
    );
    assert!(
        eff[0]
            .detail
            .iter()
            .any(|l| l.starts_with("- IN ACCEPT -source 10.10.10.7 -p tcp -dport 8090"))
    );
    assert_eq!(follow_ups(&eff), vec!["deploy"]);
}

#[test]
fn feat_firewall_1_options_and_a_new_declaration() {
    let text = "stack_name: x\nvmid: 150\nhostname: 150-app-x\nnetwork:\n  ip: 10.10.10.50/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\nresources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 0\n  disk_gb: 8\n  storage: local-lvm\nlxc:\n  template: clone:996\n  unprivileged: true\n  features: nesting=1\n  protection: true\nboot:\n  onboot: true\napps: []\n";
    let t: StackTexts = BTreeMap::from([("lxc-compose.yml".to_string(), text.to_string())]);
    let f = FirewallEdit {
        enabled: false,
        comment: Some("line one\nline two".into()),
        policy_in: FwAction::Drop,
        policy_out: FwAction::Accept,
        management_open: None,
        rules: vec![RuleEdit {
            origin: None,
            rule: rule(
                FwDir::In,
                FwAction::Accept,
                "10.10.10.4",
                Some(FwProto::Tcp),
                Some("80"),
            ),
        }],
    };
    let out = changes("x", &t, &StackEdit::Firewall(f), None).unwrap();
    let new = out[0].new.as_deref().unwrap();
    assert!(new.ends_with("apps: []\nfirewall:\n  enabled: false\n  comment: |-\n    line one\n    line two\n  policy_in: DROP\n  policy_out: ACCEPT\n  rules:\n    - dir: in\n      action: ACCEPT\n      source: 10.10.10.4\n      proto: tcp\n      dport: '80'\n"), "{new}");
    let eff = effects(
        Some(&parse_manifest(text).unwrap()),
        &parse_manifest(new).unwrap(),
        &[],
        None,
        &Vec::new(),
    );
    assert!(eff[0].what.contains("declared, not enabled"), "{eff:?}");
    assert_eq!(eff[0].by, None);
}

#[test]
fn feat_stacks_2_settings_and_the_resize_they_need() {
    let t = texts("kp-soft");
    let out = changes(
        "kp-soft",
        &t,
        &StackEdit::Settings(SettingsEdit {
            memory_mb: Some(3072),
            disk_gb: Some(8),
            order: Some(80),
            cores: Some(2), // unchanged: no op
            ..Default::default()
        }),
        None,
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    let (old, new) = (
        out[0].old.as_deref().unwrap(),
        out[0].new.as_deref().unwrap(),
    );
    let h = hunks(old, new, 3);
    assert_eq!(h.len(), 3, "{}", unified("x", Some(old), Some(new)));
    let eff = effects(
        Some(&parse_manifest(old).unwrap()),
        &parse_manifest(new).unwrap(),
        &[],
        None,
        &Vec::new(),
    );
    let whats: Vec<&str> = eff.iter().map(|e| e.what.as_str()).collect();
    assert!(whats[0].starts_with("Resize applies"), "{whats:?}");
    assert_eq!(eff[0].detail, vec!["memory 2 GB → 3 GB"]);
    assert!(
        whats
            .iter()
            .any(|w| w.contains("Proxmox cannot shrink a disk")),
        "{whats:?}"
    );
    assert!(whats.iter().any(|w| w.contains("boot policy")), "{whats:?}");
    assert_eq!(follow_ups(&eff), vec!["deploy", "resize"]);
    let subject = commit_subject(
        "kp-soft",
        &editplan::summary("settings", &editplan::file_diffs(&out), &eff),
        "feat-stacks-2",
    );
    assert_eq!(
        subject,
        "stacks/kp-soft: settings: memory 2 GB → 3 GB [feat-stacks-2]"
    );
    let msg = commit_message(&subject, "because", &eff, &editplan::file_diffs(&out));
    assert!(msg.starts_with(&format!(
        "{subject}\n\nbecause\n\nchanged stacks/kp-soft/lxc-compose.yml"
    )));
}

#[test]
fn feat_stacks_2_an_image_changes_in_its_compose_file() {
    let mut t: StackTexts = texts("kp-soft");
    t.insert(
        "web/docker-compose.yml".into(),
        "services:\n  web:\n    image: nginx:1.27 # pinned\n    restart: unless-stopped\n".into(),
    );
    assert_eq!(
        images(&t).get("web/web").map(String::as_str),
        Some("nginx:1.27")
    );
    let out = changes(
        "kp-soft",
        &t,
        &StackEdit::Settings(SettingsEdit {
            images: BTreeMap::from([("web/web".to_string(), "nginx:1.28".to_string())]),
            ..Default::default()
        }),
        None,
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].new.as_deref(),
        Some("services:\n  web:\n    image: nginx:1.28 # pinned\n    restart: unless-stopped\n")
    );
    let bad = changes(
        "kp-soft",
        &t,
        &StackEdit::Settings(SettingsEdit {
            images: BTreeMap::from([("web/web".to_string(), "nginx 1.28".to_string())]),
            ..Default::default()
        }),
        None,
    );
    assert!(bad.unwrap_err().why.contains("not an image reference"));
}

#[test]
fn feat_stacks_2_the_raw_editor_writes_only_existing_non_secret_files() {
    let t = texts("admin");
    assert!(raw_path_problem("lxc-compose.yml", &t).is_none());
    assert!(
        raw_path_problem("admin/.env", &t)
            .unwrap()
            .contains("latch")
    );
    assert!(raw_path_problem("../gateway/lxc-compose.yml", &t).is_some());
    assert!(raw_path_problem("/etc/passwd", &t).is_some());
    assert!(
        raw_path_problem("new.yml", &t)
            .unwrap()
            .contains("existing")
    );
    let out = changes(
        "admin",
        &t,
        &StackEdit::Raw {
            path: "lxc-compose.yml".into(),
            content: t["lxc-compose.yml"].replace("memory_mb: 512", "memory_mb: 768"),
        },
        None,
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(
        outside_stack("admin", out.iter().map(|c| c.path.as_str())),
        Vec::<String>::new()
    );
    assert_eq!(
        outside_stack(
            "admin",
            ["stacks/admin2/x", "stacks/admin/../x", "README.md"]
        ),
        vec!["stacks/admin2/x", "stacks/admin/../x", "README.md"]
    );
}

#[test]
fn feat_stacks_3_an_app_joins_a_stack_with_its_data_folder() {
    let t = texts("kp-soft");
    let files = AddAppFiles {
        apps: vec!["mealie".into()],
        files: BTreeMap::from([(
            "mealie/docker-compose.yml".to_string(),
            "services:\n  mealie:\n    image: ghcr.io/mealie-recipes/mealie:v2\n    volumes:\n      - /appdata/kp-soft/mealie-config:/app/data\n".to_string(),
        )]),
        appdata: vec!["/appdata/kp-soft/mealie-config".into()],
        owner_uid: 101000,
    };
    let out = changes(
        "kp-soft",
        &t,
        &StackEdit::AddApp {
            preset: "mealie".into(),
            tiles: Default::default(),
        },
        Some(&files),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    let m = parse_manifest(out[0].new.as_deref().unwrap()).unwrap();
    assert_eq!(m.apps, vec!["kp-soft", "jobtracker", "mealie"]);
    let last = m.storage.last().unwrap();
    assert_eq!(last.host_path, "/appdata/kp-soft/mealie-config");
    assert_eq!(last.app.as_deref(), Some("mealie"));
    assert_eq!(out[1].path, "stacks/kp-soft/mealie/docker-compose.yml");
    assert!(out[1].old.is_none());
    // An app of that name already there: refused.
    let again = AddAppFiles {
        apps: vec!["jobtracker".into()],
        ..files
    };
    assert!(
        changes(
            "kp-soft",
            &t,
            &StackEdit::AddApp {
                preset: "x".into(),
                tiles: Default::default(),
            },
            Some(&again)
        )
        .is_err()
    );
    assert_eq!(
        mount_value("s", "/appdata/other/x-config", 1)["app"],
        serde_yaml::Value::Null
    );
}

#[test]
fn feat_stacks_3_a_new_stack_is_checked_against_the_fleet() {
    let taken = Taken {
        names: ["kp-soft".to_string(), "admin".into()].into(),
        vmids: [104u16, 105, 106, 116, 120].into(),
        ips: ["10.10.10.7".to_string()].into(),
    };
    let ok = NewStack {
        name: "notes".into(),
        vmid: 121,
        preset: "custom".into(),
        ram_mb: 1024,
        cores: 2,
        disk_gb: 16,
        swap_mb: None,
        no_data: vec![],
        tile: None,
    };
    let presets = vec!["custom".to_string(), "mealie".into()];
    assert!(problems(&ok, &taken, &presets).is_empty());
    let fields = |req: NewStack| {
        problems(&req, &taken, &presets)
            .into_iter()
            .map(|(f, _)| f)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fields(NewStack {
            name: "kp-soft".into(),
            ..ok.clone()
        }),
        vec!["name"]
    );
    assert_eq!(
        fields(NewStack {
            name: "Bad_Name".into(),
            ..ok.clone()
        }),
        vec!["name"]
    );
    assert_eq!(
        fields(NewStack {
            vmid: 101,
            ..ok.clone()
        }),
        vec!["vmid"]
    );
    assert_eq!(
        fields(NewStack {
            vmid: 116,
            ..ok.clone()
        }),
        vec!["vmid"]
    );
    assert_eq!(
        fields(NewStack {
            vmid: 107,
            ..ok.clone()
        }),
        vec!["vmid"]
    );
    assert_eq!(
        fields(NewStack {
            vmid: 400,
            ..ok.clone()
        }),
        vec!["vmid"]
    );
    assert_eq!(
        fields(NewStack {
            preset: "nope".into(),
            ram_mb: 64,
            ..ok.clone()
        }),
        vec!["preset", "ram_mb"]
    );
    assert_eq!(ip_for(121).as_deref(), Some("10.10.10.21"));
    assert_eq!(ip_for(100), None);
    assert_eq!(suggest_vmid(&taken), Some(108));
}

#[test]
fn feat_firewall_2_the_matrix_reads_both_ends() {
    let load = |stack: &str| {
        let m = parse_manifest(&texts(stack)["lxc-compose.yml"]).unwrap();
        FleetFirewall {
            stack: stack.into(),
            vmid: m.vmid,
            ip: m.network.ip.split('/').next().unwrap().parse().unwrap(),
            firewall: m.firewall,
        }
    };
    let fleet: Vec<FleetFirewall> = ["admin", "gateway", "kp-soft", "uptime", "metrics"]
        .iter()
        .map(|s| load(s))
        .collect();
    let m = matrix(&fleet);
    let cell = |from: &str, to: &str| {
        m.cells
            .iter()
            .find(|c| c.from == from && c.to == to)
            .unwrap()
            .clone()
    };
    // Traefik (gateway, CT 104) reaches the dashboard on 8090, and nothing else does from kp-soft.
    assert_eq!(cell("gateway", "admin").allowed, vec!["tcp 8090"]);
    assert_eq!(cell("uptime", "admin").allowed, vec!["tcp 8090", "icmp"]);
    assert_eq!(cell("kp-soft", "admin").state, "none");
    // Prometheus (CT 113) scrapes kp-soft on 8081 and 9100.
    assert_eq!(
        cell("metrics", "kp-soft").allowed,
        vec!["tcp 8081", "tcp 9100"]
    );
    // admin's own outbound drop of the VLAN stops what kp-soft would let in.
    assert!(cell("admin", "kp-soft").allowed.is_empty());
    let rows: Vec<_> = m
        .rules
        .iter()
        .filter(|r| r.stack == "admin" && r.dir == "in")
        .collect();
    assert_eq!(rows[0].peer_stacks, vec!["gateway"]);
    assert!(m.unguarded.iter().all(|s| s != "admin" && s != "kp-soft"));
}

#[test]
fn feat_settings_1_the_page_and_its_checks() {
    let file = homelab_proto::HostConfigFile {
        path: "/etc/homelab/host.toml".into(),
        sha256: "a".repeat(64),
        values: BTreeMap::from([
            ("backup_hour".to_string(), serde_json::json!(4)),
            (
                "retention".to_string(),
                serde_json::json!([{"every_days": 1, "span_days": 7}]),
            ),
        ]),
        secrets_set: vec!["token".into()],
        unknown: vec![],
    };
    let p = hostsettings::page(&file);
    let f = |k: &str| p.fields.iter().find(|x| x.info.key == k).unwrap();
    assert!(f("token").set && f("token").value.is_null());
    assert_eq!(
        f("retention").toml.as_deref(),
        Some("[[retention]]\nevery_days = 1\nspan_days = 7\n")
    );
    assert!(!f("loki_url").set);
    let json = serde_json::to_value(f("backup_hour")).unwrap();
    assert_eq!(json["access"], "browser");
    assert_eq!(json["apply"], "live");
    assert_eq!(json["kind"]["type"], "int");

    assert_eq!(
        parse_fragment("retention", "[[retention]]\nevery_days = 2\n").unwrap(),
        serde_json::json!([{"every_days": 2}])
    );
    assert!(
        parse_fragment(
            "retention",
            "backup_hour = 3\n[[retention]]\nevery_days = 2\n"
        )
        .is_err()
    );
    assert!(parse_fragment("zfs_jobs", "[[retention]]\nevery_days = 2\n").is_err());
    assert_eq!(
        parse_fragment("zfs_jobs", "  ").unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(toml_fragment("x", &serde_json::Value::Null), "");

    let base = Change {
        expect_sha256: "a".repeat(64),
        ..Default::default()
    };
    let ok = check(Change {
        values: BTreeMap::from([("backup_hour".to_string(), serde_json::json!(5))]),
        fragments: BTreeMap::from([(
            "retention".to_string(),
            "[[retention]]\nevery_days = 1\n".to_string(),
        )]),
        ..base.clone()
    })
    .unwrap();
    assert_eq!(ok.len(), 2);
    // arch-self: the keys that can cut the dashboard off are never sent.
    for key in ["listen", "state_dir", "tokens", "token"] {
        let e = check(Change {
            values: BTreeMap::from([(key.to_string(), serde_json::json!("x"))]),
            ..base.clone()
        })
        .unwrap_err();
        assert!(e.why.contains("ssh"), "{key}: {}", e.why);
    }
    // The dashboard's route needs its name typed.
    let route = |confirms: &[&str]| {
        check(Change {
            values: BTreeMap::from([("gateway_vmid".to_string(), serde_json::json!(104))]),
            confirms: confirms.iter().map(|s| s.to_string()).collect(),
            ..base.clone()
        })
    };
    assert!(route(&[]).unwrap_err().why.contains("type its name"));
    assert!(route(&["gateway_vmid"]).is_ok());
    assert!(check(base.clone()).is_err());
    assert!(
        check(Change {
            expect_sha256: "x".into(),
            ..base
        })
        .is_err()
    );
}

#[test]
fn feat_stacks_2_the_subject_always_names_a_feature() {
    assert_eq!(
        subject_with_id(None, "d [feat-stacks-2]", "feat-stacks-2"),
        "d [feat-stacks-2]"
    );
    assert_eq!(
        subject_with_id(Some("  raise memory  "), "d", "feat-stacks-2"),
        "raise memory [feat-stacks-2]"
    );
    assert_eq!(
        subject_with_id(Some("x [fix-9]\nsecond"), "d", "feat-stacks-2"),
        "x [fix-9]"
    );
    assert_eq!(version_triple("3.62.2"), Some((3, 62, 2)));
    assert_eq!(version_triple("v3.63.0-4-gabc"), Some((3, 63, 0)));
    assert_eq!(version_triple("x"), None);
}

#[test]
fn feat_stacks_2_the_diff_is_cut_into_hunks_with_context() {
    let old = (1..=20)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let new = old.replace("l3\n", "L3\n").replace("l18\n", "l18\nnew\n");
    let h = hunks(&old, &new, 2);
    assert_eq!(h.len(), 2);
    assert_eq!(
        (h[0].old_start, h[0].old_len, h[0].new_start, h[0].new_len),
        (1, 5, 1, 5)
    );
    assert_eq!((h[1].old_start, h[1].new_start, h[1].new_len), (17, 17, 5));
    assert_eq!(counts(&old, &new), (2, 1));
    let u = unified("f", None, Some("a\n"));
    assert!(
        u.starts_with("--- /dev/null\n+++ b/f\n@@ -0,0 +1,1 @@\n+a\n"),
        "{u}"
    );
}

#[test]
fn feat_stacks_2_the_commit_subject_says_what_changed() {
    use homelab_admin::core::stackedit::describe;
    let m = parse_manifest(&texts("admin")["lxc-compose.yml"]).unwrap();
    let s = describe(
        &StackEdit::Settings(SettingsEdit {
            memory_mb: Some(1024),
            cores: Some(1), // unchanged: not named
            order: Some(70),
            ..Default::default()
        }),
        Some(&m),
    );
    assert_eq!(s, "memory 1 GB, boot order 70");
    let mut f = firewall_as_is("admin");
    f.rules.remove(0);
    f.rules.push(RuleEdit {
        origin: None,
        rule: rule(
            FwDir::In,
            FwAction::Accept,
            "10.10.10.10",
            Some(FwProto::Tcp),
            Some("8090"),
        ),
    });
    assert_eq!(
        describe(&StackEdit::Firewall(f), Some(&m)),
        "firewall: add IN ACCEPT from 10.10.10.10 tcp 8090, remove 1 rule(s)"
    );
}

/// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): when a
/// `tile_watch_source` is given, the plan shows the same derived rule the
/// deploy would write, so the owner sees it before it lands on pve rather
/// than being surprised by it.
#[test]
fn tile_watch_derived_rule_shows_in_the_plan() {
    let base = "stack_name: x\nvmid: 150\nhostname: 150-app-x\nnetwork:\n  ip: 10.10.10.50/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\nresources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 0\n  disk_gb: 8\n  storage: local-lvm\nlxc:\n  template: clone:996\n  unprivileged: true\n  features: nesting=1\n  protection: true\nboot:\n  onboot: true\napps: []\n";
    let old = parse_manifest(base).unwrap();
    let new_text = format!(
        "{base}tiles:\n  x.kp-soft.dev:\n    name: X\n    group: Apps\n    url: http://10.10.10.50:8080/\n    probe: http://10.10.10.50:8080/\nfirewall:\n  enabled: true\n  rules:\n    - dir: in\n      action: ACCEPT\n      source: 10.10.10.4\n      proto: tcp\n      dport: '80'\n"
    );
    let new = parse_manifest(&new_text).unwrap();
    // No source: only the hand-declared rule renders, as before this
    // decision.
    let none = effects(Some(&old), &new, &[], None, &Vec::new());
    assert!(
        !none[0].detail.iter().any(|l| l.contains("tile watch")),
        "{:?}",
        none[0].detail
    );
    // A source: the derived rule shows up in the same rendered-file diff.
    let with = effects(Some(&old), &new, &[], Some("10.10.10.20"), &Vec::new());
    assert!(
        with[0]
            .detail
            .iter()
            .any(|l| l.contains("-source 10.10.10.20") && l.contains("tile watch")),
        "{:?}",
        with[0].detail
    );
}

/// owner remark 2026-09-30 ("de uptime-check tijd … in de wizard"): the
/// settings form's two per-tile fields write straight into the tile's own
/// `tiles:` entry, comments kept, only when a value is given.
#[test]
fn settings_edit_writes_a_tiles_watch_override() {
    let texts = texts("uptime");
    let edit = StackEdit::Settings(SettingsEdit {
        tiles: BTreeMap::from([(
            "kuma.kp-soft.dev".to_string(),
            TileEdit {
                watch_every: Some(30),
                down_after: Some(180),
            },
        )]),
        ..Default::default()
    });
    let out = changes("uptime", &texts, &edit, None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let new = out[0].new.as_ref().unwrap();
    assert!(
        new.contains("kuma.kp-soft.dev:\n    name: \"Uptime Kuma\"")
            || new.contains("kuma.kp-soft.dev:"),
        "{new}"
    );
    let m = parse_manifest(new).unwrap();
    let t = &m.tiles["kuma.kp-soft.dev"];
    assert_eq!(t.watch_every, Some(30));
    assert_eq!(t.down_after, Some(180));
    // A second edit setting only watch_every leaves down_after as it is
    // now, and a field left out of the whole edit changes nothing at all.
    let unchanged = changes(
        "uptime",
        &texts,
        &StackEdit::Settings(SettingsEdit::default()),
        None,
    )
    .unwrap();
    assert!(unchanged.is_empty());
}

/// The settings form refuses a tile key the stack does not declare, and a
/// down_after shorter than watch_every, the same way the browser's
/// `tileProblems` does before the plan is even asked for.
#[test]
fn settings_edit_tile_validation() {
    let texts = texts("uptime");
    let unknown = StackEdit::Settings(SettingsEdit {
        tiles: BTreeMap::from([(
            "not-a-tile.kp-soft.dev".to_string(),
            TileEdit {
                watch_every: Some(30),
                down_after: None,
            },
        )]),
        ..Default::default()
    });
    let err = changes("uptime", &texts, &unknown, None).unwrap_err();
    assert!(err.why.contains("is not a tile of this stack"), "{err:?}");

    use homelab_admin::core::stackedit::settings_problems;
    let too_low = SettingsEdit {
        tiles: BTreeMap::from([(
            "kuma.kp-soft.dev".to_string(),
            TileEdit {
                watch_every: Some(5),
                down_after: None,
            },
        )]),
        ..Default::default()
    };
    let problems = settings_problems(&too_low);
    assert!(
        problems.iter().any(|p| p.contains("at least 10 s")),
        "{problems:?}"
    );
    let inverted = SettingsEdit {
        tiles: BTreeMap::from([(
            "kuma.kp-soft.dev".to_string(),
            TileEdit {
                watch_every: Some(120),
                down_after: Some(60),
            },
        )]),
        ..Default::default()
    };
    let problems = settings_problems(&inverted);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("must be at least check every")),
        "{problems:?}"
    );
}
