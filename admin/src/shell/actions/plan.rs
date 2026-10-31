//! one-process-batch: the step plan of a batch (every member's steps),
//! made before its first job starts, and the facts the plans read.

use std::collections::BTreeMap;
use std::time::Duration;

use homelab_proto::Command;

use super::{Actions, JobView};
use crate::core::actions::{self, ActionKind, ActionRequest, Material};
use crate::core::actions_progress::{self, FactsNeeded, MemberPlan, PlanFacts};

impl Actions {
    /// one-process-batch-2: read what `commands`' step plans depend on besides
    /// the commands themselves (`actions_progress::facts_needed`): the native
    /// services a stack's files declare, the stack's restic repositories, the
    /// host's managed stacks (newest fleet reading), and host.toml's
    /// `zfs_jobs` and `device_backups`. A fact that cannot be read is left
    /// out, and the commands that need it are then estimated and named so.
    pub(super) async fn plan_facts(&self, commands: &[Command]) -> PlanFacts {
        let need = actions_progress::facts_needed(commands);
        let mut facts = PlanFacts {
            natives: self.read_natives(&need).await,
            ..PlanFacts::default()
        };
        self.read_repositories(&need, &mut facts).await;
        if need.fleet
            && let Ok(s) = self.inner.shared.try_read()
            && let Some(fleet) = s.fleet.as_ref()
        {
            facts.fleet_stacks = Some(fleet.stacks.iter().map(|x| x.name.clone()).collect());
        }
        if need.zfs_jobs || need.devices {
            self.read_host_config(&need, &mut facts).await;
        }
        facts
    }

    /// The stacks whose native services are read: the native actions' own,
    /// and the native stacks among those verify-restore names.
    async fn read_natives(
        &self,
        need: &FactsNeeded,
    ) -> BTreeMap<String, Vec<homelab_proto::NativeServiceManifest>> {
        let mut native_stacks = need.natives.clone();
        for st in &need.repositories {
            if self.fleet_native(st) == Some(true) && !native_stacks.contains(st) {
                native_stacks.push(st.clone());
            }
        }
        if native_stacks.is_empty() {
            return BTreeMap::new();
        }
        let files = self.inner.files.clone();
        tokio::task::spawn_blocking(move || {
            let mut out = BTreeMap::new();
            for stack in native_stacks {
                let units = files.native_units(&stack);
                let read: Vec<_> = units
                    .iter()
                    .filter_map(|u| files.native_release(&stack, Some(u)).ok())
                    .map(|(m, _, _)| m)
                    .collect();
                // Every declared unit read, or the stack stays unread.
                if !units.is_empty() && read.len() == units.len() {
                    out.insert(stack, read);
                }
            }
            out
        })
        .await
        .unwrap_or_default()
    }

    /// verify-restore: the repositories a stack keeps.
    async fn read_repositories(&self, need: &FactsNeeded, facts: &mut PlanFacts) {
        for stack in &need.repositories {
            match self.fleet_native(stack) {
                Some(true) => {
                    if let Some(ms) = facts.natives.get(stack) {
                        facts.repositories.insert(
                            stack.clone(),
                            homelab_core::ops::restoredrill::backed_up_units(ms),
                        );
                    }
                }
                Some(false) => {
                    if let Some(owners) = self.compose_repositories(stack).await {
                        facts.repositories.insert(stack.clone(), owners);
                    }
                }
                None => {}
            }
        }
    }

    /// The restic owner groups a compose stack's manifest declares.
    async fn compose_repositories(&self, stack: &str) -> Option<Vec<String>> {
        let files = self.inner.files.clone();
        let s2 = stack.to_string();
        let read =
            tokio::task::spawn_blocking(move || files.read(&s2, ActionKind::Backup, None)).await;
        let Ok(Ok(Material::Manifest(m))) = read else {
            return None;
        };
        Some(
            homelab_core::ops::backup::owner_groups(&m)
                .into_iter()
                .map(|(owner, _)| owner)
                .collect(),
        )
    }

    /// host.toml's `zfs_jobs` and `device_backups`, when the host answers.
    async fn read_host_config(&self, need: &FactsNeeded, facts: &mut PlanFacts) {
        let reply = self
            .inner
            .host
            .ask_traced(Command::GetHostConfig, Duration::from_secs(10), None)
            .await;
        if let Ok(r) = reply
            && r.ok
            && let Ok(file) = serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message)
        {
            if need.zfs_jobs {
                facts.zfs_jobs = match file.values.get("zfs_jobs") {
                    None => Some(Vec::new()),
                    Some(v) => serde_json::from_value(v.clone()).ok(),
                };
            }
            if need.devices {
                facts.device_backups = match file.values.get("device_backups") {
                    None => Some(Vec::new()),
                    Some(v) => serde_json::from_value::<
                        Vec<homelab_core::ops::devicebackup::DeviceBackup>,
                    >(v.clone())
                    .ok()
                    .map(|d| d.into_iter().map(|x| x.name).collect()),
                };
            }
        }
    }

    /// one-process-batch-1 (Kenny, 2026-10-23: "van begin tot einde moet dat
    /// 1 proces zijn, met een op voorhand bekend aantal stappen"): every
    /// member's step plan, from the commands it will send (the same source
    /// a single action's up-front total uses, `actions_progress::
    /// commands_plan_with`), once per batch. one-process-batch-2: every
    /// command that reaches the host has its step names in core, next to the
    /// op that runs them, so a member is exact wherever its inputs could be
    /// read; only a member with a command whose inputs could not be read is
    /// estimated from the history of the same operation and marked not
    /// exact (with the reason), and the page then says "about N steps".
    pub(super) async fn ensure_batch_plan(&self, batch: u64) {
        let _gate = self.inner.batch_plan_gate.lock().await;
        if self
            .inner
            .batch_plans
            .lock()
            .is_ok_and(|p| p.contains_key(&batch))
        {
            return;
        }
        let ids = self.batch_ids(batch);
        if ids.is_empty() {
            return;
        }
        let history = self.history().await;
        let mut plans = Vec::new();
        for id in ids {
            let Some(v) = self.job(id) else { continue };
            plans.push(self.member_plan(&v, &history).await);
        }
        if let Ok(mut p) = self.inner.batch_plans.lock() {
            p.insert(batch, plans);
        }
        self.publish_batch(batch);
    }

    /// One batch member's plan from the commands it will send.
    async fn member_plan(
        &self,
        v: &JobView,
        history: &[homelab_core::history::HistoryEntry],
    ) -> MemberPlan {
        let req = ActionRequest {
            stack: v.stack.clone(),
            action: v.action,
            args: v.args.clone(),
        };
        let commands = match self.material(&req).await {
            Ok(m) => actions::commands(&req, m).ok(),
            Err(_) => None,
        };
        let facts = match &commands {
            Some(c) => self.plan_facts(c).await,
            None => PlanFacts::default(),
        };
        actions_progress::member_plan_with(
            v.job,
            &v.stack,
            commands.as_deref(),
            &facts,
            history,
            &format!("{}-{}", v.action.slug(), v.stack),
        )
    }
}
