//! redesign-flows-6 (3.71.0): the host's fresh read of one stack's
//! containers after a deploy (`Command::StackRuntime`), which the
//! dashboard's Update flow verifies with: every container with whether it
//! runs, each `manual` one's image, and whether a container runs the image
//! line the flow committed — through a registry-cache rewrite too.

use homelab_core::ops::pins::{parse_runtime, runs_image, runtime_script};

const OUT: &str = "demo-api|running\ndemo-web|exited\n--manual--\n\
/demo-api|registry-cache.lan:5000/example/demo-api:v3.0.0@sha256:bbbb|sha256:img|github.com/example/demo-api|example/demo-api@sha256:bbbb\n";

#[test]
fn redesign_flows_6_the_runtime_reads_every_container_and_each_manual_image() {
    let rt = parse_runtime(OUT, 7);
    assert_eq!(
        rt.containers,
        vec![
            ("demo-api".to_string(), true),
            ("demo-web".to_string(), false)
        ]
    );
    assert_eq!(rt.images["demo-api"].digest, "sha256:bbbb");
    assert_eq!(rt.images["demo-api"].seen_at, 7);
    assert!(runtime_script().starts_with("docker ps -a --format '{{.Names}}|{{.State}}'"));
}

#[test]
fn redesign_flows_6_a_container_runs_the_committed_line_by_digest_or_version() {
    let rt = parse_runtime(OUT, 0);
    assert_eq!(
        runs_image(&rt.images, "example/demo-api:v3.0.0@sha256:bbbb"),
        Some(true)
    );
    assert_eq!(
        runs_image(&rt.images, "ghcr.io/example/demo-api:v3.0.0"),
        Some(true)
    );
    assert_eq!(
        runs_image(&rt.images, "example/demo-api:v2.3.0@sha256:aaaa"),
        Some(false)
    );
    assert_eq!(runs_image(&rt.images, "example/other:v1"), None);
}
