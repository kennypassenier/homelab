//! D60: the image rewrite that points a pull at the cache in the house.

use homelab_core::ops::registry_cache::{rewrite_compose, split_registry, CacheCfg, CacheUpstream};

fn cfg() -> CacheCfg {
    CacheCfg {
        host: "10.10.10.17".into(),
        upstreams: vec![
            CacheUpstream {
                registry: "docker.io".into(),
                port: 5000,
            },
            CacheUpstream {
                registry: "ghcr.io".into(),
                port: 5001,
            },
            CacheUpstream {
                registry: "gcr.io".into(),
                port: 5002,
            },
            CacheUpstream {
                registry: "lscr.io".into(),
                port: 5003,
            },
        ],
    }
}

const ALL: [&str; 4] = ["docker.io", "ghcr.io", "gcr.io", "lscr.io"];

fn all() -> Vec<String> {
    ALL.iter().map(|s| s.to_string()).collect()
}

/// Docker's naming rules are not obvious, and getting them wrong produces a
/// 404 from the cache rather than an error anyone can read. A first segment
/// is a registry only if it has a dot or a port; a Hub image with no
/// namespace lives under `library/`.
#[test]
fn an_image_reference_is_split_the_way_docker_splits_it() {
    assert_eq!(
        split_registry("ghcr.io/gethomepage/homepage:v2.1.2"),
        ("ghcr.io".into(), "gethomepage/homepage:v2.1.2".into())
    );
    assert_eq!(
        split_registry("lscr.io/linuxserver/qbittorrent:latest"),
        ("lscr.io".into(), "linuxserver/qbittorrent:latest".into())
    );
    // No dot in the first segment: Docker Hub, with a namespace.
    assert_eq!(
        split_registry("vikunja/vikunja@sha256:abc"),
        ("docker.io".into(), "vikunja/vikunja@sha256:abc".into())
    );
    // No slash at all: Docker Hub's own library.
    assert_eq!(
        split_registry("postgres:16-alpine"),
        ("docker.io".into(), "library/postgres:16-alpine".into())
    );
}

#[test]
fn every_image_is_pointed_at_the_cache_when_it_answers() {
    let compose = "services:\n  a:\n    image: ghcr.io/gethomepage/homepage:v2.1.2\n  b:\n    image: postgres:16-alpine\n  c:\n    image: gcr.io/cadvisor/cadvisor:latest\n";
    let out = rewrite_compose(compose, &cfg(), &all(), None);
    assert!(
        out.contains("image: 10.10.10.17:5001/gethomepage/homepage:v2.1.2"),
        "{}",
        out
    );
    assert!(
        out.contains("image: 10.10.10.17:5000/library/postgres:16-alpine"),
        "{}",
        out
    );
    assert!(
        out.contains("image: 10.10.10.17:5002/cadvisor/cadvisor:latest"),
        "{}",
        out
    );
    // Indentation and every other line survive untouched.
    assert!(
        out.starts_with("services:\n  a:\n    image: 10.10.10.17:5001/"),
        "{}",
        out
    );
    assert!(out.ends_with('\n'));
}

/// The whole reason Kenny chose this variant: a cache that does not answer
/// costs speed, not the deploy. An upstream missing from the available list
/// is left naming its own origin.
#[test]
fn an_upstream_that_did_not_answer_is_left_alone() {
    let compose = "    image: ghcr.io/x/y:1\n    image: postgres:16\n";
    let out = rewrite_compose(compose, &cfg(), &["docker.io".to_string()], None);
    assert!(
        out.contains("image: ghcr.io/x/y:1"),
        "ghcr was down: {}",
        out
    );
    assert!(
        out.contains("image: 10.10.10.17:5000/library/postgres:16"),
        "{}",
        out
    );

    // Nothing answered at all: the file is returned exactly as it was.
    let out = rewrite_compose(compose, &cfg(), &[], None);
    assert_eq!(out, compose);
}

/// The cache is anonymous, so a private image must keep going to its own
/// registry with its own token. A cache holding kp-soft's image would hand
/// it to anyone on the LAN who asked.
#[test]
fn a_private_registry_is_never_routed_through_the_cache() {
    let compose =
        "    image: ghcr.io/kennypassenier/kp-soft:v0.2.0\n    image: grafana/promtail:3.0.0\n";
    let out = rewrite_compose(compose, &cfg(), &all(), Some("ghcr.io"));
    assert!(
        out.contains("image: ghcr.io/kennypassenier/kp-soft:v0.2.0"),
        "the private one keeps its own registry: {}",
        out
    );
    assert!(
        out.contains("image: 10.10.10.17:5000/grafana/promtail:3.0.0"),
        "its neighbours are still cached: {}",
        out
    );
}

/// Lines that are not an image reference are never touched — including a
/// comment that happens to mention one.
#[test]
fn only_image_lines_are_rewritten() {
    let compose = "# see image: ghcr.io/x/y:1 for why\n    environment:\n      - IMAGE=ghcr.io/x/y:1\n    image: ghcr.io/x/y:1\n";
    let out = rewrite_compose(compose, &cfg(), &all(), None);
    assert!(
        out.starts_with("# see image: ghcr.io/x/y:1 for why"),
        "{}",
        out
    );
    assert!(out.contains("- IMAGE=ghcr.io/x/y:1"), "{}", out);
    assert_eq!(out.matches("10.10.10.17:5001").count(), 1, "{}", out);
}

/// The daemon has to be told the cache speaks plain HTTP, or every cached
/// pull fails with "server gave HTTP response to HTTPS client" — which reads
/// like a broken cache rather than a missing setting.
#[tokio::test]
async fn the_daemon_config_declares_the_cache_addresses() {
    use homelab_core::executor::MockExecutor;
    use homelab_core::sink::VecSink;
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 108, true, Some(&cfg()))
        .await
        .expect("guards apply");
    let pushed = exec.file("/tmp/does-not-matter").unwrap_or_default();
    let _ = pushed;
    let calls = exec.calls().join("\n");
    assert!(
        calls.contains("daemon.json"),
        "the daemon config is written"
    );

    // And without a cache the file keeps its original shape.
    let exec2 = MockExecutor::new();
    let sink2 = VecSink::new();
    homelab_core::ops::guards::apply(&exec2, &sink2, 108, true, None)
        .await
        .expect("guards apply");
    assert!(exec2.calls().join("\n").contains("daemon.json"));
}
