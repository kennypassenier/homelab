//! latch-files (Kenny, 2026-09-30: "Vastleggen via latch"): secret files a
//! stack reads from latch for an absolute path in the container. The latch
//! here is a stub on PATH, as in latch_secrets_tests.rs: a real subprocess,
//! so argv, cwd and exit codes are exercised for real.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// SAFETY: this file has exactly one `#[test]` fn (see the comment above
/// it), so these two wrappers are never called from more than one thread.
#[allow(unsafe_code)]
fn set_env(k: &str, v: &str) {
    unsafe { std::env::set_var(k, v) }
}

#[allow(unsafe_code)]
fn remove_env(k: &str) {
    unsafe { std::env::remove_var(k) }
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A native-only stack with one unit and the given `latch_files` block.
fn stack_dir(root: &Path, latch_files: &str) -> PathBuf {
    let dir = root.join("stacks").join("lftest");
    write(
        &dir.join("lxc-compose.yml"),
        &format!(
            "stack_name: lftest\nvmid: 141\nhostname: 141-app-lftest\n\
             network:\n  ip: 10.10.10.41/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n\
             resources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 256\n  disk_gb: 4\n\
             lxc:\n  template: clone:999\nboot:\n  onboot: true\n\
             storage: []\napps: []\nnative_only: true\nnatives: [switch]\n{}",
            latch_files
        ),
    );
    write(
        &dir.join("switch").join("switch.service"),
        "[Unit]\nDescription=switch\n\n[Service]\nExecStart=/usr/local/bin/switch\n",
    );
    dir
}

fn install_stub(bin_dir: &Path, log: &Path) {
    std::fs::create_dir_all(bin_dir).unwrap();
    let stub = bin_dir.join("latch");
    let mut f = std::fs::File::create(&stub).unwrap();
    writeln!(
        f,
        "#!/bin/sh\necho \"$(pwd)|$@\" >> {}\ncase \"$LATCH_STUB_MODE\" in\n\
         fail) echo 'not found' >&2; exit 1;;\n\
         empty) exit 0;;\n\
         template) printf 'token = \"${{KYU_TOKEN}}\"\\n';;\n\
         *) printf 'url = \"http://ha/api/webhook/ID\"\\n';;\nesac",
        log.display()
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
}

const ENTRY: &str = "latch_files:\n  - from: switch/config.toml\n    \
                     dest: /appdata/lftest/switch-config/config.toml\n    \
                     mode: \"640\"\n    owner: root:switch\n    restarts: switch\n";

/// One test fn on purpose: PATH and HOMELAB_LATCH_ENV are process-global.
#[test]
fn latch_files_are_read_from_latch_and_checked_before_any_call() {
    let tmp = std::env::temp_dir().join(format!("homelab-lf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let log = tmp.join("stub.log");
    install_stub(&tmp.join("bin"), &log);
    set_env(
        "PATH",
        &format!(
            "{}:{}",
            tmp.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    let calls = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .count()
    };

    // 1. No HOMELAB_LATCH_ENV: refused, naming it, and latch never ran.
    remove_env("HOMELAB_LATCH_ENV");
    let dir = stack_dir(&tmp, ENTRY);
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("HOMELAB_LATCH_ENV"), "{}", err);
    assert_eq!(calls(), 0);

    // 2. The happy path: one call, from the stacks/ root, the stack's path.
    set_env("HOMELAB_LATCH_ENV", "prod");
    remove_env("LATCH_STUB_MODE");
    let spec = homelab_client::spec::build_spec(&dir).unwrap();
    assert_eq!(spec.secret_files.len(), 1);
    let f = &spec.secret_files[0];
    assert_eq!(f.path, "/appdata/lftest/switch-config/config.toml");
    assert_eq!(f.content, "url = \"http://ha/api/webhook/ID\"\n");
    assert_eq!(f.mode, "640");
    assert_eq!(f.owner.as_deref(), Some("root:switch"));
    assert_eq!(f.restarts.as_deref(), Some("switch"));
    let line = std::fs::read_to_string(&log).unwrap();
    assert!(
        line.contains("stacks|cat lftest/switch/config.toml --env prod\n"),
        "{}",
        line
    );
    // The secret is in the intent: a changed secret is a changed stack.
    let before = homelab_core::manifest::intent_hash(&spec);
    let mut other = spec.clone();
    other.secret_files[0].content = "changed\n".into();
    assert_ne!(before, homelab_core::manifest::intent_hash(&other));

    // 3. latch fails or answers nothing: the deploy stops, saying which file.
    set_env("LATCH_STUB_MODE", "fail");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(
        err.contains("lftest/switch/config.toml") && err.contains("not found"),
        "{}",
        err
    );
    // A `${` in a file breaks every --expand of the environment: refused.
    set_env("LATCH_STUB_MODE", "template");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(
        err.contains("template") && err.contains("lftest/switch/config.toml"),
        "{}",
        err
    );
    set_env("LATCH_STUB_MODE", "empty");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("empty content"), "{}", err);
    remove_env("LATCH_STUB_MODE");

    // 4. A bad entry is refused before latch is asked anything.
    let n = calls();
    for (bad, want) in [
        (ENTRY.replace("dest: /appdata", "dest: appdata"), "absolute"),
        (ENTRY.replace("/switch-config/", "/../"), "absolute"),
        (ENTRY.replace("\"640\"", "\"rw\""), "octal"),
        (ENTRY.replace("root:switch", "root; rm -rf /"), "user:group"),
        (
            ENTRY.replace("restarts: switch", "restarts: kyu"),
            "not a native unit",
        ),
        (ENTRY.replace("from: switch", "from: /switch"), "relative"),
    ] {
        let d = stack_dir(&tmp, &bad);
        let err = homelab_client::spec::build_spec(&d).unwrap_err();
        assert!(err.contains(want), "{} -> {}", want, err);
    }
    assert_eq!(calls(), n, "latch was asked about a refused entry");
    let _ = std::fs::remove_dir_all(&tmp);
}
