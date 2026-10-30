//! The real `Executor`: runs commands and file operations on the Proxmox host
//! for the core operations (AR2). Everything the core decides is carried out here.

use crate::*;

/// restore-two-sources-1 (Kenny, 2026-10-22: "ik wil ook zelf kunnen kiezen
/// welke van de twee terug wordt gezet"): the backup settings pointed at the
/// repository set `source` names. "local" is the set a compose stack's
/// snapshot is written to first (`second_copy_dataset`); "remote" or none is
/// `restic_base`. The local settings carry no `local_dataset` of their own,
/// so a safety copy a restore takes first writes straight into the local
/// repository instead of copying it onto itself.
pub(crate) async fn backup_cfg_for_source(
    exec: &dyn Executor,
    base: homelab_core::ops::backup::BackupCfg,
    source: Option<&str>,
) -> Result<homelab_core::ops::backup::BackupCfg, String> {
    match source.map(str::trim) {
        None | Some("") | Some("remote") => Ok(base),
        Some("local") => {
            let ds = base.local_dataset.clone().ok_or_else(|| {
                "no local repository set is configured (`second_copy_dataset` in host.toml)"
                    .to_string()
            })?;
            let path = homelab_core::ops::secondcopy::local_base(exec, &ds)
                .await
                .map_err(|e| e.to_string())?;
            Ok(homelab_core::ops::backup::BackupCfg {
                restic_base: path,
                local_dataset: None,
                ..base
            })
        }
        Some(other) => Err(format!(
            "unknown backup source '{other}': it is local or remote"
        )),
    }
}

pub(crate) struct RealExecutor;

#[async_trait]
impl Executor for RealExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        // Transcript emission is handled by core's TracingExecutor inside the
        // pipeline; here we only trace at the log level for non-pipeline calls.
        let rendered = cmd.rendered();
        tracing::trace!("run {}", rendered);
        // fix-53 (expert panel, timeout-leaves-container-work-running,
        // 2026-09-27): the command leads a process group of its own, and a
        // timeout kills that whole group. `kill_on_drop` alone reached only
        // the direct child (`pct`), not the `lxc-attach` and script it had
        // started, so a "timed out" step went on changing the container.
        let child = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .map_err(|e| CoreError::Other(format!("spawn {}: {}", rendered, e)))?;
        let group = child.id();
        let out = match tokio::time::timeout(
            Duration::from_secs(cmd.timeout_s),
            child.wait_with_output(),
        )
        .await
        {
            Ok(out) => out.map_err(|e| CoreError::Other(format!("wait {}: {}", rendered, e)))?,
            Err(_) => {
                if let Some(pgid) = group.and_then(|p| i32::try_from(p).ok()) {
                    // SAFETY: killpg only sends a signal; the group is the one
                    // this call created for the child it spawned. The one
                    // exception to the workspace's `unsafe_code = "deny"`.
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::killpg(pgid, libc::SIGKILL);
                    }
                }
                return Err(CoreError::Timeout {
                    rendered: rendered.clone(),
                    seconds: cmd.timeout_s,
                });
            }
        };
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(CmdOutput {
            stdout,
            stderr,
            code: out.status.code().unwrap_or(-1),
        })
    }

    /// ask-kp11be-5: the same process rules as `run` (own process group,
    /// the group killed on a timeout), but stdout is read line by line
    /// while the command runs, so restic's status lines reach the job
    /// panel as they happen. What the callback consumes is not kept; what
    /// is kept is capped, so a chatty command cannot grow without bound.
    async fn run_streaming(
        &self,
        cmd: &Cmd,
        on_line: &homelab_core::executor::LineFn<'_>,
    ) -> Result<CmdOutput, CoreError> {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};
        /// The most stdout kept for the caller (4 MiB).
        const KEEP_MAX: usize = 4 * 1024 * 1024;
        let rendered = cmd.rendered();
        tracing::trace!("run (streaming) {}", rendered);
        let mut child = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0)
            .spawn()
            .map_err(|e| CoreError::Other(format!("spawn {}: {}", rendered, e)))?;
        let group = child.id();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let err_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut e) = stderr {
                // Capped like the kept stdout: read what fits, drop the rest.
                let _ = (&mut e).take(KEEP_MAX as u64).read_to_end(&mut buf).await;
                let _ = tokio::io::copy(&mut e, &mut tokio::io::sink()).await;
            }
            buf
        });
        let work = async {
            let mut kept = String::new();
            if let Some(o) = stdout {
                let mut rd = tokio::io::BufReader::new(o);
                let mut raw = Vec::new();
                loop {
                    raw.clear();
                    match rd.read_until(b'\n', &mut raw).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&raw);
                            let line = line.trim_end_matches(['\n', '\r']);
                            if !on_line(line) && kept.len() + line.len() < KEEP_MAX {
                                kept.push_str(line);
                                kept.push('\n');
                            }
                        }
                    }
                }
            }
            let status = child.wait().await;
            (kept, status)
        };
        // backup-stall: progress that stands still for the watch's limit
        // ends the command the way a timeout does (its whole group).
        let stall = cmd.stall.clone();
        let watch = async {
            match &stall {
                Some(s) => loop {
                    tokio::time::sleep(Duration::from_secs(s.limit_s.clamp(1, 5))).await;
                    if s.idle_s() >= s.limit_s {
                        break;
                    }
                },
                None => std::future::pending::<()>().await,
            }
        };
        let raced = async {
            tokio::select! {
                r = work => Some(r),
                () = watch => None,
            }
        };
        let kill_group = || {
            if let Some(pgid) = group.and_then(|p| i32::try_from(p).ok()) {
                // SAFETY: as in `run` — a signal to the group this call
                // created for the child it spawned.
                #[allow(unsafe_code)]
                unsafe {
                    libc::killpg(pgid, libc::SIGKILL);
                }
            }
        };
        match tokio::time::timeout(Duration::from_secs(cmd.timeout_s), raced).await {
            Ok(None) => {
                kill_group();
                Err(CoreError::Stalled {
                    rendered,
                    seconds: stall.map(|s| s.limit_s).unwrap_or(0),
                })
            }
            Ok(Some((kept, status))) => {
                let status =
                    status.map_err(|e| CoreError::Other(format!("wait {}: {}", rendered, e)))?;
                let stderr = err_task.await.unwrap_or_default();
                Ok(CmdOutput {
                    stdout: kept,
                    stderr: String::from_utf8_lossy(&stderr).to_string(),
                    code: status.code().unwrap_or(-1),
                })
            }
            Err(_) => {
                kill_group();
                Err(CoreError::Timeout {
                    rendered,
                    seconds: cmd.timeout_s,
                })
            }
        }
    }

    /// Atomic by contract (AR4): write a temp file, fsync, rename over.
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        use std::os::unix::fs::PermissionsExt;
        let path = path.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), CoreError> {
            let p = std::path::Path::new(&path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::State(e.to_string()))?;
            }
            // fix-51 (see REGISTER.md): a temp name of its own per write.
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let tmp = format!(
                "{}.{}.{}.tmp",
                path,
                std::process::id(),
                SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            {
                // Created with its final mode: a secret written into a file
                // that is world-readable until the chmod below is readable in
                // that window. `mode()` applies at creation (minus umask),
                // and the explicit set_permissions afterwards still fixes the
                // exact bits.
                use std::os::unix::fs::OpenOptionsExt;
                let _ = std::fs::remove_file(&tmp);
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(mode)
                    .open(&tmp)
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.write_all(content.as_bytes())
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.sync_all().map_err(|e| CoreError::State(e.to_string()))?;
            }
            let placed = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
                .and_then(|()| std::fs::rename(&tmp, &path));
            if let Err(e) = placed {
                let _ = std::fs::remove_file(&tmp);
                return Err(CoreError::State(e.to_string()));
            }
            // fix-51: the rename is only durable once the directory entry
            // is; without this a power cut can bring the old file back.
            if let Some(parent) = p.parent()
                && let Ok(d) = std::fs::File::open(parent)
            {
                let _ = d.sync_all();
            }
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Other(e.to_string()))?
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        // fix-50: absence and unreadability are different answers; the state
        // store may only treat the first as a fresh install.
        tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::NotFound(format!("{}: {}", path, e))
            } else {
                CoreError::State(format!("{}: {}", path, e))
            }
        })
    }

    async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

// ── Sink + journal adapters ─────────────────────────────────────────────────
