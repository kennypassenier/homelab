//! Run one job's commands in order and follow each to its reply.

use homelab_proto::{Command, RpcResponse};

use super::{Actions, JobView, Stop};
use crate::core::actions::{self, ActionKind, ActionRequest, Material};
use crate::core::actions_cli::cli_line_typed;
use crate::core::actions_progress::{self, Tracker};

impl Actions {
    pub(super) async fn execute(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        if let Some(done) = self.run_composite(req, view).await {
            return done;
        }
        let (commands, expected) = self.commands_for(req, view).await?;
        self.store(view);
        let mut tracker = Tracker::new(self.history().await);
        self.announce_total(&commands, view, &mut tracker).await;
        let last = self.send_all(commands, view, &mut tracker).await?;
        Ok(self.after_last(req, view, expected, last).await)
    }

    /// The jobs of several parts, each part an ordinary action:
    /// redesign-flows-6 (the Update flow), eten-home-ip-1 (a job of the
    /// dashboard's own, no host command), bundle-release-rollout /
    /// -secret-restart / -hosttoml-deploys.
    async fn run_composite(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Option<Result<RpcResponse, Stop>> {
        Some(match req.action {
            ActionKind::UpdateApps => self.run_update_flow(req, view).await,
            ActionKind::UpdateHomeAddress => self.run_home_address(req, view).await,
            ActionKind::RollOutRelease => self.run_release_rollout(req, view).await,
            ActionKind::ChangeSecret if req.args.then_deploy => {
                self.run_secret_then_deploy(req, view).await
            }
            ActionKind::RestartHost if req.args.then_apply => {
                self.run_restart_then_apply(req, view).await
            }
            _ => return None,
        })
    }

    /// The commands the job sends, and the version an update of the host is
    /// expected to answer as; sets the job's CLI line.
    async fn commands_for(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<(Vec<Command>, Option<String>), Stop> {
        let material = self.material(req).await.map_err(Stop::Refused)?;
        let over = actions::cli_override(req, &material);
        let expected = match &material {
            Material::HostRelease { tag, .. } => {
                homelab_client::release::expected_host_version(tag)
            }
            _ => None,
        };
        // native-first-install-1: "Install newest" on a stack whose files
        // declare natives the host has not adopted yet is an install of the
        // newest release of each declared unit.
        let first = self.first_install(req).await.map_err(Stop::Refused)?;
        let over = first.as_ref().map(|(_, l)| l.clone()).or(over);
        let commands = match first {
            Some((c, _)) => c,
            None => actions::commands(req, material).map_err(Stop::Refused)?,
        };
        // The name was typed and checked before the job ran (cli-yes).
        view.cli = over.or_else(|| cli_line(req, commands.last()));
        Ok((commands, expected))
    }

    /// one-process-batch-2 (Kenny, 2026-10-23: "van begin tot einde moet dat
    /// 1 proces zijn, met een op voorhand bekend aantal stappen"): the
    /// job's whole step plan, named before the first command is sent. Every
    /// command that reaches the host has its plan-names function in core,
    /// next to the op that runs the steps (`homelab_core::ops::*`), and the
    /// op plans from that same function, so the count here cannot drift
    /// from what runs. The facts a command's plan depends on besides the
    /// command itself (the stack's native services, the host's patch
    /// targets, zfs jobs and devices) are read first (`plan_facts`).
    ///
    /// A job with several plans to come (Apply, a batch item that stages a
    /// binary and then deploys it, a native action the host fans out over
    /// the stack's units) fixes its total here as the sum, so the
    /// announcements that follow only confirm it. A job of ONE plan shows
    /// the total from the first frame too, but the host's own announcement
    /// replaces it, so the host's count wins should the dashboard's inputs
    /// ever have differed from the host's. A job with a command whose
    /// inputs could not be read keeps no up-front number: the host's
    /// announcement sets it, as before.
    async fn announce_total(
        &self,
        commands: &[Command],
        view: &mut JobView,
        tracker: &mut Tracker,
    ) {
        let facts = self.plan_facts(commands).await;
        let plan = actions_progress::commands_plan_with(commands, &facts);
        if plan.exact() && !plan.parts.is_empty() {
            let total = plan.names().len();
            if plan.multi() {
                tracker.set_total(total);
            } else {
                tracker.expect_total(total);
            }
            view.plan = Some(plan.labeled());
            self.store(view);
            self.announce_plan(view, tracker);
        }
    }

    /// Send the commands one after the other; the reply of the last one
    /// sent (a failing command ends the run).
    async fn send_all(
        &self,
        commands: Vec<Command>,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) -> Result<RpcResponse, Stop> {
        let mut last = None;
        for command in commands {
            let r = self.follow(command, view, tracker).await?;
            let ok = r.ok;
            last = Some(r);
            if !ok {
                break;
            }
        }
        last.ok_or_else(|| Stop::Link("nothing was sent".into()))
    }

    /// An update of the host is done when the shipped version answers; a
    /// restart of the host waits the same way.
    async fn after_last(
        &self,
        req: &ActionRequest,
        view: &JobView,
        expected: Option<String>,
        last: RpcResponse,
    ) -> RpcResponse {
        if req.action == ActionKind::UpdateHost && last.ok {
            return self.await_updated_host(view, expected, last, false).await;
        }
        // Owner decision 2026-09-30 (item 2): "Save and restart the host"
        // waits the same way `homelab release-update` does — the line
        // drops and comes back on its own (arch-host-link) — but with no
        // new version expected, since the binary did not change.
        if req.action == ActionKind::RestartHost && last.ok {
            return self.await_updated_host(view, None, last, true).await;
        }
        last
    }
}

/// The CLI line of the job's last command, with the commit checkout a
/// deploy of one commit needs first.
fn cli_line(req: &ActionRequest, main: Option<&Command>) -> Option<String> {
    main.and_then(|c| cli_line_typed(c, req.args.force, true))
        .map(|l| match &req.args.commit {
            Some(c) if req.action == ActionKind::DeployCommit => {
                format!("git checkout {c} -- stacks/{} && {l}", req.stack)
            }
            _ => l,
        })
}
