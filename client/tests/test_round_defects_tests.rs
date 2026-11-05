//! Four defects from the test round: a scoped `check` compared one stack
//! against the whole fleet, `snapshots` printed epoch seconds, and `checks`
//! and `tool status` printed an empty line or raw JSON after the tick.

use homelab_client::check_scope::check_scope;
use homelab_client::output::readable_reply;
use homelab_client::snapshots::render_at;
use homelab_core::ops::backup::{RepoStatus, SnapRun};
use homelab_proto::Command;

/// A fresh scratch directory, unique per test name.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("homelab-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_single_stack_directory_scopes_the_check_to_that_stack() {
    let t = scratch("scope_single");
    touch(&t.join("stacks/kyu/lxc-compose.yml"), "vmid: 109\n");
    touch(&t.join("stacks/kyu/kyu/service.yml"), "vmid: 109\n");
    touch(&t.join("stacks/inbox/lxc-compose.yml"), "vmid: 118\n");
    let arg = t.join("stacks/kyu").display().to_string();
    let scope = check_scope(&arg);
    assert_eq!(scope.only.as_deref(), Some("kyu"));
    assert_eq!(scope.base, t.join("stacks").display().to_string());
    let files = vec![
        (format!("{}/inbox", scope.base), 118),
        (format!("{}/kyu", scope.base), 109),
    ];
    let digests = scope.digest_files(&files);
    assert_eq!(digests.len(), 1);
    assert!(digests[0].0.ends_with("/kyu"));
}

#[test]
fn a_native_only_stack_directory_is_scoped_too() {
    let t = scratch("scope_native");
    touch(&t.join("stacks/admin/service.yml"), "vmid: 120\n");
    touch(&t.join("stacks/admin/admin/admin.service"), "x");
    let scope = check_scope(&t.join("stacks/admin").display().to_string());
    assert_eq!(scope.only.as_deref(), Some("admin"));
}

#[test]
fn a_stacks_directory_is_not_scoped() {
    let t = scratch("scope_dir");
    touch(&t.join("stacks/kyu/lxc-compose.yml"), "vmid: 109\n");
    let arg = t.join("stacks").display().to_string();
    let scope = check_scope(&arg);
    assert_eq!(scope.only, None);
    assert_eq!(scope.base, arg);
}

fn snap(time: u64) -> SnapRun {
    SnapRun {
        id: "abcdef0123".into(),
        short_id: "abcdef01".into(),
        time,
        run: None,
        size_bytes: Some(1_572_864),
        file_count: None,
        trigger: Some("nightly".into()),
    }
}

#[test]
fn snapshots_print_a_local_time_the_age_the_size_and_the_trigger() {
    let now = 1_800_000_000;
    let at = now - 3 * 3600 - 5 * 60;
    let repo = RepoStatus {
        owner: "kyu".into(),
        newest_snapshot: Some(snap(at)),
        snapshot_count: 1,
        snapshots: vec![snap(at)],
        size_bytes: None,
        drill: None,
        error: None,
        measured_at: None,
    };
    let out = render_at(false, &[repo], now, &|_| "2026-11-05 03:00".into());
    let line = out.lines().find(|l| l.contains("abcdef01")).unwrap();
    assert!(line.contains("2026-11-05 03:00"), "{line}");
    assert!(line.contains("3 h 5 min ago"), "{line}");
    assert!(line.contains("1.5 MiB"), "{line}");
    assert!(line.contains("nightly"), "{line}");
    assert!(!line.contains(&at.to_string()), "{line}");
}

#[test]
fn the_checks_listing_has_no_empty_first_line() {
    let body = readable_reply(
        &Command::ListManualChecks { json: false },
        "\nkyu\n  ab12  [unanswered]  is it fine?\n",
    );
    assert!(body.starts_with("kyu"), "{body:?}");
}

#[test]
fn tool_status_is_a_table_not_json() {
    let raw = r#"{"tools":[
      {"name":"latch","installed":"2.6.0","newest":"2.7.0","follow":"minor","verdict":{"state":"install","version":"2.7.0"}},
      {"name":"restic","installed":null,"newest":"0.19.1","follow":"any","verdict":{"state":"current"}},
      {"name":"broken","error":"no release found"}]}"#;
    let out = readable_reply(&Command::ToolStatus, raw);
    assert!(!out.contains('{') && !out.contains("\"tools\""), "{out}");
    let latch = out.lines().find(|l| l.contains("latch")).unwrap();
    assert!(
        latch.contains("2.6.0") && latch.contains("2.7.0"),
        "{latch}"
    );
    assert!(latch.contains("update to 2.7.0"), "{latch}");
    assert!(
        out.lines()
            .any(|l| l.contains("broken") && l.contains("no release found"))
    );
    assert!(out.lines().next().unwrap().contains("TOOL"), "{out}");
}

#[test]
fn tool_status_that_is_not_json_is_left_as_it_came() {
    assert_eq!(readable_reply(&Command::ToolStatus, "nope"), "nope");
}
