//! S2 · the final reconciliation.
//!
//! Every step before it has said it succeeded. This asks the container
//! itself whether that is true, and it asks about the whole stack rather
//! than about one step's own work.
//!
//! The reason it is a separate pass and not more assertions inside the
//! steps: a step can only check what it did. What hurt on 2026-09-01 was
//! never one step being wrong — it was the deploy stopping halfway and
//! everything after it silently not happening. Only something that looks
//! at the finished whole can notice that.

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok};
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn reconcile(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        let mut wrong: Vec<String> = Vec::new();
        if pct_sh(exec, m.vmid, "true", 30).await.is_err() {
            wrong.push("the container did not answer".into());
        }
        let live = run_ok(exec, &Cmd::new("pct", &["config", &vm], 60))
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
        self.check_config(&live, &mut wrong);
        if !m.native_only {
            self.check_apps_running(&mut wrong).await;
        }
        self.check_natives_active(&mut wrong).await;
        if !wrong.is_empty() {
            return Err(CoreError::Command {
                rendered: format!("reconcile {}", m.stack_name),
                detail: format!(
                    "the deploy reported success but the container does not match \
                     the stack file: {}",
                    wrong.join("; ")
                ),
            });
        }
        log_info(
            ctx,
            format!(
                "[reconcile] the container matches the stack file on {} point(s)",
                2 + m.storage.len() + m.data_mounts.len() + m.apps.len() + m.natives.len()
            ),
        );
        Ok(StepOutcome::Unchanged)
    }

    /// Hostname, mounts and boot policy against `pct config`.
    fn check_config(&self, live: &str, wrong: &mut Vec<String>) {
        let (_, _, m, _) = self.parts();
        // Hostname: A2 refuses a mismatch before touching anything, so a
        // mismatch here means the container was swapped under us.
        if !live.contains(&format!("hostname: {}", m.hostname)) {
            wrong.push(format!("hostname is not {}", m.hostname));
        }
        // Every declared mount, storage and borrowed alike. F118 lost these
        // by dropping a field it did not understand, and the deploy reported
        // success while the downloader came up with no disks.
        for st in &m.storage {
            if !live.contains(&format!("{},mp=", st.host_path)) {
                wrong.push(format!("storage {} is not mounted", st.host_path));
            }
        }
        for dm in &m.data_mounts {
            if !live.contains(&format!("{},mp={}", dm.host_path, dm.mount_point)) {
                wrong.push(format!("data mount {} is not mounted", dm.host_path));
            }
        }
        // Boot policy, read with W3's own parser rather than by looking for a
        // line. `pct config` prints nothing at all when onboot is 0, so a
        // text match on "onboot: 0" can never succeed and a stack that asks
        // for onboot: false would be reported wrong forever.
        // Only when the value can actually be read. W3 leaves an unreadable
        // boot policy alone, and a check that demands more than the deploy is
        // willing to enforce turns into a deploy that can never succeed.
        let live_boot = crate::ops::reconcile::parse(live);
        if let Some(on) = live_boot.onboot
            && on != m.boot.onboot
        {
            wrong.push(format!(
                "boot policy is {} where the stack file says {}",
                on, m.boot.onboot
            ));
        }
    }

    /// Every app actually running. `docker compose up -d` returning 0 is
    /// not the same as a container that stayed up: a crash loop exits 0
    /// on the way in.
    ///
    /// Asked per app directory rather than by container name. An app is a
    /// compose project, and a project may define several services with
    /// names of their own: `stacks/registry/registry` runs cache-dockerhub,
    /// cache-gcr, cache-ghcr and cache-lscr, and not one of them is called
    /// `registry`. Matching names reported that healthy stack as broken.
    async fn check_apps_running(&self, wrong: &mut Vec<String>) {
        let (_, _, m, exec) = self.parts();
        for app in &m.apps {
            // fix-132/fix-133: JSON + the Unknown state, so a probe that
            // could not be read is its own finding rather than silently
            // the same "no running service" as an actually dead app.
            match crate::ops::util::compose_running_services(exec, m.vmid, &m.stack_name, app, "")
                .await
                .unwrap_or(None)
            {
                Some(running) if !running.is_empty() => {}
                Some(_) => wrong.push(format!("app '{}' has no running service", app)),
                None => wrong.push(format!(
                    "app '{}' :: could not read `docker compose ps`",
                    app
                )),
            }
        }
    }

    /// Every native unit actually active.
    async fn check_natives_active(&self, wrong: &mut Vec<String>) {
        let (_, _, m, exec) = self.parts();
        for n in &m.natives {
            let act = pct_sh(
                exec,
                m.vmid,
                &format!("systemctl is-active {} 2>/dev/null || true", n),
                60,
            )
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
            if !act.trim().starts_with("active") {
                wrong.push(format!("native unit '{}' is not active", n));
            }
        }
    }
}
