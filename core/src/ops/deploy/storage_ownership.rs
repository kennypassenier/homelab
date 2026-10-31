//! Storage ownership: can the app actually write its own data?
//!
//! This is the fifth version of this step in one afternoon, and the first
//! one that asks the right question. The four before it tried to work out
//! WHO SHOULD own the directory, and each was confidently wrong on a shape
//! nobody had thought of — an empty user field with PUID, the name
//! "nobody", an entrypoint that drops privileges after starting, and a
//! supervisor whose PID 1 is root while the application runs beside it as
//! a child. Every wrong answer was printed as a `chown` to copy, and each
//! one would have broken the service it was about.
//!
//! The question that has no shapes is whether the application can write.
//! It is also the only thing anyone actually cares about: Loki did not
//! crash because a number was wrong, it crashed because it could not open
//! its own database.
//!
//! So the container is asked to write one file and delete it again. No
//! uid arithmetic, no image conventions, no fifth special case — and when
//! it fails, the message reports what IS rather than prescribing a fix
//! that might be as wrong as the last four.

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok};
use crate::manifest::MountSpec;
use crate::runner::StepOutcome;

use super::run::{Run, log_warn};

impl Run<'_, '_> {
    pub(super) async fn storage_ownership(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, _) = self.parts();
        let mut wrong: Vec<String> = Vec::new();
        for mount in &m.storage {
            let Some(app) = mount.app.as_ref() else {
                continue;
            };
            if !m.apps.contains(app) {
                continue;
            }
            wrong.extend(self.check_writable(app, mount).await);
        }
        if wrong.is_empty() {
            Ok(StepOutcome::Unchanged)
        } else {
            Err(CoreError::Command {
                rendered: format!("storage ownership {}", m.stack_name),
                detail: wrong.join("; "),
            })
        }
    }

    /// `Some(finding)` when the app provably cannot write its mount.
    async fn check_writable(&self, app: &str, mount: &MountSpec) -> Option<String> {
        let (ctx, _, _, _) = self.parts();
        // Where does this host path appear inside the app's container?
        // Docker's own mapping, so no path convention has to be guessed.
        let dest = self.mounted_at(app, mount).await;
        if dest.is_empty() {
            // The app does not mount this directory. It belongs to a
            // sibling, or the container is not running — either way there
            // is nothing here to judge.
            return None;
        }
        if !self.is_running(app, mount).await {
            return None;
        }
        let probe = self.write_probe(app, &dest).await;
        // A probe that cannot RUN says nothing about the data. The
        // cloudflared image is distroless and has no shell at all, so the
        // exec fails before it ever reaches the directory — reporting
        // that as "cannot write" is the same mistake as counting a 403 as
        // healthy, in the opposite direction. Same rule as the service
        // checks: unreadable is reported, never treated as failure.
        let out = match &probe {
            Ok(o) => format!("{}{}", o.stdout, o.stderr),
            Err(e) => e.to_string(),
        };
        if Self::unprobeable(&out) {
            log_warn(
                ctx,
                format!(
                    "[storage] '{}' has no shell to probe with, so whether it \
                     can write {} is unknown — not assumed to be fine",
                    app, mount.host_path
                ),
            );
            return None;
        }
        let failed = probe.map(|o| !o.success()).unwrap_or(true);
        if !failed {
            return None;
        }
        Some(self.ownership_finding(app, mount, &dest).await)
    }

    async fn write_probe(
        &self,
        app: &str,
        dest: &str,
    ) -> Result<crate::executor::CmdOutput, CoreError> {
        let (_, _, m, exec) = self.parts();
        pct_sh(
            exec,
            m.vmid,
            &format!(
                "docker exec {} sh -c 'touch {}/.homelab-write-probe && rm -f {}/.homelab-write-probe' 2>&1",
                app, dest, dest
            ),
            120,
        )
        .await
    }

    /// When it fails, the message reports what IS rather than prescribing
    /// a fix.
    async fn ownership_finding(&self, app: &str, mount: &MountSpec, dest: &str) -> String {
        let exec = self.exec;
        let owner = run_ok(
            exec,
            &Cmd::new("stat", &["-c", "%u:%g", &mount.host_path], 30),
        )
        .await
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_else(|_| "unreadable".into());
        format!(
            "'{}' cannot write to {} (mounted at {} inside it); the \
             directory belongs to {} on the host — compare that with \
             the user the application runs as, which is not always \
             the container's PID 1",
            app, mount.host_path, dest, owner
        )
    }

    fn unprobeable(out: &str) -> bool {
        out.contains("executable file not found")
            || out.contains("OCI runtime exec failed")
            || out.contains("no such file or directory: unknown")
            || out.contains("is restarting")
            || out.contains("is not running")
            || out.contains("is paused")
    }

    /// The path inside the container where `mount` appears; empty if it does not.
    async fn mounted_at(&self, app: &str, mount: &MountSpec) -> String {
        let (_, _, m, exec) = self.parts();
        pct_sh(
            exec,
            m.vmid,
            &format!(
                "docker inspect {} -f '{{{{range .Mounts}}}}{{{{if eq .Source \"{}\"}}}}{{{{.Destination}}}}{{{{end}}}}{{{{end}}}}' 2>/dev/null || true",
                app, mount.mount_point
            ),
            60,
        )
        .await
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default()
    }

    /// T81 (F289): establish that the probe CAN run before reading
    /// anything into its result. A container that is restarting
    /// fails `docker exec` before it reaches the directory, and the
    /// first JobTracker deploy reported that as "cannot write … the
    /// directory belongs to 110001" — a hypothesis printed as a
    /// finding. Not running is reported as unmeasured, never as a
    /// permissions fault.
    async fn is_running(&self, app: &str, mount: &MountSpec) -> bool {
        let (ctx, _, m, exec) = self.parts();
        let status = pct_sh(
            exec,
            m.vmid,
            &format!(
                "docker inspect {} -f '{{{{.State.Status}}}}' 2>/dev/null || echo unknown",
                app
            ),
            30,
        )
        .await
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
        if status == "running" {
            return true;
        }
        log_warn(
            ctx,
            format!(
                "[storage] '{}' is {} — whether it can write {} could not be \
                 measured, so nothing about its ownership is claimed",
                app,
                if status.is_empty() {
                    "in an unknown state"
                } else {
                    status.as_str()
                },
                mount.host_path
            ),
        );
        false
    }
}
