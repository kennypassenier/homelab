//! corr-rogue-daemon (2026-09-27): `homelab-host --version`, run by hand on
//! pve to read the version, ignored the flag and started a second daemon. It
//! took port 8443 while systemd restarted the service after a self-update,
//! and the real unit failed five times with AddrInUse. An argument the
//! daemon does not know must stop it before it opens anything.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run the host binary with `args`; fail if it is still running after a few
/// seconds (it would be serving). Returns (exit code, stdout, stderr).
fn run(args: &[&str]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_homelab-host"))
        .args(args)
        // A config path that cannot exist, so a daemon that did start would
        // not read the operator's real settings.
        .env("HOMELAB_CONFIG", "/nonexistent/homelab-host-test.toml")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn homelab-host");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            let out = child.wait_with_output().expect("output");
            return (
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            );
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("homelab-host {args:?} was still running after 5 s: it started serving");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn corr_rogue_daemon_version_prints_the_version_and_exits() {
    let (code, out, _) = run(&["--version"]);
    assert_eq!(code, 0);
    // fix-141: the build follows the version, in brackets.
    assert!(out.trim().starts_with(env!("CARGO_PKG_VERSION")), "{out}");
}

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): a
/// hand-built binary and the release both said "3.59.3". `--version` names
/// the build (`git describe --dirty`); `--selfcheck`, which the self-update
/// reads, stays the bare version.
#[test]
fn fix_141_version_names_the_build_it_was_made_from() {
    let build = env!("HOMELAB_BUILD");
    assert!(!build.is_empty());
    let (code, out, _) = run(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(
        out.trim(),
        format!("{} ({})", env!("CARGO_PKG_VERSION"), build)
    );
}

#[test]
fn corr_rogue_daemon_an_unknown_argument_is_refused_before_serving() {
    let (code, _, err) = run(&["--bogus"]);
    assert_eq!(code, 2, "stderr: {err}");
    assert!(
        err.contains("--bogus"),
        "the refusal names the argument: {err}"
    );
}

#[test]
fn corr_rogue_daemon_selfcheck_still_answers() {
    let (code, out, _) = run(&["--selfcheck"]);
    assert_eq!(code, 0);
    assert_eq!(out.trim(), env!("CARGO_PKG_VERSION"));
}
