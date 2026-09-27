//! fix-68 · `homelab today`: doctor, the fleet check (with its manual
//! checks) and the open incident bundles merged into one list and one verdict.

use homelab_core::doctor::{Check, Health};
use homelab_core::ops::fleetcheck::{Finding, Severity};
use homelab_core::ops::today::{assemble, open_incidents, render, Level};
use homelab_core::state::{HostState, StackState};

const NOW: u64 = 1_790_530_000;
const HOUR: u64 = 3600;

fn stack(applied_at: u64, last_backup: u64) -> StackState {
    StackState {
        vmid: 109,
        hostname: "109-app-kyu".into(),
        apps: Vec::new(),
        applied_at,
        last_backup,
        applied_hash: String::new(),
        manifest: None,
        enabled: true,
        natives: Vec::new(),
        incomplete_step: None,
        route_file: None,
        extra_route_files: Vec::new(),
    }
}

fn state() -> HostState {
    let mut st = HostState::default();
    st.stacks
        .insert("kyu".into(), stack(NOW - 2 * HOUR, NOW - 20 * HOUR));
    st.stacks
        .insert("kp-soft".into(), stack(NOW - 50 * HOUR, NOW - 20 * HOUR));
    st.stacks.insert("soft".into(), stack(NOW, NOW));
    st
}

fn finding(severity: Severity, subject: &str) -> Finding {
    Finding {
        severity,
        subject: subject.into(),
        what: format!("{} is off", subject),
        remedy: format!("fix {}", subject),
    }
}

fn check(health: Health, name: &str) -> Check {
    Check {
        name: name.into(),
        health,
        detail: "detail".into(),
        remedy: None,
    }
}

/// The finding's own scenario: the one broken item `check` listed seventh of
/// nine goes to the top, the deliberate (noted) items and the healthy doctor
/// lines are left out, and the verdict counts what is left.
///
/// covers: fix-68
#[test]
fn fix_68_one_list_most_severe_first_ending_in_one_verdict() {
    let doctor = vec![
        check(Health::Ok, "host disk"),
        check(Health::Warn, "offsite"),
    ];
    let findings = vec![
        finding(Severity::Noted, "registry"),
        finding(Severity::Drift, "almanac"),
        finding(Severity::Broken, "kp-soft/kp-soft"),
    ];
    let today = assemble(&doctor, &findings, &[], &state(), NOW);
    let levels: Vec<Level> = today.items.iter().map(|i| i.level).collect();
    assert_eq!(
        levels,
        vec![Level::Broken, Level::Attention, Level::Attention],
        "{:?}",
        today.items
    );
    assert!(today.items[0].what.starts_with("kp-soft/kp-soft"));
    assert!(
        !today.items.iter().any(|i| i.what.starts_with("registry")),
        "a deliberate, noted item needs nobody"
    );
    assert_eq!(today.verdict(), "3 things need you");
    let text = render(&today);
    assert!(text.ends_with("3 things need you"), "{}", text);
    assert!(
        text.contains("→ fix kp-soft/kp-soft"),
        "one remedy per line: {}",
        text
    );

    let quiet = assemble(&[check(Health::Ok, "host disk")], &[], &[], &state(), NOW);
    assert_eq!(quiet.verdict(), "Nothing needs you");
    assert!(!quiet.needs_you());
}

/// An incident bundle stays on the list only while nothing on its stack has
/// succeeded since. 91 bundles lay on pve on 2026-09-27, most of them long
/// dealt with; counting all of them would make "N things need you" a number
/// nobody can bring to zero.
///
/// covers: fix-68
#[test]
fn fix_68_an_incident_is_open_until_its_stack_succeeds_again() {
    let names: Vec<String> = [
        // kyu deployed two hours ago: this older failure is dealt with.
        format!("{}-update-kyu", NOW - 5 * HOUR),
        // ...and this one came after that deploy: still open.
        format!("{}-deploy-kyu", NOW - HOUR),
        // kp-soft, not the stack called `soft` that succeeded just now.
        format!("{}-update-kp-soft", NOW - 10 * HOUR),
        // A stack the host does not track: open for a day.
        format!("{}-deploy-drill", NOW - 3 * HOUR),
        format!("{}-deploy-drill", NOW - 30 * HOUR),
        "not-a-bundle".to_string(),
    ]
    .to_vec();
    let open = open_incidents(&names, &state(), NOW);
    assert_eq!(
        open,
        vec![
            format!("{}-deploy-kyu", NOW - HOUR),
            format!("{}-update-kp-soft", NOW - 10 * HOUR),
            format!("{}-deploy-drill", NOW - 3 * HOUR),
        ]
    );
    let today = assemble(&[], &[], &names, &state(), NOW);
    assert_eq!(today.verdict(), "3 things need you");
    assert!(today.items.iter().all(|i| i.level == Level::Broken));
}
