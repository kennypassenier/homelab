//! D12: secrets from latch instead of plaintext files. The "latch" here is
//! a stub executable on PATH — a real subprocess, so argv, cwd, exit codes
//! and stdout/stderr plumbing are all exercised for real; only latch's
//! crypto is out of scope (that project tests its own).

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A stacks/<name> dir with a minimal valid manifest declaring one app on
/// latch, plus a promtail app with an ordinary on-disk compose (no secrets).
fn stack_dir(root: &Path, latch_secrets: &str) -> PathBuf {
    let dir = root.join("stacks").join("mbtest");
    write(
        &dir.join("lxc-compose.yml"),
        &format!(
            "stack_name: mbtest\nvmid: 140\nhostname: 140-app-mbtest\n\
             network:\n  ip: 10.10.10.40/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n\
             resources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 256\n  disk_gb: 4\n\
             lxc:\n  template: clone:999\nboot:\n  onboot: true\n\
             storage: []\napps: [kyu]\n{}",
            latch_secrets
        ),
    );
    write(
        &dir.join("kyu").join("docker-compose.yml"),
        "services: {}\n",
    );
    dir
}

/// Stub latch: records its argv + cwd, then behaves per LATCH_STUB_MODE.
fn install_stub(bin_dir: &Path, log: &Path) {
    std::fs::create_dir_all(bin_dir).unwrap();
    let stub = bin_dir.join("latch");
    let mut f = std::fs::File::create(&stub).unwrap();
    writeln!(
        f,
        "#!/bin/sh\necho \"$(pwd)|$@\" >> {}\ncase \"$LATCH_STUB_MODE\" in\n\
         fail) echo 'no secrets found for mbtest/prod :: commit+push env files first' >&2; exit 1;;\n\
         empty) exit 0;;\n\
         *) printf 'KYU_TOKEN=stubtoken\\nKYU_SECRET_KEY=stubkey\\n';;\nesac",
        log.display()
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// One test fn on purpose: PATH and HOMELAB_LATCH_ENV are process-global,
/// and parallel tests would race on them.
#[test]
fn d12_latch_sourced_secrets() {
    let tmp = std::env::temp_dir().join(format!("homelab-d12-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let log = tmp.join("stub.log");
    install_stub(&tmp.join("bin"), &log);
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            tmp.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    // 1. Declared but HOMELAB_LATCH_ENV unset → hard error naming the remedy.
    std::env::remove_var("HOMELAB_LATCH_ENV");
    let dir = stack_dir(&tmp, "latch_secrets: [kyu]\n");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("HOMELAB_LATCH_ENV"), "{}", err);

    // 2. Happy path: content arrives in memory, correct argv + cwd.
    std::env::set_var("HOMELAB_LATCH_ENV", "prod");
    std::env::remove_var("LATCH_STUB_MODE");
    let spec = homelab_client::spec::build_spec(&dir).unwrap();
    assert_eq!(
        spec.env.get("kyu").map(|s| s.as_str()),
        Some("KYU_TOKEN=stubtoken\nKYU_SECRET_KEY=stubkey\n"),
        "latch stdout becomes the app env, byte for byte"
    );
    let logged = std::fs::read_to_string(&log).unwrap();
    let line = logged.lines().last().unwrap();
    assert!(
        line.ends_with("|cat mbtest/kyu/.env --env prod --expand"),
        "pinned D10 interface, --expand included: {}",
        line
    );
    let cwd = line.split('|').next().unwrap();
    assert!(
        cwd.ends_with("/stacks"),
        "latch runs from the stacks root (the latch project): {}",
        cwd
    );
    // Plaintext-scan (standing rule 10): nothing latch produced may land on
    // the workstation disk — the stack tree holds no .env afterwards.
    assert!(
        !dir.join("kyu/.env").exists(),
        "no plaintext .env may be written"
    );

    // 3. Both sources for one app → refused, not silently preferred.
    write(&dir.join("kyu").join(".env"), "KYU_TOKEN=plain\n");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("BOTH"), "{}", err);
    std::fs::remove_file(dir.join("kyu/.env")).unwrap();

    // 4. latch fails → its stderr (which carries the remedy) reaches the user.
    std::env::set_var("LATCH_STUB_MODE", "fail");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("commit+push env files first"), "{}", err);

    // 5. Empty stdout is not a sealed env — refused with a remedy.
    std::env::set_var("LATCH_STUB_MODE", "empty");
    let err = homelab_client::spec::build_spec(&dir).unwrap_err();
    assert!(err.contains("empty content"), "{}", err);

    // 6. No latch_secrets declared → latch is never invoked at all.
    std::env::set_var("LATCH_STUB_MODE", "fail");
    let plain = stack_dir(&tmp.join("second"), "");
    write(&plain.join("kyu").join(".env"), "KYU_TOKEN=plain\n");
    let before = std::fs::read_to_string(&log).unwrap().lines().count();
    let spec = homelab_client::spec::build_spec(&plain).unwrap();
    assert_eq!(
        spec.env.get("kyu").map(|s| s.as_str()),
        Some("KYU_TOKEN=plain\n")
    );
    let after = std::fs::read_to_string(&log).unwrap().lines().count();
    assert_eq!(before, after, "plaintext-only stacks must not touch latch");

    // 7. A latch_secrets entry that names no real app is a typo, caught
    // before latch is ever invoked (also what keeps '__' out of the path:
    // manifest app names are validated [a-z0-9-]).
    std::env::remove_var("LATCH_STUB_MODE");
    let typo = stack_dir(&tmp.join("third"), "latch_secrets: [mailbx]\n");
    let err = homelab_client::spec::build_spec(&typo).unwrap_err();
    assert!(err.contains("no such app"), "{}", err);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// A typo in a stack file must be refused, not ignored.
///
/// `latch_secret:` instead of `latch_secrets:` used to parse cleanly, deploy
/// cleanly, and produce a container with no secrets in it. `gateway_routes:`
/// instead of `gateway_route:` produced a hostname with no route. Both are
/// the shape that cost the downloader its disks on 2026-08-31: a field the
/// reader did not recognise and dropped without a word.
#[test]
fn a_misspelled_key_in_a_stack_file_is_refused() {
    // Named for this test, not just the pid: tests in one binary share a
    // process id, and a sibling test was reaching into this directory.
    let tmp = std::env::temp_dir().join(format!(
        "homelab-typo-{}-{}",
        std::process::id(),
        "unknownfield"
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let dir = tmp.join("typo");
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(dir.join("app/docker-compose.yml"), "services: {}\n").unwrap();
    let base = "\
stack_name: typo
vmid: 150
hostname: 150-app-typo
network: {ip: 10.10.10.50/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}
resources: {cores: 1, memory_mb: 512, swap_mb: 256, disk_gb: 4, storage: local-lvm}
lxc: {template: 'clone:998', unprivileged: true, features: 'nesting=1', protection: false, gpu: false, vpn: false}
boot: {onboot: true, order: 50}
storage: []
apps: [app]
";
    // The correct spelling parses.
    std::fs::write(
        dir.join("lxc-compose.yml"),
        format!("{}latch_secrets: [app]\n", base),
    )
    .unwrap();
    let ok_spelling = format!("{:?}", homelab_client::spec::build_spec(&dir));
    assert!(
        !ok_spelling.to_lowercase().contains("unknown field"),
        "the correct spelling must not be refused as unknown: {}",
        ok_spelling
    );
    // The typo does not.
    std::fs::write(
        dir.join("lxc-compose.yml"),
        format!("{}latch_secret: [app]\n", base),
    )
    .unwrap();
    let err = format!("{:?}", homelab_client::spec::build_spec(&dir));
    assert!(
        err.contains("latch_secret") || err.to_lowercase().contains("unknown field"),
        "a misspelled key must be named and refused, got: {}",
        err
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
