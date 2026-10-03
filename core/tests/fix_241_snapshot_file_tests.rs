//! fix-241 (2026-10-02, Jellyfin 12.1): one file could not be read from a
//! snapshot without a live restore. `read_snapshot_file` reads it with
//! `restic dump` to stdout, capped at 1 MiB, and never writes anything:
//! not next to live data, not anywhere.
//!
//! Two halves: the mock proves which commands run (one `restic dump`, no
//! restore, no file written); a real subprocess with a stub `restic` on PATH
//! proves the shell around it — the cap, the exit codes, the bytes — and
//! that nothing appears in the working directory or the "live" directory.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use base64::Engine;
use homelab_core::executor::{Cmd, CmdOutput, MockExecutor};
use homelab_core::ops::backup::{
    BackupCfg, SNAPSHOT_FILE_CAP, read_snapshot_file, resolve_snapshot_path, snapshot_file_cmd,
};

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// covers: fix-241
#[tokio::test]
async fn fix_241_reads_with_one_restic_dump_and_writes_nothing() {
    let exec = MockExecutor::new();
    exec.respond_always("restic dump", CmdOutput::ok(&b64(b"<x>vaapi</x>\n")));
    let f = read_snapshot_file(
        &exec,
        &BackupCfg::default(),
        "jellyfin",
        "af364ed7",
        "/appdata/media/jellyfin-config/config/encoding.xml",
    )
    .await
    .unwrap();
    assert_eq!(f.text.as_deref(), Some("<x>vaapi</x>\n"));
    assert!(!f.truncated && !f.binary);
    assert_eq!(f.cap_bytes, SNAPSHOT_FILE_CAP as u64);
    let calls = exec.calls();
    assert_eq!(calls.len(), 1, "one command, the dump: {:?}", calls);
    assert_eq!(exec.ran("restic", &["dump"]), 1, "{:?}", calls);
    for verb in ["restore", "rm", "tar", "pct", "docker", "mv", "cp"] {
        assert_eq!(exec.ran(verb, &[]), 0, "{} ran: {:?}", verb, calls);
    }
    assert!(
        exec.file_paths().is_empty(),
        "a read wrote files: {:?}",
        exec.file_paths()
    );
}

/// covers: fix-241
#[test]
fn fix_241_refuses_what_could_ride_in_as_a_flag_or_leave_the_snapshot() {
    let cfg = BackupCfg::default();
    assert!(snapshot_file_cmd(&cfg, "jellyfin", "--target=/appdata", "/a/b").is_err());
    assert!(snapshot_file_cmd(&cfg, "jellyfin", "latest", "relative/path").is_err());
    assert!(snapshot_file_cmd(&cfg, "jellyfin", "latest", "/a/../../etc/passwd").is_err());
    assert!(snapshot_file_cmd(&cfg, "jelly fin", "latest", "/a/b").is_err());
    assert!(snapshot_file_cmd(&cfg, "jellyfin", "af364ed7", "/a/b").is_ok());
    let c = snapshot_file_cmd(&cfg, "jellyfin", "latest", "/a/b").unwrap();
    assert!(c.quiet, "a settings file can hold a secret: never echoed");
}

/// covers: fix-241
#[test]
fn fix_241_a_typed_path_resolves_inside_the_apps_own_directory() {
    let one = vec!["/appdata/media/jellyfin-config".to_string()];
    assert_eq!(
        resolve_snapshot_path(&one, "config/encoding.xml").unwrap(),
        "/appdata/media/jellyfin-config/config/encoding.xml"
    );
    assert_eq!(
        resolve_snapshot_path(&one, "/appdata/media/jellyfin-config/config/encoding.xml").unwrap(),
        "/appdata/media/jellyfin-config/config/encoding.xml"
    );
    assert!(resolve_snapshot_path(&one, "/etc/shadow").is_err());
    assert!(resolve_snapshot_path(&one, "/appdata/media/jellyfin-configX/a").is_err());
    assert!(resolve_snapshot_path(&one, "../sonarr-config/config.xml").is_err());
    let two = vec!["/a/one".to_string(), "/a/two".to_string()];
    let e = resolve_snapshot_path(&two, "x.conf").unwrap_err();
    assert!(e.contains("/a/one") && e.contains("/a/two"), "{}", e);
    assert!(resolve_snapshot_path(&[], "x").is_err());
}

// ── the real shell around restic, with a stub restic ─────────────────────

/// A `restic` that answers by `$STUB_MODE` and logs its argv to `$STUB_LOG`.
fn install_stub(bin: &Path) {
    std::fs::create_dir_all(bin).unwrap();
    let p = bin.join("restic");
    let mut f = std::fs::File::create(&p).unwrap();
    writeln!(
        f,
        "#!/bin/bash\necho \"$*\" >> \"$STUB_LOG\"\ncase \"$STUB_MODE\" in\n\
         text) printf 'HardwareAccelerationType=vaapi\\n';;\n\
         binary) printf 'SQLite format 3\\000\\001\\002';;\n\
         big) head -c 3145728 /dev/zero | tr '\\0' 'a';;\n\
         missing) echo 'path /x not found in snapshot' >&2; exit 1;;\n\
         esac"
    )
    .unwrap();
    drop(f);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run_real(cmd: &Cmd, bin: &Path, scratch: &Path, log: &Path, mode: &str) -> CmdOutput {
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = std::process::Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(scratch)
        .env("PATH", path)
        .env("STUB_MODE", mode)
        .env("STUB_LOG", log)
        .output()
        .unwrap();
    CmdOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn listing(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap_or_default(),
            )
        })
        .collect();
    v.sort();
    v
}

/// covers: fix-241
#[test]
fn fix_241_the_real_command_caps_reads_only_and_writes_nowhere() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let scratch = tmp.path().join("scratch");
    let live = tmp.path().join("live");
    let log = tmp.path().join("argv.log");
    install_stub(&bin);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("encoding.xml"), "the live settings\n").unwrap();
    let live_before = listing(&live);

    let cfg = BackupCfg::default();
    let path = format!("{}/encoding.xml", live.display());
    let cmd = snapshot_file_cmd(&cfg, "jellyfin", "latest", &path).unwrap();
    let parse = |out: &CmdOutput| {
        homelab_core::ops::backup::parse_snapshot_file("jellyfin", "latest", &path, out)
    };

    // A text file comes back whole.
    let f = parse(&run_real(&cmd, &bin, &scratch, &log, "text")).unwrap();
    assert_eq!(f.text.as_deref(), Some("HardwareAccelerationType=vaapi\n"));
    assert!(!f.truncated && !f.binary);

    // A database is binary: no text, said so.
    let f = parse(&run_real(&cmd, &bin, &scratch, &log, "binary")).unwrap();
    assert!(f.binary && f.text.is_none(), "{:?}", f);

    // 3 MiB is cut at the cap, and the cut is said, not an error.
    let f = parse(&run_real(&cmd, &bin, &scratch, &log, "big")).unwrap();
    assert!(f.truncated, "{:?}", f.shown_bytes);
    assert_eq!(f.shown_bytes, SNAPSHOT_FILE_CAP as u64);
    assert_eq!(f.text.as_ref().map(String::len), Some(SNAPSHOT_FILE_CAP));

    // restic's own failure is the answer, with its words.
    let e = parse(&run_real(&cmd, &bin, &scratch, &log, "missing")).unwrap_err();
    assert!(e.to_string().contains("not found in snapshot"), "{}", e);

    // restic was only ever asked to dump, with exactly the snapshot and path.
    let argv = std::fs::read_to_string(&log).unwrap();
    for line in argv.lines() {
        assert_eq!(line, format!("dump latest {}", path));
    }
    assert_eq!(argv.lines().count(), 4);

    // Nothing written where it ran, nothing changed in the live directory.
    assert!(
        listing(&scratch).is_empty(),
        "the read wrote into its working directory: {:?}",
        listing(&scratch).iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    assert_eq!(listing(&live), live_before, "the live directory changed");
}

/// The media stack as the repository declares it, as host state would hold
/// it after a deploy.
fn state_with_media() -> homelab_core::state::HostState {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/media/lxc-compose.yml");
    let m: homelab_core::manifest::StackManifest =
        serde_yaml::from_str(&std::fs::read_to_string(root).unwrap()).unwrap();
    serde_json::from_value(serde_json::json!({
        "stacks": { "media": {
            "vmid": m.vmid, "hostname": m.hostname, "apps": m.apps,
            "applied_at": 1, "manifest": m
        } }
    }))
    .unwrap()
}

/// covers: fix-241
#[test]
fn fix_241_the_host_resolves_a_typed_path_against_the_stack_in_its_state() {
    use homelab_core::ops::backup::snapshot_file_target;
    let hs = state_with_media();
    assert_eq!(
        snapshot_file_target(&hs, "media", "jellyfin", "config/encoding.xml").unwrap(),
        "/appdata/media/jellyfin-config/config/encoding.xml"
    );
    // Another app's directory is not reachable through this app's name.
    assert!(
        snapshot_file_target(
            &hs,
            "media",
            "jellyfin",
            "/appdata/media/sonarr-config/config.xml"
        )
        .is_err()
    );
    let e = snapshot_file_target(&hs, "media", "nosuchapp", "x").unwrap_err();
    assert!(e.contains("jellyfin") && e.contains("sonarr"), "{}", e);
    assert!(snapshot_file_target(&hs, "nosuchstack", "jellyfin", "x").is_err());
}
