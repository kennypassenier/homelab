//! feat-stacks-8 (arch-schedule): schedule math in Europe/Brussels, safe
//! across the clock changes, and the skip-never-catch-up rule.

use homelab_admin::core::actions::ActionArgs;
use homelab_admin::core::schedule::{
    check_input, civil_from_days, days_from_civil, local_label, next_run, offset_at, resolve_local,
    summer_time, tick, to_local, weekday, Resolved, Schedule, ScheduleFile, ScheduleInput, When,
};

const JAN1_2026: i64 = 20_454;
const DAY: i64 = 86_400;

fn utc(days: i64, h: i64, m: i64) -> i64 {
    days * DAY + h * 3_600 + m * 60
}

#[test]
fn feat_stacks_8_calendar_arithmetic_round_trips() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2026, 1, 1), JAN1_2026);
    assert_eq!(civil_from_days(JAN1_2026), (2026, 1, 1));
    assert_eq!(
        days_from_civil(2024, 2, 29),
        days_from_civil(2024, 3, 1) - 1
    );
    for d in (-800..30_000).step_by(37) {
        let (y, m, dd) = civil_from_days(d);
        assert_eq!(days_from_civil(y, m, dd), d);
    }
    assert_eq!(weekday(0), 3, "1970-01-01 was a Thursday");
    assert_eq!(weekday(JAN1_2026), 3, "2026-01-01 is a Thursday");
    assert_eq!(
        weekday(days_from_civil(2026, 9, 28)),
        0,
        "2026-09-28 is a Monday"
    );
}

#[test]
fn feat_stacks_8_summer_time_follows_the_eu_rule() {
    let (start, end) = summer_time(2026);
    assert_eq!(start, utc(days_from_civil(2026, 3, 29), 1, 0));
    assert_eq!(end, utc(days_from_civil(2026, 10, 25), 1, 0));
    assert_eq!(offset_at(start - 1), 3_600);
    assert_eq!(offset_at(start), 7_200);
    assert_eq!(offset_at(end - 1), 7_200);
    assert_eq!(offset_at(end), 3_600);
    // 2027: 28 March and 31 October.
    let (s27, e27) = summer_time(2027);
    assert_eq!(civil_from_days(s27 / DAY), (2027, 3, 28));
    assert_eq!(civil_from_days(e27 / DAY), (2027, 10, 31));
}

#[test]
fn feat_stacks_8_a_local_time_resolves_exactly_twice_or_past_the_gap() {
    let july = days_from_civil(2026, 7, 1);
    assert_eq!(
        resolve_local(2026, 7, 1, 3, 30),
        Resolved::Exact(utc(july, 1, 30))
    );
    let jan = days_from_civil(2026, 1, 15);
    assert_eq!(
        resolve_local(2026, 1, 15, 3, 30),
        Resolved::Exact(utc(jan, 2, 30))
    );
    // Spring: 02:30 does not exist; the run is at 03:00 summer time.
    let spring = days_from_civil(2026, 3, 29);
    let r = resolve_local(2026, 3, 29, 2, 30);
    assert_eq!(r, Resolved::Skipped(utc(spring, 1, 0)));
    assert_eq!(to_local(r.instant()), (2026, 3, 29, 3, 0));
    // Autumn: 02:30 happens twice; the first counts.
    let autumn = days_from_civil(2026, 10, 25);
    let r = resolve_local(2026, 10, 25, 2, 30);
    assert_eq!(r, Resolved::Twice(utc(autumn, 0, 30), utc(autumn, 1, 30)));
    assert_eq!(r.instant(), utc(autumn, 0, 30));
    assert_eq!(to_local(utc(autumn, 1, 30)), (2026, 10, 25, 2, 30));
}

#[test]
fn feat_stacks_8_a_daily_slot_runs_once_a_day_across_both_clock_changes() {
    let at = When::Day { at: "02:30".into() };
    let spring = days_from_civil(2026, 3, 28);
    let slots = at.slots_between(utc(spring, 0, 0), utc(spring + 3, 0, 0), 10);
    let local: Vec<_> = slots.iter().map(|s| to_local(*s)).collect();
    assert_eq!(
        local,
        vec![
            (2026, 3, 28, 2, 30),
            (2026, 3, 29, 3, 0),
            (2026, 3, 30, 2, 30)
        ]
    );
    let autumn = days_from_civil(2026, 10, 24);
    let slots = at.slots_between(utc(autumn, 0, 0), utc(autumn + 3, 0, 0), 10);
    assert_eq!(slots.len(), 3, "the repeated hour runs once: {slots:?}");
    assert_eq!(slots[1], utc(autumn + 1, 0, 30));
    assert_eq!(
        slots[2] - slots[1],
        25 * 3_600,
        "the autumn day is 25 hours long"
    );
}

#[test]
fn feat_stacks_8_weekly_and_once() {
    // Monday and Thursday at 04:00; from Thursday 2026-01-01 05:00 local.
    let w = When::Week {
        days: vec![0, 3],
        at: "04:00".into(),
    };
    let next = w.next_after(utc(JAN1_2026, 4, 0)).unwrap();
    assert_eq!(to_local(next), (2026, 1, 5, 4, 0));
    let next = w.next_after(next).unwrap();
    assert_eq!(to_local(next), (2026, 1, 8, 4, 0));
    let once = When::Once {
        date: "2026-10-01".into(),
        at: "12:00".into(),
    };
    let t = once.next_after(0).unwrap();
    assert_eq!(to_local(t), (2026, 10, 1, 12, 0));
    assert_eq!(
        once.next_after(t),
        None,
        "a one-off in the past has no next"
    );
}

#[test]
fn feat_stacks_8_times_and_dates_are_checked() {
    for bad in ["24:00", "3:30", "12:60", "noon"] {
        assert!(When::Day { at: bad.into() }.validate().is_err(), "{bad}");
    }
    assert!(When::Week {
        days: vec![7],
        at: "01:00".into()
    }
    .validate()
    .is_err());
    assert!(When::Week {
        days: vec![],
        at: "01:00".into()
    }
    .validate()
    .is_err());
    assert!(When::Once {
        date: "2026-02-30".into(),
        at: "01:00".into()
    }
    .validate()
    .is_err());
    let w: When = serde_json::from_str(r#"{"every":"week","days":[5,6],"at":"03:15"}"#).unwrap();
    assert!(w.validate().is_ok());
}

fn daily(at: &str, handled_until: i64) -> Schedule {
    Schedule {
        id: "s1".into(),
        stack: "media".into(),
        action: "backup".into(),
        args: ActionArgs::default(),
        when: When::Day { at: at.into() },
        enabled: true,
        note: String::new(),
        created_at: handled_until,
        handled_until,
        last_run: None,
    }
}

#[test]
fn arch_schedule_a_due_slot_runs_a_late_one_is_skipped_never_caught_up() {
    let d = days_from_civil(2026, 9, 28);
    let slot = resolve_local(2026, 9, 28, 3, 30).instant();
    let s = daily("03:30", utc(d - 1, 12, 0));
    // A tick shortly after the slot runs it.
    let p = tick(&s, slot + 20, 300, false);
    assert_eq!((p.run, p.missed.clone()), (Some(slot), vec![]));
    assert_eq!(p.handled_until, slot + 20);
    // The dashboard was down: the slot is skipped, not run late.
    let p = tick(&s, slot + 3_600, 300, false);
    assert_eq!((p.run, p.missed.clone()), (None, vec![slot]));
    // Down for two days, back just after the third slot: only that one runs.
    let s2 = daily("03:30", utc(d - 3, 12, 0));
    let p = tick(&s2, slot + 10, 300, false);
    assert_eq!(p.run, Some(slot));
    assert_eq!(p.missed.len(), 2);
    // Its previous run still going: skipped too.
    let p = tick(&s, slot + 20, 300, true);
    assert_eq!((p.run, p.missed.clone()), (None, vec![slot]));
    // Nothing due: nothing happens, and the mark moves on.
    let p = tick(&s, slot - 60, 300, false);
    assert_eq!((p.run, p.missed.len()), (None, 0));
    // Switched off: nothing runs, nothing is "missed".
    let mut off = s.clone();
    off.enabled = false;
    let p = tick(&off, slot + 20, 300, false);
    assert_eq!(
        (p.run, p.missed.len(), p.handled_until),
        (None, 0, slot + 20)
    );
    assert_eq!(next_run(&off, slot), None);
    assert_eq!(next_run(&s, slot - 60), Some(slot));
}

#[test]
fn arch_schedule_only_what_needs_no_typed_name_can_be_planned() {
    let input = |action: &str, args: ActionArgs| ScheduleInput {
        stack: "media".into(),
        action: action.into(),
        args,
        when: When::Day { at: "03:30".into() },
        enabled: true,
        note: String::new(),
    };
    assert!(check_input(&input("backup", ActionArgs::default())).is_ok());
    assert!(check_input(&input("update", ActionArgs::default())).is_ok());
    for (a, confirm) in [
        ("restore", true),
        ("destroy", true),
        ("wipe", true),
        ("forget", true),
    ] {
        let args = ActionArgs {
            confirm: confirm.then(|| "media".to_string()),
            ..Default::default()
        };
        let r = check_input(&input(a, args)).unwrap_err();
        assert!(r.why.contains("cannot be scheduled"), "{a}: {r}");
    }
    let mut bad_time = input("backup", ActionArgs::default());
    bad_time.when = When::Day { at: "25:00".into() };
    assert!(check_input(&bad_time).is_err());
    let mut host = input("patch", ActionArgs::default());
    host.stack = "_host".into();
    assert!(
        check_input(&host).is_ok(),
        "a host-wide action can be planned"
    );
}

#[test]
fn arch_state_a_schedule_file_of_another_version_or_zone_is_refused() {
    let f = ScheduleFile::default();
    assert!(f.check().is_ok());
    assert_eq!(f.zone, "Europe/Brussels");
    let mut v2 = f.clone();
    v2.schema_version = 2;
    assert!(v2.check().unwrap_err().contains("schema_version"));
    let mut utc_zone = f;
    utc_zone.zone = "UTC".into();
    assert!(utc_zone.check().is_err());
}

#[test]
fn feat_stacks_8_labels_are_local_time() {
    let summer = resolve_local(2026, 7, 1, 3, 30).instant();
    assert_eq!(local_label(summer), "2026-07-01 03:30");
}
