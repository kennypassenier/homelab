//! feat-overview-5, feat-ops-8, feat-ops-9: which notice pushes, the list
//! with read and unread, incidents, the snooze; and the act settings.

use homelab_admin::core::actions_config::from_env;
use homelab_admin::core::notify::{
    incident_draft, push_payload, route, set_stack_muted, unread_by_stack, Draft, Kind, NotifyFile,
    PushOutcome, Settings, KEEP, SNOOZE_MAX_S,
};

fn draft(kind: Kind, stack: Option<&str>, ran_s: Option<u64>) -> Draft {
    let mut d = Draft::new(kind, "deploy-media", "t", "b");
    d.stack = stack.map(str::to_string);
    d.job = Some(1);
    d.ran_s = ran_s;
    // A missed schedule is urgent when its action is (decision
    // notify-routing, 2026-09-30); these tests use a missed deploy.
    d.detail.label = Some("deploy".into());
    d
}

/// Decision notify-routing (2026-09-30) replaced feat-ops-8's "a long
/// action pushes": only urgent work is pushed, and a failed operation by
/// the host itself, never twice.
#[test]
fn feat_ops_8_only_urgent_work_pushes_from_the_dashboard() {
    let s = Settings::default();
    for (kind, ran) in [
        (Kind::ActionDone, Some(600)),
        (Kind::ActionDone, Some(30)),
        (Kind::ActionFailed, Some(3)),
        (Kind::Incident, None),
    ] {
        assert!(
            route(&s, &draft(kind, Some("media"), ran), 0).push.is_err(),
            "{kind:?}"
        );
    }
    assert!(
        route(&s, &draft(Kind::ScheduleMissed, Some("media"), None), 0)
            .push
            .is_ok()
    );
    // The dashboard's own results pop up on the open pages unless snoozed.
    assert!(route(&s, &draft(Kind::ActionDone, None, Some(1)), 0).pop_up);
}

#[test]
fn feat_ops_9_switches_per_stack_for_all_and_the_snooze() {
    let mut s = Settings::default();
    set_stack_muted(&mut s, "media", true);
    set_stack_muted(&mut s, "media", true);
    assert_eq!(s.muted_stacks, vec!["media"]);
    let failed = draft(Kind::ScheduleMissed, Some("media"), Some(3));
    assert!(route(&s, &failed, 0).push.unwrap_err().contains("stack"));
    assert!(
        route(&s, &draft(Kind::ScheduleMissed, Some("home"), None), 0)
            .push
            .is_ok()
    );
    set_stack_muted(&mut s, "media", false);
    assert!(route(&s, &failed, 0).push.is_ok());
    s.push = false;
    assert!(route(&s, &failed, 0).push.unwrap_err().contains("off"));
    s.push = true;
    let mut f = NotifyFile {
        settings: s,
        ..Default::default()
    };
    assert_eq!(f.snooze(1_000, 3_600, SNOOZE_MAX_S).unwrap(), Some(4_600));
    let r = route(&f.settings, &failed, 2_000);
    assert!(r.push.unwrap_err().contains("snoozed") && !r.pop_up);
    assert!(
        route(&f.settings, &failed, 4_600).push.is_ok(),
        "the snooze ends"
    );
    assert_eq!(f.snooze(1_000, 0, SNOOZE_MAX_S).unwrap(), None);
    assert!(f.snooze(0, SNOOZE_MAX_S + 1, SNOOZE_MAX_S).is_err());
    assert!(f.snooze(0, -1, SNOOZE_MAX_S).is_err());
}

#[test]
fn feat_overview_5_the_list_keeps_read_and_unread_and_its_newest() {
    let mut f = NotifyFile::default();
    let a = f.add(
        draft(Kind::ActionDone, Some("media"), None),
        10,
        PushOutcome::Sent,
        KEEP,
    );
    let b = f.add(
        draft(Kind::ActionFailed, Some("home"), None),
        11,
        PushOutcome::Skipped { why: "x".into() },
        KEEP,
    );
    assert_eq!((a.id, b.id), (1, 2));
    // Owner decision 2026-09-30 (item 3): the table's push column is a
    // handful of fixed words, never the raw routing reason.
    assert_eq!(a.push_short, "Pushed");
    assert_eq!(b.push_short, "No push · not urgent");
    assert_eq!(f.unread(), 2);
    assert_eq!(f.mark(Some(&[1]), true), 1);
    assert_eq!(f.mark(Some(&[1]), true), 0, "already read");
    assert_eq!(unread_by_stack(&f).get("home"), Some(&1));
    assert_eq!(f.mark(None, true), 1);
    assert_eq!(f.unread(), 0);
    assert_eq!(f.mark(None, false), 2);
    for i in 0..(KEEP + 5) {
        f.add(
            draft(Kind::ActionDone, None, None),
            100 + i as i64,
            PushOutcome::Sent,
            KEEP,
        );
    }
    assert_eq!(f.notices.len(), KEEP);
    assert_eq!(f.notices.last().unwrap().id, (KEEP + 7) as u64);
    let json = serde_json::to_value(&f.notices[0]).unwrap();
    for k in ["id", "at", "kind", "title", "body", "read", "push"] {
        assert!(json.get(k).is_some(), "{k} in {json}");
    }
}

#[test]
fn feat_overview_5_incidents_already_there_at_the_start_are_not_news() {
    let mut f = NotifyFile::default();
    assert!(f
        .new_incidents(&["1800000000-deploy-media".into()])
        .is_empty());
    let fresh = f.new_incidents(&[
        "1800000000-deploy-media".into(),
        "1800000500-backup-home".into(),
    ]);
    assert_eq!(fresh, vec!["1800000500-backup-home"]);
    assert!(f
        .new_incidents(&["1800000500-backup-home".into()])
        .is_empty());
    let d = incident_draft("1800000500-backup-home");
    assert_eq!(d.kind, Kind::Incident);
    assert!(d.title.contains("backup-home") && d.body.contains("incidents show"));
}

#[test]
fn feat_ops_8_the_push_is_the_hosts_payload_from_homelab_admin() {
    let p: serde_json::Value = serde_json::from_str(&push_payload(
        &draft(Kind::ActionFailed, Some("media"), Some(9)),
        "3.62.2",
        "https://admin.kp-soft.dev",
    ))
    .unwrap();
    assert_eq!(p["source"], "homelab-admin");
    assert_eq!(p["op"], "deploy-media");
    assert_eq!(p["label"], "action-failed");
    assert_eq!(p["ok"], false);
    // Decision notify-detail: the short version, title and what to do.
    assert_eq!(p["error"], "t — b");
    assert_eq!(
        p["click_url"],
        "https://admin.kp-soft.dev/app/notifications"
    );
    let ok: serde_json::Value = serde_json::from_str(&push_payload(
        &draft(Kind::ActionDone, None, Some(900)),
        "3.62.2",
        "https://admin.kp-soft.dev",
    ))
    .unwrap();
    assert_eq!(
        (ok["ok"].as_bool(), ok["error"].is_null()),
        (Some(true), true)
    );
}

#[test]
fn feat_ops_9_settings_are_checked() {
    let s = Settings {
        digest_at: Some("9 o'clock".into()),
        ..Default::default()
    };
    assert!(s.validate().is_err());
    let s = Settings {
        muted_stacks: vec!["Media!".into()],
        ..Default::default()
    };
    assert!(s.validate().is_err());
    assert!(serde_json::from_str::<Settings>(r#"{"push":true,"sound":1}"#).is_err());
}

#[test]
fn arch_state_a_notification_file_of_another_version_is_refused() {
    let mut f = NotifyFile::default();
    assert!(f.check().is_ok());
    f.schema_version = 9;
    assert!(f.check().is_err());
}

#[test]
fn arch_config_act_settings_from_the_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    let c = from_env(&env(&[])).unwrap();
    assert_eq!(c.data_dir.to_str(), Some("/appdata/admin/admin-config"));
    assert_eq!(c.repo.to_str(), Some("/appdata/admin/admin-config/repo"));
    assert_eq!(c.notify_url, None);
    assert_eq!(
        (c.schedule_tick_s, c.schedule_grace_s, c.incidents_poll_s),
        (20, 300, 300)
    );
    // Decision notify-routing: the host's notices every minute, the hook
    // closed until its token is set, pushes linking to the public address.
    assert_eq!(c.host_notices_poll_s, 60);
    assert_eq!(c.alerts_token, None);
    // app-knowledge (2026-09-30): no address is known in code; unset, a
    // push carries no link.
    assert_eq!(c.public_url, "");
    let c = from_env(&env(&[
        ("HOMELAB_ADMIN_PUBLIC_URL", "https://dash.example/"),
        ("HOMELAB_ADMIN_ALERTS_TOKEN", " tok "),
    ]))
    .unwrap();
    assert_eq!(c.public_url, "https://dash.example");
    assert_eq!(c.alerts_token.as_deref(), Some("tok"));
    let e = from_env(&env(&[
        ("HOMELAB_ADMIN_PUBLIC_URL", "admin.kp-soft.dev"),
        ("HOMELAB_ADMIN_ALERTS_TOKEN", "alerts-secret"),
    ]))
    .unwrap_err();
    assert!(
        e.contains("PUBLIC_URL") && !e.contains("alerts-secret"),
        "{e}"
    );
    let c = from_env(&env(&[("HOMELAB_ADMIN_STATE_DIR", "/tmp/a")])).unwrap();
    assert_eq!(c.schedules_file().to_str(), Some("/tmp/a/schedules.json"));
    let c = from_env(&env(&[
        ("HOMELAB_ADMIN_DATA_DIR", "/d"),
        ("HOMELAB_ADMIN_STATE_DIR", "/s"),
        ("HOMELAB_ADMIN_REPO", "/r"),
        (
            "HOMELAB_ADMIN_NOTIFY_URL",
            "http://10.10.10.9:8080/publish/notify.kenny",
        ),
    ]))
    .unwrap();
    assert_eq!(
        (c.data_dir.to_str(), c.repo.to_str()),
        (Some("/d"), Some("/r"))
    );
    let e = from_env(&env(&[
        ("HOMELAB_ADMIN_SCHEDULE_TICK_S", "600"),
        ("HOMELAB_ADMIN_NOTIFY_TOKEN", "secret-value"),
        ("HOMELAB_ADMIN_INCIDENTS_POLL_S", "x"),
    ]))
    .unwrap_err();
    assert!(e.contains("GRACE_S") && e.contains("NOTIFY_URL") && e.contains("INCIDENTS_POLL_S"));
    assert!(
        !e.contains("secret-value"),
        "a secret never lands in an error: {e}"
    );
}
