//! Decision "Notifications and Grafana" (2026-09-30): only urgent reaches the
//! phone at once, everything lands in the dashboard's notification centre,
//! and every notice says what is wrong, since when, the consequence and what
//! to do, with a link to the right dashboard page.

use homelab_core::error::OperatorError;
use homelab_core::notify::{
    click_url, explain_event, explain_fleet_check, explain_op, next_seq, notices_after, op_kind,
    op_stack, parse_notices, prune_notices, push_payload, push_short, push_status_short, urgency,
    Event, HostNotice, OpFacts, OpKind, BACKUP_OPS, DEPLOY_OPS, DISK_ALERTS, PIPELINE_ALERTS,
    SERVICE_DOWN_ALERTS, UPDATE_OPS,
};
use homelab_core::ops::fleetcheck::{Finding, Severity};

fn op(label: &str, ok: bool) -> Event<'_> {
    Event::Op {
        label,
        ok,
        deferred: false,
    }
}

#[test]
fn notify_routing_a_failed_backup_update_or_deploy_is_urgent() {
    for label in [
        "deploy",
        "install-native",
        "backup",
        "scheduled-backup",
        "backup-native",
        "scheduled-backup-native",
        "host-meta-backup",
        "device-backup",
        "second-copy",
        "restic-check",
        "zfs-replicate",
        "update",
        "scheduled-update",
        "update-native",
        "scheduled-update-native",
        "release-update-native",
        "scheduled-release-update",
        "self-update",
        "patch",
        "rollback-native",
    ] {
        assert!(urgency(&op(label, false)).urgent, "{label} failed");
        assert!(!urgency(&op(label, true)).urgent, "{label} succeeded");
    }
}

#[test]
fn notify_routing_everything_else_goes_to_the_centre_only() {
    for label in [
        "destroy",
        "forget",
        "resize",
        "set-enabled",
        "adopt",
        "restore",
        "apply-guards",
        "template-build",
        "home-address-whitelist",
        "wipe",
        "boot",
    ] {
        let u = urgency(&op(label, false));
        assert!(!u.urgent, "{label}");
        assert!(!u.why.is_empty());
    }
    // Standing aside is not a failure.
    let deferred = urgency(&Event::Op {
        label: "scheduled-backup",
        ok: false,
        deferred: true,
    });
    assert!(!deferred.urgent && deferred.why.contains("stood aside"));
    // The nightly check: only a broken finding reaches the phone.
    assert!(urgency(&Event::FleetCheck { broken: 1 }).urgent);
    assert!(!urgency(&Event::FleetCheck { broken: 0 }).urgent);
    // push-edge (Kenny, 2026-09-30): a parked stack and a restart that
    // interrupted work reach the phone; a clean restart does not.
    assert!(urgency(&Event::Parked).urgent);
    assert!(urgency(&Event::Boot { interrupted: true }).urgent);
    assert!(!urgency(&Event::Boot { interrupted: false }).urgent);
}

#[test]
fn notify_routing_the_urgent_alerts_are_an_explicit_list() {
    for a in [
        "HostDown",
        "TargetDown",
        "FilesystemAlmostFull",
        "PveStorageAlmostFull",
        "HypervisorRootFillingUp",
        "DiskPendingSectors",
        "DiskSmartFailed",
        "ZpoolNotOnline",
        "DriveMissing",
        // push-edge (Kenny, 2026-09-30): "Ook meteen".
        "AlmanacJournalUnreadable",
        "SystemdUnitFailed",
        "AlertDeliveryFailing",
    ] {
        assert!(urgency(&Event::Alert { alertname: a }).urgent, "{a}");
    }
    for a in [
        "SmartCollectorStale",
        "KyuBacklogGrowing",
        "TraefikServerErrors",
        "ContainerMemoryLow",
        "SomethingNew",
    ] {
        assert!(!urgency(&Event::Alert { alertname: a }).urgent, "{a}");
    }
    assert_eq!(
        SERVICE_DOWN_ALERTS.len() + DISK_ALERTS.len() + PIPELINE_ALERTS.len(),
        12
    );
    assert!(!BACKUP_OPS.is_empty() && !UPDATE_OPS.is_empty() && !DEPLOY_OPS.is_empty());
}

/// Owner decision 2026-09-30 (item 3): the notifications table's push
/// column is one of a handful of fixed words, never the full routing
/// reason ("not sent: not urgent: it succeeded" was far too long).
#[test]
fn notify_routing_push_status_short_is_a_fixed_handful_of_words() {
    assert_eq!(push_status_short(true, false, "whatever"), "Pushed");
    // A push attempt's own outcome wins over any routing reason text.
    assert_eq!(
        push_status_short(true, true, "not urgent: it succeeded"),
        "Pushed"
    );
    assert_eq!(push_status_short(false, true, "anything"), "Push failed");
    assert_eq!(
        push_status_short(
            false,
            false,
            "not pushed again: the same failure went out within 20 h"
        ),
        "No push · repeat"
    );
    assert_eq!(
        push_status_short(false, false, urgency(&op("deploy", true)).why),
        "No push · succeeded"
    );
    assert_eq!(
        push_status_short(
            false,
            false,
            urgency(&Event::Op {
                label: "deploy",
                ok: false,
                deferred: true
            })
            .why
        ),
        "No push · stood aside"
    );
    assert_eq!(
        push_status_short(false, false, urgency(&Event::FleetCheck { broken: 0 }).why),
        "No push · drift only"
    );
    assert_eq!(
        push_status_short(
            false,
            false,
            urgency(&Event::Boot { interrupted: false }).why
        ),
        "No push · resolved"
    );
    assert_eq!(
        push_status_short(
            false,
            false,
            urgency(&op("home-address-whitelist", false)).why
        ),
        "No push · not urgent"
    );
}

/// Alertmanager routes by the same list: its matcher is written out in
/// alertmanager.yml, and a test holds the two together.
#[test]
fn notify_routing_alertmanager_routes_urgent_alerts_to_the_phone() {
    let am = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks/metrics/alertmanager/alertmanager.yml"),
    )
    .unwrap();
    let mut names: Vec<&str> = SERVICE_DOWN_ALERTS
        .iter()
        .chain(DISK_ALERTS.iter())
        .chain(PIPELINE_ALERTS.iter())
        .copied()
        .collect();
    names.sort();
    let matcher = format!("alertname=~\"{}\"", names.join("|"));
    assert!(am.contains(&matcher), "want {matcher} in alertmanager.yml");
    assert!(am.contains("receiver: admin"), "{am}");
    assert!(am.contains("url: http://10.10.10.20:8090/hooks/alertmanager"));
    assert!(am.contains("credentials_file: /alertmanager/admin-token"));
}

/// Every rule says the consequence and what to do, and links a page.
#[test]
fn notify_detail_every_alert_rule_carries_consequence_remedy_and_link() {
    let rules = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks/metrics/prometheus/rules/homelab.rules.yml"),
    )
    .unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&rules).unwrap();
    let mut n = 0;
    for g in doc["groups"].as_sequence().unwrap() {
        for r in g["rules"].as_sequence().unwrap() {
            let name = r["alert"].as_str().unwrap();
            let a = &r["annotations"];
            for k in [
                "summary",
                "description",
                "consequence",
                "remedy",
                "click_url",
            ] {
                assert!(
                    a[k].as_str().is_some_and(|s| !s.trim().is_empty()),
                    "{name} has no {k}"
                );
            }
            assert!(
                a["click_url"].as_str().unwrap().contains("/"),
                "{name}: the link opens a dashboard page"
            );
            n += 1;
        }
    }
    assert!(n >= 16, "{n} rules");
}

#[test]
fn op_kind_and_stack_from_the_label_and_the_op() {
    assert_eq!(op_kind("scheduled-backup"), OpKind::Backup);
    assert_eq!(op_kind("deploy"), OpKind::Deploy);
    assert_eq!(op_kind("self-update"), OpKind::Update);
    assert_eq!(op_kind("destroy"), OpKind::Other);
    assert_eq!(
        op_stack("scheduled-backup", "backup-media"),
        Some("media".into())
    );
    assert_eq!(op_stack("deploy", "deploy-kp-soft"), Some("kp-soft".into()));
    assert_eq!(
        op_stack("set-enabled", "disable-books"),
        Some("books".into())
    );
    assert_eq!(op_stack("self-update", "self-update"), None);
    assert_eq!(op_stack("zfs-replicate", "zfs-replicate"), None);
    // A native's unit is not a stack name.
    assert_eq!(
        op_stack("release-update-native", "release-update-kyu"),
        None
    );
}

#[test]
fn notify_detail_a_failed_deploy_says_what_since_consequence_and_the_command() {
    let err = OperatorError {
        what: "compose up failed".into(),
        why: "image not found".into(),
        remedy: "check the image tag in docker-compose.yml".into(),
    };
    let e = explain_op(&OpFacts {
        op: "deploy-media",
        label: "deploy",
        ok: false,
        deferred: None,
        error: Some(&err),
        incident: Some("1800000000-deploy-media"),
    });
    assert_eq!(e.stack.as_deref(), Some("media"));
    assert_eq!(e.page, "/stacks/media");
    assert!(e.title.contains("media") && e.title.contains("failed"));
    assert!(e.what.contains("compose up failed") && e.what.contains("image not found"));
    assert!(!e.consequence.is_empty());
    assert!(e.remedy.contains("check the image tag"), "{}", e.remedy);
    assert!(e.remedy.contains("`homelab deploy media`"), "{}", e.remedy);
    assert!(e.remedy.contains("Deploy"), "the dashboard button");
    assert!(e
        .remedy
        .contains("homelab incidents show 1800000000-deploy-media"));
}

#[test]
fn notify_detail_the_remedy_follows_the_kind_of_operation() {
    let failed = |op: &str, label: &str| {
        explain_op(&OpFacts {
            op,
            label,
            ok: false,
            deferred: None,
            error: None,
            incident: None,
        })
    };
    let b = failed("backup-home", "scheduled-backup");
    assert!(b.remedy.contains("`homelab backup home`"), "{}", b.remedy);
    assert!(b.consequence.contains("restore point"), "{}", b.consequence);
    let u = failed("update-media", "scheduled-update");
    assert!(u.remedy.contains("`homelab update media`"), "{}", u.remedy);
    let s = failed("self-update", "self-update");
    assert_eq!(s.page, "/host");
    assert!(
        s.remedy.contains("`homelab release-update`"),
        "{}",
        s.remedy
    );
    let m = failed("host-meta-backup", "host-meta-backup");
    assert!(
        m.remedy.contains("`homelab backup-host-meta`"),
        "{}",
        m.remedy
    );
    let d = failed("device-backup-opnsense", "device-backup");
    assert!(
        d.remedy.contains("`homelab backup-devices`"),
        "{}",
        d.remedy
    );
    // Success and standing aside ask nothing.
    let ok = explain_op(&OpFacts {
        op: "deploy-media",
        label: "deploy",
        ok: true,
        deferred: None,
        error: None,
        incident: None,
    });
    assert!(ok.title.contains("done") && ok.remedy.starts_with("Nothing"));
    let aside = explain_op(&OpFacts {
        op: "backup-media",
        label: "scheduled-backup",
        ok: false,
        deferred: Some("a deploy holds the stack"),
        error: None,
        incident: None,
    });
    assert!(aside.title.contains("stood aside") && aside.what.contains("a deploy holds"));
}

#[test]
fn notify_detail_host_events_and_the_nightly_check() {
    let parked = explain_event("stack-disabled-media", "media", false, Some("3 failures"));
    assert_eq!(parked.page, "/stacks/media");
    assert!(
        parked.remedy.contains("`homelab enable media`"),
        "{}",
        parked.remedy
    );
    let boot = explain_event(
        "host-online",
        "boot",
        false,
        Some("interrupted: deploy-x @ up"),
    );
    assert_eq!(boot.page, "/host");
    assert!(boot.what.contains("deploy-x"));
    let findings = vec![
        Finding {
            severity: Severity::Broken,
            subject: "media".into(),
            what: "no backup for 3 days".into(),
            remedy: "homelab backup media".into(),
        },
        Finding {
            severity: Severity::Drift,
            subject: "home".into(),
            what: "hostname drifted".into(),
            remedy: "redeploy".into(),
        },
    ];
    let e = explain_fleet_check(&findings);
    assert_eq!(e.page, "/health?block=checks");
    assert!(
        e.title.contains("1 broken") && e.title.contains("1 drift"),
        "{}",
        e.title
    );
    assert!(e.what.contains("no backup for 3 days") && e.what.contains("homelab backup media"));
    assert!(!e.consequence.is_empty() && !e.remedy.is_empty());
}

#[test]
fn notify_detail_the_push_is_short_and_carries_the_link() {
    assert_eq!(
        click_url("https://admin.kp-soft.dev/", "/stacks/media"),
        "https://admin.kp-soft.dev/stacks/media"
    );
    let long = "x".repeat(2000);
    let short = push_short("deploy media failed", &long);
    assert!(short.chars().count() <= 300, "{}", short.len());
    assert!(short.starts_with("deploy media failed"));
    let p: serde_json::Value = serde_json::from_str(&push_payload(
        "homelab-host",
        "deploy-media",
        "deploy",
        false,
        Some(&short),
        "3.64.0",
        Some("https://admin.kp-soft.dev/stacks/media"),
    ))
    .unwrap();
    assert_eq!(p["click_url"], "https://admin.kp-soft.dev/stacks/media");
    assert_eq!(p["source"], "homelab-host");
    assert_eq!(p["ok"], false);
    // Without a link the shape is the old one, field for field.
    let plain = push_payload("homelab-host", "x", "y", true, None, "1", None);
    assert_eq!(
        plain,
        homelab_core::notify::op_payload("x", "y", true, None, "1")
    );
}

fn notice(seq: u64, at: u64) -> HostNotice {
    HostNotice {
        seq,
        at,
        since: at,
        op: "deploy-media".into(),
        label: "deploy".into(),
        ok: true,
        deferred: false,
        stack: Some("media".into()),
        title: "deploy media: done".into(),
        what: String::new(),
        consequence: "Nothing to do.".into(),
        remedy: "Nothing to do.".into(),
        page: "/stacks/media".into(),
        urgent: false,
        routed: "not urgent".into(),
        push: "centre only".into(),
        incident: None,
        req: Some(7),
        by: Some("admin".into()),
        findings: Vec::new(),
    }
}

#[test]
fn host_notices_are_a_log_the_dashboard_reads_after_a_cursor() {
    let text: String = [notice(10, 100), notice(11, 200), notice(12, 300)]
        .iter()
        .map(|n| format!("{}\n", serde_json::to_string(n).unwrap()))
        .collect::<String>()
        + "{torn line\n";
    let all = parse_notices(&text);
    assert_eq!(all.len(), 3, "a torn line is skipped");
    let after = notices_after(all.clone(), 10, 100);
    assert_eq!(
        after.iter().map(|n| n.seq).collect::<Vec<_>>(),
        vec![11, 12]
    );
    // The oldest past the limit are left for the next read.
    assert_eq!(
        notices_after(all.clone(), 0, 2)
            .iter()
            .map(|n| n.seq)
            .collect::<Vec<_>>(),
        vec![10, 11]
    );
    // Pruned by age, then by size.
    let kept = prune_notices(&text, 300 + 100, 200, 1 << 20).unwrap();
    assert_eq!(
        parse_notices(&kept)
            .iter()
            .map(|n| n.seq)
            .collect::<Vec<_>>(),
        vec![11, 12]
    );
    // Sequence numbers only grow, even when the clock steps back.
    assert_eq!(next_seq(0, 5_000), 5_000);
    assert_eq!(next_seq(9_000, 5_000), 9_001);
}
