//! traefik-docker-socket (expert panel 2026-09-27): the real docker socket
//! held by the internet-facing reverse proxy, read-only in name only — the
//! `:ro` on a bind mount does not limit which docker API calls the process
//! on the other end can make. Fixed in the stack files, not in code (the
//! architecture bar: no app knowledge here), by routing every compose
//! provider through a narrow, read-only socket proxy instead.
//!
//! This guard is declarative and generic on purpose: it does not name the
//! gateway stack or any app. It reads every compose file the repository
//! ships and enforces one rule — a container that bind-mounts the real
//! docker socket must itself expose a narrow, read-only proxy of it (an
//! image whose name says "socket-proxy", `read_only: true`, no published
//! `ports:`), so a NEW compose file that mounts the socket directly fails
//! this test rather than silently reintroducing the hole.

use std::path::{Path, PathBuf};

fn compose_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut out = Vec::new();
    for stack in std::fs::read_dir(&root).unwrap().flatten() {
        if !stack.path().is_dir() {
            continue;
        }
        for app in std::fs::read_dir(stack.path()).unwrap().flatten() {
            let f = app.path().join("docker-compose.yml");
            if f.is_file() {
                out.push(f);
            }
        }
    }
    out.sort();
    out
}

fn mounts_real_docker_socket(content: &str) -> bool {
    content
        .lines()
        .any(|l| l.contains("/var/run/docker.sock:/var/run/docker.sock"))
}

#[test]
fn only_a_narrow_read_only_proxy_may_hold_the_real_docker_socket() {
    let files = compose_files();
    assert!(files.len() > 10, "the compose sweep broke: {:?}", files);
    let mut offenders = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        if !mounts_real_docker_socket(&text) {
            continue;
        }
        let is_narrow_proxy = text.contains("docker-socket-proxy")
            // fix-167 (2026-10-02): the socket itself is mounted read-only. The
            // container root cannot be: the image renders haproxy.cfg at every
            // start, and a read-only root crash-looped it on the first deploy.
            && text.contains("/var/run/docker.sock:/var/run/docker.sock:ro")
            && !text.contains("ports:")
            // The whole point: a proxy that would itself allow writes back
            // to the socket is not narrow, whatever its image is called.
            && (text.contains("POST=0") || !text.contains("POST=1"));
        if !is_narrow_proxy {
            offenders.push(f.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "these compose files bind-mount the real docker socket without being a narrow, \
         read-only, unpublished socket-proxy image (traefik-docker-socket): {} — route \
         through a docker-socket-proxy-style container instead, as the gateway stack does",
        offenders.join(", ")
    );
}

/// The one container allowed to hold the socket must itself not be reachable
/// from outside its docker network: no `ports:` section at all.
#[test]
fn the_socket_proxy_publishes_no_port() {
    let files = compose_files();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        if text.contains("docker-socket-proxy") {
            assert!(
                !text.contains("ports:"),
                "{} runs the socket proxy but publishes a port — it must be reachable only \
                 from containers on its own docker network",
                f.display()
            );
        }
    }
}
