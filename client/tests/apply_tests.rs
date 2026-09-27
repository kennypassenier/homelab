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

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// apply-no-confirm-creates-drill (expert panel, 2026-09-27): every directory
/// with an `lxc-compose.yml` counted as declared, the throwaway drill stack
/// (vmid 119, destroyed in the sitting that made it) included. The next
/// `homelab apply` would have created CT 119, and the DR runbook told a
/// rebuild to bring it back. A stack file saying `ephemeral: true` is
/// deployed only by name.
/// covers: fix-100
#[test]
fn fix_100_an_ephemeral_stack_is_neither_applied_nor_in_the_runbook() {
    let stacks = repo_root().join("stacks");
    let declared: Vec<String> = homelab_client::spec::declared_stacks(&stacks)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(!declared.contains(&"drill".to_string()), "{:?}", declared);
    assert!(declared.contains(&"gateway".to_string()), "{:?}", declared);
    assert!(homelab_client::spec::is_ephemeral(&stacks.join("drill")));

    let out = std::env::temp_dir().join(format!("homelab-dr-f100-{}.md", std::process::id()));
    homelab_client::spec::generate_runbook(&stacks, out.to_str().unwrap()).unwrap();
    let doc = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    assert!(
        !doc.contains("119-app-drill"),
        "the runbook rebuilds the drill"
    );
}

/// `homelab apply` deployed every changed stack the moment the plan was
/// printed. It now asks once; `--yes` is the scripted answer and `--dry-run`
/// only shows the plan. Anything but a typed yes keeps the fleet as it is.
/// covers: fix-100
#[test]
fn fix_100_apply_deploys_only_after_a_yes() {
    use homelab_client::apply::{decide, Decision};
    assert_eq!(decide(true, false, None), Decision::Preview);
    assert_eq!(decide(true, true, Some("y")), Decision::Preview);
    assert_eq!(decide(false, true, None), Decision::Deploy);
    assert_eq!(decide(false, false, Some("y")), Decision::Deploy);
    assert_eq!(decide(false, false, Some("YES")), Decision::Deploy);
    assert_eq!(decide(false, false, Some("")), Decision::Decline);
    assert_eq!(decide(false, false, Some("n")), Decision::Decline);
    assert_eq!(decide(false, false, None), Decision::Decline);
}

/// The plan names what each deploy adds, changes and removes, file by file:
/// since ask-8 a deploy removes what the files no longer carry.
/// covers: fix-100
#[test]
fn fix_100_the_apply_plan_lists_added_changed_and_removed_files() {
    use homelab_proto::FileBlob;
    let blob = |p: &str, c: &str| FileBlob {
        path: p.into(),
        content: c.into(),
        mode: None,
    };
    let local = vec![blob("a/compose.yml", "new"), blob("a/extra.yml", "x")];
    let applied = vec![blob("a/compose.yml", "old"), blob("b/compose.yml", "gone")];
    let lines = homelab_client::apply::file_changes(&local, &applied);
    assert_eq!(
        lines,
        vec![
            "~ a/compose.yml".to_string(),
            "+ a/extra.yml".to_string(),
            "- b/compose.yml".to_string(),
        ]
    );
    assert!(homelab_client::apply::file_changes(&local, &local).is_empty());
}
