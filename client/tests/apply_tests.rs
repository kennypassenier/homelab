//! ask-8 (Kenny, 2026-09-27): `homelab apply` holds the whole stacks
//! directory against the host. A stack whose files changed is deployed, an
//! unchanged one is left alone, and a stack the host still runs but whose
//! directory is gone is offered for destruction — never destroyed without the
//! operator typing its name.

use homelab_client::apply::{plan, ApplyPlan};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// covers: ask-8
#[test]
fn apply_deploys_what_changed_and_offers_what_left_for_destruction() {
    let local = pairs(&[("syncthing", "h1"), ("media", "h2"), ("new", "h3")]);
    // `inbox` is a directory with only a service.yml: an adopted native stack,
    // not deployable, but its directory is there.
    let dirs = s(&["syncthing", "media", "new", "inbox"]);
    let host = pairs(&[
        ("syncthing", "h1"),
        ("media", "old"),
        ("drill", "x"),
        ("inbox", ""),
    ]);
    let p = plan(&local, &dirs, &host);
    assert_eq!(
        p,
        ApplyPlan {
            deploy: s(&["media", "new"]),
            unchanged: s(&["syncthing"]),
            destroy: s(&["drill"]),
        }
    );
}

/// A host record with no applied hash (half-deployed, or written before B4)
/// cannot be judged equal, so it is deployed rather than skipped.
/// covers: ask-8
#[test]
fn apply_deploys_a_stack_the_host_has_no_hash_for() {
    let p = plan(
        &pairs(&[("kyu", "h1")]),
        &s(&["kyu"]),
        &pairs(&[("kyu", "")]),
    );
    assert_eq!(p.deploy, s(&["kyu"]));
    assert!(p.destroy.is_empty());
}
