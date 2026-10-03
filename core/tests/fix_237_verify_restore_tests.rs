//! fix-237 (2026-10-02, before the Jellyfin 12 upgrade): a stack snapshot
//! could not be proved restorable without touching live data. The nightly
//! drill rotates and does not say which repository it proved; `homelab
//! restore` writes the live data. `verify_restore_one` restores one named
//! snapshot (or `latest`) of one repository into the drill's scratch
//! directory with the drill's own restore-and-judge code, reports the files
//! and the size, and empties the scratch directory before and after.
//!
//! The real shell runs with a stub `restic` on PATH that writes what a
//! restore writes, so the test sees every file the run leaves anywhere.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor, MockExecutor};
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::restoredrill::{
    verify_restore_one, verify_restore_owners, verify_restore_target,
};

/// Runs every command for real, with the stub directory first on PATH.
struct RealExec {
    bin: PathBuf,
    log: PathBuf,
    mode: &'static str,
}

#[async_trait]
impl Executor for RealExec {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = std::process::Command::new(&cmd.program)
            .args(&cmd.args)
            .env("PATH", path)
            .env("STUB_MODE", self.mode)
            .env("STUB_LOG", &self.log)
            .output()
            .map_err(|e| CoreError::Other(e.to_string()))?;
        Ok(CmdOutput {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        })
    }
    async fn write_file(&self, _: &str, _: &str, _: u32) -> Result<(), CoreError> {
        Err(CoreError::Other(
            "a verify-restore writes no file itself".into(),
        ))
    }
    async fn read_file(&self, _: &str) -> Result<String, CoreError> {
        Err(CoreError::NotFound("none".into()))
    }
    async fn sleep_ms(&self, _: u64) {}
}

/// A `restic` that logs its argv and, on `restore <snap> --target <dir>`,
/// writes two files where restic would: under the target, at the absolute
/// path the snapshot holds.
fn install_stub(bin: &Path) {
    std::fs::create_dir_all(bin).unwrap();
    let p = bin.join("restic");
    let mut f = std::fs::File::create(&p).unwrap();
    writeln!(
        f,
        "#!/bin/bash\necho \"$*\" >> \"$STUB_LOG\"\n\
         [ \"$1\" = restore ] && [ \"$3\" = --target ] || exit 3\n\
         d=\"$4/appdata/demo/app-config/config\"; mkdir -p \"$d\"\n\
         case \"$STUB_MODE\" in\n\
         good) printf 'setting=1\\n' > \"$d/settings.xml\"; head -c 4096 /dev/zero > \"$d/blob.bin\";;\n\
         empty) : > \"$d/settings.xml\";;\n\
         fail) echo 'no snapshot found' >&2; exit 1;;\n\
         esac"
    )
    .unwrap();
    drop(f);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Every file and directory under `root`, with each file's bytes, skipping
/// the stub's own argv log.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == "argv.log") {
                continue;
            }
            let rel = p.strip_prefix(root).unwrap().display().to_string();
            if p.is_dir() {
                out.push((format!("{}/", rel), Vec::new()));
                stack.push(p);
            } else {
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

struct Sandbox {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    scratch: PathBuf,
}

fn sandbox() -> Sandbox {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    install_stub(&root.join("bin"));
    let scratch = root.join("scratch");
    // The nightly drill's own directory, which this must leave alone.
    std::fs::create_dir_all(scratch.join("restore-drill")).unwrap();
    std::fs::write(scratch.join("restore-drill/keep"), "drill").unwrap();
    // Live data next to it, which this must never write.
    std::fs::create_dir_all(root.join("live/app-config/config")).unwrap();
    std::fs::write(root.join("live/app-config/config/settings.xml"), "live\n").unwrap();
    Sandbox {
        _tmp: tmp,
        root,
        scratch,
    }
}

fn exec(sb: &Sandbox, mode: &'static str) -> RealExec {
    RealExec {
        bin: sb.root.join("bin"),
        log: sb.root.join("argv.log"),
        mode,
    }
}

/// covers: fix-237
#[tokio::test]
async fn fix_237_restores_into_scratch_only_reports_and_empties_it_after() {
    let sb = sandbox();
    let scratch = sb.scratch.to_str().unwrap().to_string();
    // A leftover of an earlier, interrupted verify-restore is emptied first.
    let target = verify_restore_target(&scratch, "app");
    let before = tree(&sb.root);
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(format!("{}/stale", target), "old").unwrap();

    let r = verify_restore_one(
        &exec(&sb, "good"),
        &BackupCfg::default(),
        "app",
        "af364ed7",
        &scratch,
    )
    .await
    .unwrap();
    assert!(r.passed, "{:?}", r);
    assert_eq!(r.owner, "app");
    assert_eq!(r.snapshot, "af364ed7");
    assert_eq!(r.files, 2, "{:?}", r);
    assert_eq!(r.bytes, 10 + 4096, "{:?}", r);
    assert_eq!(r.largest_bytes, 4096);

    // restic was asked for exactly that snapshot, into the scratch target.
    let argv = std::fs::read_to_string(sb.root.join("argv.log")).unwrap();
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        vec![format!("restore af364ed7 --target {}", target)]
    );
    assert!(target.starts_with(&format!("{}/", scratch)), "{}", target);

    // Nothing anywhere changed: the restore is gone, the stale leftover
    // too, the drill's directory and the live data are as they were.
    assert_eq!(
        tree(&sb.root),
        before,
        "a verify-restore left something behind or wrote outside its scratch directory"
    );
}

/// covers: fix-237
#[tokio::test]
async fn fix_237_a_restore_that_proves_nothing_fails_and_still_empties_scratch() {
    let sb = sandbox();
    let scratch = sb.scratch.to_str().unwrap().to_string();
    let before = tree(&sb.root);
    for mode in ["empty", "fail"] {
        let r = verify_restore_one(
            &exec(&sb, mode),
            &BackupCfg::default(),
            "app",
            "latest",
            &scratch,
        )
        .await
        .unwrap();
        assert!(!r.passed, "{}: {:?}", mode, r);
        assert!(r.problem.is_some(), "{}: {:?}", mode, r);
        assert_eq!(tree(&sb.root), before, "{}: scratch not emptied", mode);
    }
}

/// covers: fix-237
#[tokio::test]
async fn fix_237_refuses_a_scratch_directory_or_name_that_could_reach_live_data() {
    let exec = MockExecutor::new();
    let cfg = BackupCfg::default();
    for (owner, snap, scratch) in [
        ("app", "latest", "/"),
        ("app", "latest", ""),
        ("app", "latest", "relative/scratch"),
        ("app", "latest", "/appdata/.restore-scratch/../media"),
        ("../media", "latest", "/appdata/.restore-scratch"),
        ("app/..", "latest", "/appdata/.restore-scratch"),
        ("app", "--target=/appdata", "/appdata/.restore-scratch"),
    ] {
        assert!(
            verify_restore_one(&exec, &cfg, owner, snap, scratch)
                .await
                .is_err(),
            "{} {} {}",
            owner,
            snap,
            scratch
        );
    }
    assert!(exec.calls().is_empty(), "{:?}", exec.calls());
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

/// covers: fix-237
#[test]
fn fix_237_which_repositories_a_verify_restore_takes() {
    let hs = state_with_media();
    let all = verify_restore_owners(&hs, "media", None, "latest").unwrap();
    assert!(all.len() > 1, "{:?}", all);
    assert!(all.contains(&"jellyfin".to_string()), "{:?}", all);
    assert_eq!(
        verify_restore_owners(&hs, "media", Some("jellyfin"), "af364ed7").unwrap(),
        vec!["jellyfin".to_string()]
    );
    // A snapshot id belongs to one repository: without --app it is ambiguous.
    let e = verify_restore_owners(&hs, "media", None, "af364ed7").unwrap_err();
    assert!(e.contains("--app") && e.contains("jellyfin"), "{}", e);
    let e = verify_restore_owners(&hs, "media", Some("nosuchapp"), "latest").unwrap_err();
    assert!(e.contains("jellyfin"), "{}", e);
    assert!(verify_restore_owners(&hs, "nosuchstack", None, "latest").is_err());
}
