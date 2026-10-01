//! fix-144 (expert panel 2026-09-27, update-policy-doc-drift): the update
//! policy of every app and native service, as a table generated from the
//! stack files into docs/deployment/UPDATE_POLICY.md.
//!
//! The hand-kept lists disagreed with the stack files and with each other
//! (http-switchboard under "the docker path", kyu "publishes no release
//! assets at all", almanac "updates itself" and `manual` at once; REGISTER
//! F192, "classes and the fleet disagree in seven places"). The artefact is
//! checked, as the DR runbook is: a stack file that changes without the
//! table being regenerated fails here.

use std::path::{Path, PathBuf};

use homelab_client::updatepolicy::{BEGIN, END, policy_table, splice};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn fix_144_the_table_says_what_each_stack_file_says() {
    let t = policy_table(&root().join("stacks")).unwrap();
    for row in [
        // compose: stack, app, service, label
        "| gateway | traefik | traefik | `manual` |",
        "| syncthing | syncthing | syncthing | `auto` |",
        // natives: stack, unit, policy, what the night does
        "| kyu | kyu | `auto` |",
        "| kyu | http-switchboard | `manual` | nothing; updated on request |",
        "| almanac | almanac | `self` | runs its own update verb |",
    ] {
        assert!(t.contains(row), "missing `{row}` in:\n{t}");
    }
    // One label decides for an app with several services (update.rs asks
    // the first container), so a mixed app is said out loud. gateway's
    // goaccess sidecar was the other mixed app until it was retired
    // 2026-10-01 with Homepage, GoAccess and Uptime Kuma; paperless-db
    // (postgres `manual`, redis `auto`) still is one.
    assert!(
        t.lines()
            .any(|l| l.contains("| paperwork | paperless-db |") && l.contains("mixed")),
        "{t}"
    );
}

#[test]
fn fix_144_only_the_marked_section_is_replaced() {
    let doc = format!("# Title\n\nprose\n\n{BEGIN}\nold table\n{END}\n\nmore prose\n");
    let out = splice(&doc, "NEW\n").unwrap();
    assert_eq!(
        out,
        format!("# Title\n\nprose\n\n{BEGIN}\nNEW\n{END}\n\nmore prose\n")
    );
    assert!(
        splice("# no markers\n", "NEW\n").is_err(),
        "a document without the markers is refused, not appended to"
    );
}

#[test]
fn fix_144_the_committed_update_policy_matches_a_fresh_generation() {
    let committed =
        std::fs::read_to_string(root().join("docs/deployment/UPDATE_POLICY.md")).unwrap();
    let fresh = splice(&committed, &policy_table(&root().join("stacks")).unwrap()).unwrap();
    assert_eq!(
        fresh, committed,
        "docs/deployment/UPDATE_POLICY.md is stale — run `homelab update-policy` and read \
         the diff before committing it"
    );
}
