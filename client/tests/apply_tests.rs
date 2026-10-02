//! ask-8 (Kenny, 2026-09-27): `homelab apply` holds the whole stacks
//! directory against the host. A stack whose files changed is deployed, an
//! unchanged one is left alone, and a stack the host still runs but whose
//! directory is gone is offered for destruction — never destroyed without the
//! operator typing its name.

use homelab_client::apply::{ApplyPlan, plan, plan_exit_code, redeploy_reason};
use homelab_core::manifest::ComponentDigests;

/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): `apply`
/// could only act; `apply --plan` prints the plan and answers with an exit
/// code a script can test: 0 in sync, 2 something to deploy or destroy.
#[test]
fn fix_142_apply_plan_exits_zero_in_sync_and_two_when_changes_are_pending() {
    let in_sync = ApplyPlan {
        deploy: vec![],
        unchanged: s(&["syncthing"]),
        destroy: vec![],
    };
    assert_eq!(plan_exit_code(&in_sync), 0);
    let to_deploy = ApplyPlan {
        deploy: s(&["media"]),
        ..in_sync.clone()
    };
    assert_eq!(plan_exit_code(&to_deploy), 2);
    let to_destroy = ApplyPlan {
        destroy: s(&["drill"]),
        ..in_sync
    };
    assert_eq!(plan_exit_code(&to_destroy), 2);
}

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
    use homelab_client::apply::{Decision, decide};
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

/// fix-159: the plan says beforehand which running native units the deploy
/// restarts and why: a changed unit file or drop-in ("unit changed"). A new
/// unit starts rather than restarts, and a stack whose unit files are
/// unchanged restarts nothing. (An env file is never in the stack files, so
/// "env changed" is only in the deploy's transcript, core's tests.)
#[test]
fn fix_159_the_plan_names_the_native_units_a_deploy_restarts() {
    use homelab_client::apply::native_restarts;
    use homelab_proto::FileBlob;
    let blob = |p: &str, c: &str| FileBlob {
        path: p.into(),
        content: c.into(),
        mode: None,
    };
    let admin = "[Service]\nEnvironmentFile=/appdata/admin/admin-config/admin.env\n\
                 ExecStart=/usr/local/bin/homelab-admin\n";
    let applied = vec![
        blob("admin/admin.service", admin),
        blob("admin/checks.yml", "x"),
    ];
    // Unchanged: nothing restarts.
    assert!(native_restarts(&applied, &applied).is_empty());
    // A setting in the unit file.
    let changed_unit = format!("{admin}Environment=HOMELAB_ADMIN_LIVE_ANNOUNCE_MS=5000\n");
    let mut local = applied.clone();
    local[0] = blob("admin/admin.service", &changed_unit);
    assert_eq!(
        native_restarts(&local, &applied),
        vec!["restarts admin: unit changed".to_string()]
    );
    // A drop-in of the unit.
    let mut local = applied.clone();
    local.push(blob(
        "rootfs/etc/systemd/system/admin.service.d/limits.conf",
        "[Service]\nMemoryMax=1G\n",
    ));
    assert_eq!(
        native_restarts(&local, &applied),
        vec!["restarts admin: unit changed".to_string()]
    );
    // A unit the host never had starts; it does not restart.
    let local = vec![blob(
        "kyu/kyu.service",
        "[Service]\nExecStart=/usr/local/bin/kyu\n",
    )];
    assert!(native_restarts(&local, &[]).is_empty());
    // A file no unit reads restarts nothing.
    let mut local = applied.clone();
    local[1] = blob("admin/checks.yml", "y");
    assert!(native_restarts(&local, &applied).is_empty());
}

// ── fix-192 (media-redeploys-without-changing, Kenny 2026-10-02) ──────────

fn digests(files: &[(&str, &str)], env: &[(&str, &str)]) -> ComponentDigests {
    ComponentDigests {
        manifest: "m1".into(),
        files: files
            .iter()
            .map(|(p, h)| (p.to_string(), h.to_string()))
            .collect(),
        env: env
            .iter()
            .map(|(a, h)| (a.to_string(), h.to_string()))
            .collect(),
        secret_files: Default::default(),
        built_by: String::new(),
    }
}

#[test]
fn fix_192_no_component_digests_recorded_is_said_plainly() {
    let local = digests(&[("media/docker-compose.yml", "h1")], &[]);
    assert_eq!(
        redeploy_reason(&local, None),
        "new, or applied by a host that recorded no component digests yet — comparing by the \
         combined fingerprint only"
    );
}

#[test]
fn fix_192_names_a_changed_file() {
    let local = digests(&[("media/docker-compose.yml", "h2")], &[]);
    let applied = digests(&[("media/docker-compose.yml", "h1")], &[]);
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "~ media/docker-compose.yml"
    );
}

#[test]
fn fix_192_names_a_new_file() {
    let local = digests(
        &[
            ("media/docker-compose.yml", "h1"),
            ("media/config.yml", "h9"),
        ],
        &[],
    );
    let applied = digests(&[("media/docker-compose.yml", "h1")], &[]);
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "+ media/config.yml"
    );
}

#[test]
fn fix_192_names_a_gone_file() {
    let local = digests(&[("media/docker-compose.yml", "h1")], &[]);
    let applied = digests(
        &[("media/docker-compose.yml", "h1"), ("media/old.yml", "h5")],
        &[],
    );
    assert_eq!(redeploy_reason(&local, Some(&applied)), "- media/old.yml");
}

#[test]
fn fix_192_names_a_changed_env() {
    let local = digests(&[("media/docker-compose.yml", "h1")], &[("media", "e2")]);
    let applied = digests(&[("media/docker-compose.yml", "h1")], &[("media", "e1")]);
    assert_eq!(redeploy_reason(&local, Some(&applied)), "env: media");
}

#[test]
fn fix_192_names_a_changed_secret_file() {
    let mut local = digests(&[("media/docker-compose.yml", "h1")], &[]);
    local
        .secret_files
        .insert("/opt/media/secret".into(), "s2".into());
    let mut applied = digests(&[("media/docker-compose.yml", "h1")], &[]);
    applied
        .secret_files
        .insert("/opt/media/secret".into(), "s1".into());
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "secret: /opt/media/secret"
    );
}

/// The exact defect this fix answers: the host recorded an earlier
/// manifest digest (a release predating a new manifest field), nothing in
/// files/env/secrets moved, but the combined intent hash still differs —
/// the plan must say so instead of pointing at a file nothing touched.
#[test]
fn fix_192_manifest_only_difference_names_the_versions() {
    let mut local = digests(&[("media/docker-compose.yml", "h1")], &[]);
    local.manifest = "m2".into();
    local.built_by = "v3.70.2".into();
    let mut applied = digests(&[("media/docker-compose.yml", "h1")], &[]);
    applied.manifest = "m1".into();
    applied.built_by = "v3.70.0".into();
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "no file, env or secret changed — only the manifest homelab derives from them \
         (homelab v3.70.0 → v3.70.2)"
    );
}

#[test]
fn fix_192_manifest_only_difference_with_no_recorded_build_names_it_plainly() {
    let mut local = digests(&[("media/docker-compose.yml", "h1")], &[]);
    local.manifest = "m2".into();
    let mut applied = digests(&[("media/docker-compose.yml", "h1")], &[]);
    applied.manifest = "m1".into();
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "no file, env or secret changed — only the manifest homelab derives from them \
         (homelab an earlier version → this version)"
    );
}

#[test]
fn fix_192_identical_digests_say_so() {
    let local = digests(&[("media/docker-compose.yml", "h1")], &[("media", "e1")]);
    let applied = digests(&[("media/docker-compose.yml", "h1")], &[("media", "e1")]);
    assert_eq!(
        redeploy_reason(&local, Some(&applied)),
        "no difference found between the recorded digests"
    );
}

// ── fix-219 (drift-finding-names-only-filenames) ────────────────────────────
//
// A repo-drift finding named only the file names that changed, so Kenny had
// to ask what changed. `file_diffs` pairs the same two file lists
// `deploy_diff` already pairs for the Apply page (this working copy's own
// files, the host's `GetApplied` copy) into a per-file unified diff, capped
// short, with a secret path never showing content.

mod fix_219_file_diffs {
    use homelab_client::apply::file_diffs;
    use homelab_proto::FileBlob;

    fn blob(p: &str, c: &str) -> FileBlob {
        FileBlob {
            path: p.into(),
            content: c.into(),
            mode: None,
        }
    }

    /// Fail-before-fix case 1: a changed compose line's diff names the
    /// actual old line and the actual new line, not just the file name.
    #[test]
    fn a_changed_compose_line_includes_the_old_and_the_new_line() {
        let local = vec![blob(
            "syncthing/docker-compose.yml",
            "services:\n  syncthing:\n    image: syncthing/syncthing:1.28\n",
        )];
        let applied = vec![blob(
            "syncthing/docker-compose.yml",
            "services:\n  syncthing:\n    image: syncthing/syncthing:1.27\n",
        )];
        let diffs = file_diffs(&local, &applied);
        let d = diffs
            .iter()
            .find(|d| d.path == "syncthing/docker-compose.yml")
            .expect("the changed file is in the diff");
        assert_eq!(d.sign, '~');
        let text = d.diff.as_deref().expect("a non-secret file shows content");
        assert!(
            text.contains("-    image: syncthing/syncthing:1.27"),
            "{text}"
        );
        assert!(
            text.contains("+    image: syncthing/syncthing:1.28"),
            "{text}"
        );
    }

    /// Fail-before-fix case 2: a secret file (`.env`) never shows content,
    /// only that it changed — even though `file_diffs` is handed its full
    /// content directly (belt and braces: today's digest comparison never
    /// sends a secret path here in the first place, but the helper itself
    /// must never be the place that leaks one).
    #[test]
    fn a_secret_file_never_shows_content() {
        let local = vec![blob("syncthing/.env", "TOKEN=new-secret")];
        let applied = vec![blob("syncthing/.env", "TOKEN=old-secret")];
        let diffs = file_diffs(&local, &applied);
        let d = diffs
            .iter()
            .find(|d| d.path == "syncthing/.env")
            .expect("the changed secret path is still named");
        assert_eq!(d.sign, '~');
        assert!(d.diff.is_none(), "{:?}", d.diff);
    }

    /// Fail-before-fix case 3: the cap works — a diff longer than
    /// `FILE_DIFF_MAX_LINES` is cut short with a "… N more lines" marker,
    /// never the whole file.
    #[test]
    fn a_long_diff_is_capped() {
        let old: String = (0..30).map(|i| format!("old{i}\n")).collect();
        let new: String = (0..30).map(|i| format!("new{i}\n")).collect();
        let local = vec![blob("x/docker-compose.yml", &new)];
        let applied = vec![blob("x/docker-compose.yml", &old)];
        let diffs = file_diffs(&local, &applied);
        let text = diffs[0].diff.as_deref().unwrap();
        assert!(text.contains("more lines"), "{text}");
        let body_lines = text
            .lines()
            .filter(|l| !(l.starts_with("--- ") || l.starts_with("+++ ") || l.starts_with("@@ ")))
            .count();
        assert_eq!(body_lines, 21, "{text}");
    }

    #[test]
    fn a_new_file_is_a_plus_with_only_added_lines() {
        let local = vec![blob("x/new.yml", "a\nb\n")];
        let diffs = file_diffs(&local, &[]);
        assert_eq!(diffs[0].sign, '+');
        let text = diffs[0].diff.as_deref().unwrap();
        assert!(text.contains("+a") && text.contains("+b"), "{text}");
    }

    #[test]
    fn a_removed_file_is_a_minus_with_only_removed_lines() {
        let applied = vec![blob("x/gone.yml", "a\nb\n")];
        let diffs = file_diffs(&[], &applied);
        assert_eq!(diffs[0].sign, '-');
        let text = diffs[0].diff.as_deref().unwrap();
        assert!(text.contains("-a") && text.contains("-b"), "{text}");
    }

    #[test]
    fn an_unchanged_file_produces_no_entry() {
        let same = vec![blob("x/same.yml", "a\n")];
        assert!(file_diffs(&same, &same).is_empty());
    }
}
