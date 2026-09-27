//! older-client-no-warning (expert panel, 2026-09-27): updating the client
//! was a five-line gh/sha256sum/install routine per workstation (op-9 step
//! 6). `homelab self-install [tag]` downloads the release's `homelab`,
//! verifies it against the release's SHA256SUMS and replaces the running
//! binary. The replacing half is tested here; the download is the same
//! staging `release-update` uses.

/// covers: fix-105
#[test]
fn fix_105_self_install_replaces_the_binary_in_place() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("homelab-self-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("homelab");
    std::fs::write(&target, b"old client").unwrap();

    homelab_client::release::install_binary(b"new client", &target).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"new client");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755, "the new client must be executable");
    let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(leftovers.len(), 1, "no temporary file left behind");
    let _ = std::fs::remove_dir_all(&dir);
}
