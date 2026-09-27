//! fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): what
//! `homelab check` sends about each stack directory so the host can compare
//! it with what it last applied.
//!
//! The digest must describe exactly what a deploy would send into the
//! container, or the check reports drift that a deploy would not change (or
//! misses drift it would): orchestrator inputs (`lxc-compose.yml`,
//! `traefik-routes.yml`, `checks.yml`, `service.yml`) and `.env` files stay
//! out, and no secret is read.

use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "homelab-check-digest-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn fix_142_a_stack_digest_hashes_what_a_deploy_would_send() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = scratch("syncthing");
    // The real stack file, so the manifest parses the way a deploy's does.
    std::fs::copy(
        repo.join("stacks/syncthing/lxc-compose.yml"),
        dir.join("lxc-compose.yml"),
    )
    .unwrap();
    std::fs::write(dir.join("traefik-routes.yml"), "http: {}\n").unwrap();
    std::fs::create_dir_all(dir.join("syncthing")).unwrap();
    std::fs::write(dir.join("syncthing/docker-compose.yml"), "services: {}\n").unwrap();
    std::fs::write(dir.join("syncthing/.env"), "SECRET=x\n").unwrap();
    std::fs::write(dir.join("syncthing/checks.yml"), "{}\n").unwrap();

    let d = homelab_client::spec::stack_digest(&dir).expect("digest");
    let name = dir.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(d.stack, name);
    let m = d.manifest.expect("the manifest travels");
    assert_eq!(m.stack_name, "syncthing");
    assert_eq!(
        d.files.keys().cloned().collect::<Vec<_>>(),
        vec!["syncthing/docker-compose.yml".to_string()],
        "only what goes into the container"
    );
    assert_eq!(
        d.files["syncthing/docker-compose.yml"],
        homelab_core::manifest::sha256_hex(b"services: {}\n")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_142_a_stack_with_only_a_service_file_sends_no_manifest() {
    let dir = scratch("inbox");
    std::fs::write(dir.join("service.yml"), "unit: inbox\nvmid: 118\n").unwrap();
    let d = homelab_client::spec::stack_digest(&dir).expect("digest");
    assert!(d.manifest.is_none());
    assert!(d.files.is_empty(), "{:?}", d.files);
    let _ = std::fs::remove_dir_all(&dir);
}
