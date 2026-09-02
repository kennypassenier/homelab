//! T51: the front page rendered from the routes the orchestrator already
//! writes, instead of a file somebody maintains.

use homelab_core::ops::homepage::{entries_from_route, services_yaml, Entry};

/// The real media route fragment, verbatim from CT 104 on 2026-09-01.
const MEDIA_ROUTE: &str = r#"# Traefik route fragment for the media stack.
#
# Six names, all at the address that does not change in a C4 rebuild.

http:
  routers:
    jellyfin:
      rule: "Host(`fin.kp-soft.dev`)"
      entryPoints: [web]
      service: jellyfin
    sonarr:
      rule: "Host(`son.kp-soft.dev`)"
      entryPoints: [web]
      service: sonarr

  services:
    jellyfin:
      loadBalancer:
        servers:
          - url: "http://10.10.10.6:8096"
"#;

/// covers: F166
#[test]
fn a_route_fragment_yields_its_hostnames_and_not_its_backends() {
    let got = entries_from_route(MEDIA_ROUTE);
    assert_eq!(
        got,
        vec![
            Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into()
            },
            Entry {
                app: "sonarr".into(),
                host: "son.kp-soft.dev".into()
            },
        ],
        "only the routers' hostnames belong on the front page"
    );
    // The `services:` block below carries an internal address and a name that
    // repeats the router's. Picking that up would put http://10.10.10.6:8096
    // on the front page — reachable from the house only, and not the door
    // Kenny uses.
    assert!(
        !format!("{:?}", got).contains("10.10.10.6"),
        "an internal backend address must never reach the page: {:?}",
        got
    );
}

#[test]
fn a_stack_without_routes_is_skipped_rather_than_rendered_empty() {
    let out = services_yaml(&[
        ("registry".into(), vec![]),
        (
            "media".into(),
            vec![Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
            }],
        ),
    ]);
    assert!(
        !out.contains("registry"),
        "an empty heading reads as 'broken' when it means 'no front door': {}",
        out
    );
    assert!(out.contains("- media:"));
    assert!(out.contains("href: https://fin.kp-soft.dev/"));
}

#[test]
fn rendering_twice_produces_the_same_bytes() {
    let stacks = vec![(
        "media".into(),
        vec![
            Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
            },
            Entry {
                app: "sonarr".into(),
                host: "son.kp-soft.dev".into(),
            },
        ],
    )];
    assert_eq!(
        services_yaml(&stacks),
        services_yaml(&stacks),
        "a deploy that changes nothing must write an identical file, or every \
         deploy reports a change that is not one"
    );
}

/// covers: F190
///
/// A generated file lands in a directory that belongs to a CONTAINER, and
/// the orchestrator writes it as host root. On an unprivileged container
/// that is the wrong owner, and Uptime Kuma proves it is not cosmetic: it
/// chowns everything under `/app/data` at startup, cannot own a host-root
/// file, exits non-zero and crash-loops. That is how the monitoring went
/// down on 2026-09-02 — over a backup file, and measuring afterwards found
/// this code had written two files with exactly the same problem.
#[tokio::test]
async fn a_generated_file_takes_the_owner_of_the_directory_it_lands_in() {
    let exec = homelab_core::executor::MockExecutor::new();
    homelab_core::ops::util::write_file_owned_like_dir(
        &exec,
        "/appdata/home/homepage-config/services.yaml",
        "- gateway:\n",
        0o644,
    )
    .await
    .unwrap();

    let chowns = exec.calls_containing("chown");
    assert_eq!(
        chowns.len(),
        1,
        "expected exactly one chown after the write, got {:?}",
        exec.calls()
    );
    assert!(
        chowns[0].contains("--reference")
            && chowns[0].contains("/appdata/home/homepage-config")
            && chowns[0].contains("/appdata/home/homepage-config/services.yaml"),
        "the owner must be copied from the parent directory, not computed: {}",
        chowns[0]
    );
}

/// The chown is best-effort: a convenience file must never fail a deploy.
#[tokio::test]
async fn a_failing_chown_does_not_fail_the_write() {
    let exec = homelab_core::executor::MockExecutor::new();
    exec.enqueue(
        "chown",
        homelab_core::executor::CmdOutput::failed(1, "chown: not permitted"),
    );
    let r = homelab_core::ops::util::write_file_owned_like_dir(
        &exec,
        "/appdata/uptime/kuma-seeder-config/host-monitors.json",
        "{}",
        0o644,
    )
    .await;
    assert!(r.is_ok(), "a failed chown must not fail the write");
}
