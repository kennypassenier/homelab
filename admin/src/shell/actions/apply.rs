//! dash-apply: the plan of the whole stacks directory against the host, and
//! the material of an Apply press.

use super::{Actions, LocalStacks};
use crate::core::actions::{ActionArgs, ActionKind, ActionRequest, Material, Refusal};

impl Actions {
    /// dash-apply: the plan against the host, as `homelab apply --plan`
    /// prints it; `with_specs` builds what a deploy sends (for the run).
    pub async fn apply_plan(
        &self,
        with_specs: bool,
    ) -> Result<(crate::core::applyview::ApplyView, LocalStacks), Refusal> {
        let files = self.inner.files.clone();
        let local = tokio::task::spawn_blocking(move || files.local_stacks(with_specs))
            .await
            .map_err(|e| Refusal::new("the plan", e.to_string(), "report this"))??;
        let host = self.host_pairs().ok_or_else(|| {
            Refusal::new(
                "the plan",
                "the dashboard has not read the host's fleet yet",
                "wait for the fleet page to show the stacks",
            )
        })?;
        let mut view =
            crate::core::applyview::plan(&local.hashes, &local.dirs, &host, &local.ephemeral);
        // fix-192: per-stack, why it is in `deploy` — from digests alone,
        // never a fetch of the host's applied files.
        let host_digests = self.host_digests().unwrap_or_default();
        view.reasons = crate::core::applyview::deploy_reasons(
            &view.deploy,
            &local.component_digests,
            &host_digests,
        );
        Ok((view, local))
    }

    pub(super) async fn apply_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let (view, mut local) = self.apply_plan(true).await?;
        let chosen = self.choose_apply(req, &view)?;
        // arch-deploy-guard: every chosen stack is checked before the
        // first one is sent, so a refusal leaves nothing half-applied.
        if !req.args.force {
            for name in &chosen.deploy {
                let r = ActionRequest {
                    stack: name.clone(),
                    action: ActionKind::Deploy,
                    args: ActionArgs::default(),
                };
                self.guard(&r)?;
            }
        }
        let deploy = chosen
            .deploy
            .iter()
            .filter_map(|n| local.specs.remove(n))
            .collect();
        Ok(Material::Apply {
            deploy,
            destroy: chosen.destroy,
        })
    }

    /// redesign-flows-5: the press's own subset — the ticked deploys, the
    /// stacks left alone named, a destroy only as its own confirmed step for
    /// the CT number the host records under that name.
    fn choose_apply(
        &self,
        req: &ActionRequest,
        view: &crate::core::applyview::ApplyView,
    ) -> Result<crate::core::applyview::Chosen, Refusal> {
        let ids = req.args.destroy_id_list().unwrap_or_default();
        crate::core::applyview::choose(
            view,
            &req.args.leave_out_names(),
            &req.args.destroy_names(),
            &ids,
            req.args.destroy_ack,
            |n| self.fleet_stack(n).map(|(vmid, _)| vmid),
        )
        .map_err(|why| {
            Refusal::new(
                "apply",
                why,
                "plan again: Deploy all changes lists what each press may deploy, leave alone or destroy",
            )
        })
    }
}
