//! Stopping a stack's labelled containers for the snapshot, and starting them.

use crate::error::CoreError;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

use super::job::Job;

impl Job<'_, '_> {
    /// Quiesce: stop containers labeled com.homelab.backup.pause=true, and
    /// REMEMBER WHICH ONES.
    ///
    /// This used to stop by label and resume by the manifest's `apps` list,
    /// and the two are not the same set. On 2026-08-31 the metrics stack's
    /// nightly backup stopped prometheus and alertmanager — both labelled —
    /// and resumed prometheus, promtail and pve-exporter, because host state
    /// still held the app list from before alertmanager was added. The
    /// snapshot then failed on a stale path, and Alertmanager stayed down for
    /// six hours. A backup that can leave a service off is worse than a
    /// backup that fails, so what is paused is now what is resumed, by name.
    pub(super) async fn quiesce(&self) -> Result<StepOutcome, CoreError> {
        let script = "docker ps --filter label=com.homelab.backup.pause=true --format '{{.Names}}'";
        let out = util_pct_sh(self.exec, self.m.vmid, script, 60).await?;
        let names: Vec<String> = out
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if names.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let stop = format!("docker stop {}; true", names.join(" "));
        let _ = util_pct_sh(self.exec, self.m.vmid, &stop, 120).await?;
        if let Ok(mut g) = self.paused.lock() {
            *g = names;
        }
        Ok(StepOutcome::Changed)
    }

    /// Resume the paused containers — unconditionally (the step runs even
    /// when the snapshot failed).
    pub(super) async fn resume(&self) -> Result<StepOutcome, CoreError> {
        // Exactly what quiesce stopped, by name — this is the half that must
        // not depend on any list that can go stale.
        let names = self.paused.lock().map(|g| g.clone()).unwrap_or_default();
        if !names.is_empty() {
            let start = format!("docker start {}; true", names.join(" "));
            let _ = util_pct_sh(self.exec, self.m.vmid, &start, 300).await?;
        }
        // Then the declared apps, which also brings back anything that was
        // down for an unrelated reason. Belt and braces.
        let dir_cmds = self
            .m
            .apps
            .iter()
            .map(|a| {
                crate::ops::util::app_dir_script(&self.m.stack_name, a)
                    .raw("docker compose up -d")
                    .build()
            })
            .collect::<Vec<_>>()
            .join("; ");
        if !dir_cmds.is_empty() {
            let script = format!("{}; true", dir_cmds);
            let _ = util_pct_sh(self.exec, self.m.vmid, &script, 300).await?;
        }
        Ok(StepOutcome::Changed)
    }
}
