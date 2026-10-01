//! gap-34: `destroy`, `resize` and `prune-orphans` send only a stack's
//! manifest (`destroy`, `resize`) or its file paths (`prune-orphans`) to the
//! host, never a secret — so none of the three has any reason to run latch,
//! even for a stack whose `latch_secrets`/`latch_files` would make a real
//! deploy run it. Before this, all three called `spec::build_spec`, which
//! ran latch (and demanded `HOMELAB_LATCH_ENV`) for exactly such a stack and
//! then discarded everything latch returned.
//!
//! Same stub-latch-on-PATH approach as `latch_secrets_tests.rs`: a real
//! subprocess, so "latch was not consulted" is proven by an unchanged call
//! log, not by reading the source.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A stack directory declaring both a latch-sourced app env and a
/// latch-sourced file, so both `fetch_latch_secrets` and `fetch_latch_files`
/// would run on a real `build_spec`.
fn stack_dir(root: &Path) -> PathBuf {
    let dir = root.join("stacks").join("gap34");
    write(
        &dir.join("lxc-compose.yml"),
        "stack_name: gap34\nvmid: 141\nhostname: 141-app-gap34\n\
         network:\n  ip: 10.10.10.41/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n\
         resources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 256\n  disk_gb: 4\n\
         lxc:\n  template: clone:999\nboot:\n  onboot: true\n\
         storage: []\napps: [kyu]\n\
         latch_secrets: [kyu]\n\
         latch_files:\n  - from: kyu/token.env\n    dest: /opt/gap34/token.env\n    mode: \"640\"\n",
    );
    write(
        &dir.join("kyu").join("docker-compose.yml"),
        "services: {}\n",
    );
    dir
}

/// A stub that always fails — proof that nothing here calls it, not merely
/// that a lenient call happened to succeed.
fn install_failing_stub(bin_dir: &Path, log: &Path) {
    std::fs::create_dir_all(bin_dir).unwrap();
    let stub = bin_dir.join("latch");
    let mut f = std::fs::File::create(&stub).unwrap();
    writeln!(
        f,
        "#!/bin/sh\necho \"$@\" >> {}\necho 'latch must not have been called' >&2\nexit 1",
        log.display()
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn gap_34_build_manifest_and_files_only_never_touch_latch() {
    let tmp = std::env::temp_dir().join(format!("homelab-gap34-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let log = tmp.join("stub.log");
    install_failing_stub(&tmp.join("bin"), &log);
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            tmp.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    // The point: HOMELAB_LATCH_ENV is deliberately unset. A real build_spec
    // on this stack dir would refuse right here; destroy/resize/prune-orphans
    // must not care.
    std::env::remove_var("HOMELAB_LATCH_ENV");

    let dir = stack_dir(&tmp);

    // destroy / resize: the manifest alone, with no secret-bearing fields.
    let manifest = homelab_client::spec::build_manifest(&dir)
        .expect("build_manifest must succeed with no HOMELAB_LATCH_ENV");
    assert_eq!(manifest.stack_name, "gap34");

    // prune-orphans: the file list alone.
    let spec = homelab_client::spec::build_spec_files_only(&dir)
        .expect("build_spec_files_only must succeed with no HOMELAB_LATCH_ENV");
    assert_eq!(spec.manifest.stack_name, "gap34");
    assert!(
        spec.env.is_empty(),
        "no app env was ever read from latch: {:?}",
        spec.env
    );
    assert!(
        spec.secret_files.is_empty(),
        "no secret file was ever fetched from latch: {:?}",
        spec.secret_files
    );
    // compose files from disk still show up — prune-orphans still needs them.
    assert!(
        spec.files.iter().any(|f| f.path.contains("docker-compose")),
        "ordinary on-disk files are still collected: {:?}",
        spec.files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );

    assert!(
        !log.exists(),
        "latch must never have been invoked, but its call log exists: {}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );

    // A real deploy on the very same directory DOES need it and DOES fail
    // without it — the control proving the stub and the directory are wired
    // correctly, not just permissive.
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("HOMELAB_LATCH_ENV"), "{}", err);

    let _ = std::fs::remove_dir_all(&tmp);
}
