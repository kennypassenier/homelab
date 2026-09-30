//! Decision "Notifications and Grafana" (Kenny, 2026-09-30): everything
//! lands in the centre, only the urgent is pushed at once, every notice
//! says what, since when, the consequence and what to do with a link, a
//! digest at 09:00 when something waits; and (Kenny, 09:16) a remedy the
//! dashboard can run carries a Fix button that opens the action's dialog.

use homelab_admin::core::actions::ActionKind;
use homelab_admin::core::notify::{
    alert_drafts, digest, digest_due, fix_for, host_draft, push_payload, route, today_lines,
    DigestRecord, Draft, FixSource, Kind, Level, NotifyFile, PushOutcome, Settings,
};
use homelab_core::notify::HostNotice;
use homelab_core::ops::fleetcheck::{Finding, Severity};
use homelab_core::ops::today::{Item, Level as TodayLevel, Today};

fn draft(kind: Kind, label: Option<&str>) -> Draft {
    let mut d = Draft::new(kind, "backup-media", "t", "b");
    d.stack = Some("media".into());
    d.detail.label = label.map(str::to_string);
    d
}

#[test]
fn notify_routing_the_dashboard_pushes_only_the_urgent() {
    let s = Settings::default();
    // A missed backup schedule is a backup that did not happen.
    assert!(route(&s, &draft(Kind::ScheduleMissed, Some("backup")), 0)
        .push
        .is_ok());
    // A missed "enable" is not urgent.
    assert!(route(&s, &draft(Kind::ScheduleMissed, Some("enable")), 0)
        .push
        .is_err());
    // The host pushes a failed operation itself; the dashboard never doubles it.
    let why = route(&s, &draft(Kind::ActionFailed, None), 0)
        .push
        .unwrap_err();
    assert!(why.contains("host"), "{why}");
    let mut long = draft(Kind::ActionDone, None);
    long.ran_s = Some(3_600);
    assert!(
        route(&s, &long, 0).push.is_err(),
        "a long action is no longer pushed"
    );
    assert!(route(&s, &draft(Kind::Incident, None), 0).push.is_err());
    // The switches still hold for what is urgent.
    let off = Settings {
        push: false,
        ..Default::default()
    };
    assert!(route(&off, &draft(Kind::ScheduleMissed, Some("backup")), 0)
        .push
        .unwrap_err()
        .contains("off"));
}

#[test]
fn notify_detail_the_push_is_short_with_a_link() {
    let mut d = draft(Kind::ScheduleMissed, Some("backup"));
    d.detail.remedy = Some("run `homelab backup media`".into());
    d.detail.link = Some("/app/stacks/media".into());
    let p: serde_json::Value =
        serde_json::from_str(&push_payload(&d, "3.64.0", "https://admin.kp-soft.dev")).unwrap();
    assert_eq!(p["source"], "homelab-admin");
    assert_eq!(p["click_url"], "https://admin.kp-soft.dev/app/stacks/media");
    assert_eq!(p["error"], "t — run `homelab backup media`");
    // No page of its own: the notifications page.
    let bare = draft(Kind::ScheduleMissed, Some("backup"));
    let p: serde_json::Value =
        serde_json::from_str(&push_payload(&bare, "3.64.0", "https://x.test/")).unwrap();
    assert_eq!(p["click_url"], "https://x.test/app/notifications");
}

#[test]
fn fix_a_failed_operation_offers_its_own_action_again() {
    let op = |op: &str, label: &str, ok: bool, stack: Option<&str>| {
        fix_for(&FixSource::Op {
            op,
            label,
            ok,
            deferred: false,
            stack,
        })
    };
    let f = op("deploy-media", "deploy", false, Some("media")).unwrap();
    assert_eq!((f.action.as_str(), f.stack.as_str()), ("deploy", "media"));
    assert_eq!(f.label, "Deploy");
    let f = op("backup-home", "scheduled-backup", false, Some("home")).unwrap();
    assert_eq!((f.action.as_str(), f.stack.as_str()), ("backup", "home"));
    let f = op("self-update", "self-update", false, None).unwrap();
    assert_eq!(
        (f.action.as_str(), f.stack.as_str()),
        ("update-host", "_host")
    );
    let f = op("stack-disabled-books", "books", false, None).unwrap();
    assert_eq!((f.action.as_str(), f.stack.as_str()), ("enable", "books"));
    assert!(op("deploy-media", "deploy", true, Some("media")).is_none());
    assert!(op("release-update-kyu", "release-update-native", false, None).is_none());
    assert!(op("destroy-x", "destroy", false, Some("x")).is_none());
    assert!(fix_for(&FixSource::Op {
        op: "backup-media",
        label: "scheduled-backup",
        ok: false,
        deferred: true,
        stack: Some("media"),
    })
    .is_none());
}

#[test]
fn fix_a_remedy_that_names_a_command_the_dashboard_runs() {
    let t = |s: &str| fix_for(&FixSource::Text(s));
    let f = t("the last backup is 3 days old — run `homelab backup media`").unwrap();
    assert_eq!((f.action.as_str(), f.stack.as_str()), ("backup", "media"));
    let f = t("homelab checks answer e576228c ok").unwrap();
    assert_eq!(
        (f.action.as_str(), f.stack.as_str()),
        ("answer-check", "_host")
    );
    assert_eq!(f.args.get("check").map(String::as_str), Some("e576228c"));
    assert_eq!(f.args.get("verdict").map(String::as_str), Some("ok"));
    let f = t("check first with `homelab doctor`, then `homelab patch`").unwrap();
    assert_eq!(f.action, "patch");
    assert!(t("raise memory_mb and run `homelab resize <stack>`").is_none());
    assert!(t("nothing to run here").is_none());
    assert!(t("`homelab deploy Media!`").is_none(), "not a stack name");
    let f = t("`homelab release-update`").unwrap();
    assert_eq!(f.action, "update-host");
    // An alert names its container; HostDown offers a deploy of its stack.
    let f = fix_for(&FixSource::Alert {
        alertname: "HostDown",
        host: Some("120-app-admin"),
        remedy: "",
    })
    .unwrap();
    assert_eq!((f.action.as_str(), f.stack.as_str()), ("deploy", "admin"));
    assert!(fix_for(&FixSource::Alert {
        alertname: "TraefikServerErrors",
        host: None,
        remedy: "read its logs tab",
    })
    .is_none());
}

/// Every fix names an action the dashboard has, with a label to show.
#[test]
fn fix_every_action_it_can_name_exists() {
    let texts = [
        "`homelab deploy a`",
        "`homelab backup a`",
        "`homelab update a`",
        "`homelab enable a`",
        "`homelab backup-native a`",
        "`homelab update-native a`",
        "`homelab release-update-native a`",
        "`homelab rollback-native a`",
        "`homelab patch`",
        "`homelab backup-host-meta`",
        "`homelab backup-devices`",
        "`homelab zfs-replicate`",
        "`homelab release-update`",
        "`homelab checks answer abc nok`",
    ];
    for t in texts {
        let f = fix_for(&FixSource::Text(t)).unwrap_or_else(|| panic!("{t}"));
        let k = ActionKind::from_slug(&f.action).unwrap_or_else(|| panic!("{}", f.action));
        assert_eq!(f.label, k.label());
    }
}

fn host_notice(seq: u64, urgent: bool) -> HostNotice {
    HostNotice {
        seq,
        at: 1_000,
        since: 900,
        op: "deploy-media".into(),
        label: "deploy".into(),
        ok: false,
        deferred: false,
        stack: Some("media".into()),
        title: "deploy media failed".into(),
        what: "compose: pull failed".into(),
        consequence: "It may run the old version.".into(),
        remedy: "Run it again: `homelab deploy media`.".into(),
        page: "/app/stacks/media".into(),
        urgent,
        routed: "urgent: a deploy failed".into(),
        push: "sent".into(),
        incident: None,
        req: Some(77),
        by: Some("admin".into()),
        findings: Vec::new(),
    }
}

#[test]
fn host_notices_land_in_the_centre_with_their_detail_and_fix() {
    let (d, push) = host_draft(&host_notice(5, true));
    assert_eq!(d.kind, Kind::HostEvent);
    assert_eq!(d.detail.level, Level::Critical);
    assert_eq!(d.detail.since, Some(900));
    assert_eq!(d.detail.link.as_deref(), Some("/app/stacks/media"));
    assert_eq!(d.detail.fixes[0].action, "deploy");
    assert_eq!(
        push,
        PushOutcome::BySender {
            who: "the host".into()
        }
    );
    let mut n = host_notice(6, false);
    n.push = "centre only".into();
    n.ok = true;
    let (d, push) = host_draft(&n);
    assert_eq!(d.detail.level, Level::Ok);
    assert!(d.detail.fixes.is_empty());
    assert!(matches!(push, PushOutcome::Skipped { .. }));
    // The nightly check: one fix per finding the dashboard can run.
    let mut fc = host_notice(7, true);
    fc.op = "fleet-check".into();
    fc.label = "nightly".into();
    fc.stack = None;
    fc.findings = vec![
        Finding {
            severity: Severity::Broken,
            subject: "media".into(),
            what: "no backup".into(),
            remedy: "`homelab backup media`".into(),
        },
        Finding {
            severity: Severity::Drift,
            subject: "home".into(),
            what: "drift".into(),
            remedy: "edit the file by hand".into(),
        },
    ];
    let (d, _) = host_draft(&fc);
    assert_eq!(d.kind, Kind::FleetCheck);
    assert_eq!(d.detail.fixes.len(), 1);
    assert_eq!(d.detail.fixes[0].stack, "media");
}

#[test]
fn host_notices_merge_into_the_dashboards_own_notice_of_that_job() {
    let mut f = NotifyFile::default();
    let own = f.add(
        {
            let mut d = Draft::new(
                Kind::ActionFailed,
                "deploy-media",
                "Deploy media: failed",
                "x",
            );
            d.job = Some(3);
            d
        },
        950,
        PushOutcome::Skipped { why: "x".into() },
    );
    // The job that sent request 77 is job 3: the host's words fill that notice.
    let merged = f
        .import_host(&host_notice(5, true), Some(3), 1_001)
        .unwrap();
    assert_eq!(merged.id, own.id);
    assert_eq!(f.notices.len(), 1);
    assert_eq!(
        merged.remedy.as_deref(),
        Some("Run it again: `homelab deploy media`.")
    );
    assert_eq!(merged.job, Some(3));
    assert_eq!(merged.fixes.len(), 1);
    assert_eq!(f.host_cursor, 5);
    // Without a job of its own it is a notice of its own.
    let n = f.import_host(&host_notice(6, false), None, 1_002).unwrap();
    assert_eq!(f.notices.len(), 2);
    assert_eq!(n.kind, Kind::HostEvent);
    assert_eq!(f.host_cursor, 6);
}

fn am(status: &str, name: &str, severity: &str, fp: &str) -> serde_json::Value {
    serde_json::json!({
        "version": "4",
        "status": status,
        "alerts": [{
            "status": status,
            "labels": { "alertname": name, "severity": severity, "host": "120-app-admin" },
            "annotations": {
                "summary": format!("{name} summary"),
                "description": "the detail",
                "consequence": "what it costs",
                "remedy": "what to do",
                "click_url": "https://admin.kp-soft.dev/app/host"
            },
            "startsAt": "2026-09-30T07:00:00Z",
            "endsAt": "0001-01-01T00:00:00Z",
            "fingerprint": fp
        }]
    })
}

#[test]
fn alerts_become_notices_firing_and_resolved() {
    let firing = alert_drafts(&am("firing", "HostDown", "critical", "fp1"));
    assert_eq!(firing.len(), 1);
    let a = &firing[0];
    assert!(a.firing);
    assert_eq!(a.draft.kind, Kind::Alert);
    assert_eq!(a.draft.title, "HostDown summary");
    assert_eq!(a.draft.detail.level, Level::Critical);
    assert_eq!(a.draft.detail.since, Some(1_790_751_600));
    assert_eq!(a.draft.detail.consequence.as_deref(), Some("what it costs"));
    assert_eq!(a.draft.detail.remedy.as_deref(), Some("what to do"));
    assert_eq!(a.draft.detail.link.as_deref(), Some("/app/host"));
    assert_eq!(a.draft.detail.fixes[0].action, "deploy");
    assert_eq!(
        a.push,
        PushOutcome::BySender {
            who: "Alertmanager".into()
        }
    );
    let warn = alert_drafts(&am("firing", "KyuBacklogGrowing", "warning", "fp2"));
    assert_eq!(warn[0].draft.detail.level, Level::Warning);
    assert!(matches!(warn[0].push, PushOutcome::Skipped { .. }));

    let mut f = NotifyFile::default();
    let first = f.add_alert(firing[0].clone(), 1_000).unwrap();
    assert!(!first.read);
    // Alertmanager repeats a firing alert every 12 h: one notice, not two.
    assert!(f
        .add_alert(
            alert_drafts(&am("firing", "HostDown", "critical", "fp1"))[0].clone(),
            2_000
        )
        .is_none());
    // Resolved: stored as read, and the firing one no longer waits.
    let resolved = alert_drafts(&am("resolved", "HostDown", "critical", "fp1"));
    assert_eq!(resolved[0].draft.kind, Kind::AlertResolved);
    let r = f.add_alert(resolved[0].clone(), 3_000).unwrap();
    assert!(r.read && r.fixes.is_empty());
    assert_eq!(f.unread(), 0);
    // Firing again after it resolved is news again.
    assert!(f
        .add_alert(
            alert_drafts(&am("firing", "HostDown", "critical", "fp1"))[0].clone(),
            4_000
        )
        .is_some());
    assert!(alert_drafts(&serde_json::json!({"alerts": "nope"})).is_empty());
}

#[test]
fn daily_digest_at_nine_only_when_something_waits_worst_first() {
    let s = Settings::default();
    assert_eq!(s.digest_at.as_deref(), Some("09:00"));
    // 2026-09-30 is summer time in Brussels: 09:00 local is 07:00 UTC.
    let nine = 1_790_751_600;
    assert_eq!(digest_due(&s, None, nine - 60), None, "not yet");
    assert_eq!(digest_due(&s, None, nine).as_deref(), Some("2026-09-30"));
    let sent = DigestRecord {
        day: "2026-09-30".into(),
        at: nine,
        count: 2,
        push: PushOutcome::Sent,
    };
    assert_eq!(digest_due(&s, Some(&sent), nine + 600), None, "once a day");
    assert_eq!(
        digest_due(&s, None, nine + 5 * 3600),
        None,
        "too late that day: skipped, not sent in the evening"
    );
    let off = Settings {
        digest_at: None,
        ..Default::default()
    };
    assert_eq!(digest_due(&off, None, nine), None);

    let mut f = NotifyFile::default();
    assert!(digest(&f.notices, &[]).is_none(), "all clear: nothing");
    let mut ok = Draft::new(Kind::HostEvent, "deploy-a", "deploy a: done", "");
    ok.detail.level = Level::Ok;
    f.add(ok, 1, PushOutcome::Skipped { why: "x".into() });
    let mut crit = Draft::new(Kind::HostEvent, "backup-b", "backup b failed", "");
    crit.detail.level = Level::Critical;
    f.add(crit, 2, PushOutcome::Sent);
    let mut done = Draft::new(Kind::Alert, "x", "read already", "");
    done.detail.level = Level::Critical;
    let n = f.add(done, 3, PushOutcome::Sent);
    f.mark(Some(&[n.id]), true);
    let today = today_lines(&Today {
        items: vec![Item {
            level: TodayLevel::Attention,
            source: "check".into(),
            what: "a manual check is open".into(),
            remedy: "homelab checks".into(),
        }],
        unread: Vec::new(),
    });
    let d = digest(&f.notices, &today).unwrap();
    assert_eq!(d.count, 3, "two unread notices and one Today item");
    assert_eq!(d.lines[0], "backup b failed", "worst first");
    assert_eq!(d.lines[1], "a manual check is open");
    assert!(d.title.contains('3'), "{}", d.title);
    assert!(!d.lines.iter().any(|l| l.contains("read already")));
}

#[test]
fn settings_the_digest_time_is_checked_and_old_files_still_load() {
    let bad = Settings {
        digest_at: Some("25:00".into()),
        ..Default::default()
    };
    assert!(bad.validate().is_err());
    // A file from before the decision still reads (long_action_s is gone).
    let old: Settings =
        serde_json::from_str(r#"{"push":true,"muted_stacks":[],"long_action_s":120}"#).unwrap();
    assert_eq!(old.digest_at.as_deref(), Some("09:00"));
    let json = serde_json::to_value(&old).unwrap();
    assert!(json.get("long_action_s").is_none());
}
