//! What a job reads before it runs: the files, the release, the secret or
//! the plan its commands are built from.

use super::Actions;
use super::preview::given_tag;
use crate::core::actions::{self, ActionKind, ActionRequest, Material, Needs, Refusal};
use homelab_proto::Command;

impl Actions {
    pub(super) async fn material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        match req.action.needs() {
            Needs::HostRelease => return self.host_release_material(req).await,
            Needs::NativeRelease => return self.native_release_material(req).await,
            Needs::Apply => return self.apply_material(req).await,
            _ => {}
        }
        if req.action == ActionKind::ChangeSecret {
            return self.secret_material(req);
        }
        if req.action.needs() == Needs::Vmid {
            return self.vmid_material(req);
        }
        self.read_material(req).await
    }

    async fn host_release_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        let releases = self.releases()?;
        let tag = match given_tag(&req.args) {
            Some(t) => t.to_string(),
            None => releases
                .latest_tag(crate::shell::releases::HOMELAB_REPO)
                .await
                .map_err(|why| {
                    Refusal::new(what.clone(), why, "name the tag, or try again later")
                })?,
        };
        let bin = releases.host_binary(&tag).await.map_err(|why| {
            Refusal::new(
                what.clone(),
                why,
                "nothing was sent to the host; sign the release, or pick a signed one",
            )
        })?;
        Ok(Material::HostRelease {
            tag,
            binary_b64: bin.binary_b64,
            proof: bin.proof,
        })
    }

    async fn native_release_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        let files = self.inner.files.clone();
        let (stack, unit) = (req.stack.clone(), req.args.unit.clone());
        let (m, unit_file, dir) =
            tokio::task::spawn_blocking(move || files.native_release(&stack, unit.as_deref()))
                .await
                .map_err(|e| Refusal::new(what.clone(), e.to_string(), "report this"))??;
        let tag = match given_tag(&req.args) {
            Some(t) => t.to_string(),
            None => {
                let repo = m.release_repo.clone().unwrap_or_default();
                self.releases()?.latest_tag(&repo).await.map_err(|why| {
                    Refusal::new(what.clone(), why, "name the tag, or try again later")
                })?
            }
        };
        Ok(Material::NativeRelease {
            manifest: Box::new(m),
            unit_file,
            tag,
            dir,
        })
    }

    fn secret_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        let secret: homelab_proto::SecretRef =
            serde_json::from_str(req.args.secret_ref.as_deref().unwrap_or("")).map_err(|_| {
                Refusal::new(what.clone(), "secret_ref did not read", "report this")
            })?;
        let content = self
            .take_staged_secret(req.args.stage_token.as_deref().unwrap_or(""))
            .ok_or_else(|| {
                Refusal::new(
                    what.clone(),
                    "the staged value is gone (it expired, or was already used)",
                    "stage the new value again on the Secrets page and press Save once",
                )
            })?;
        Ok(Material::Secret { secret, content })
    }

    fn vmid_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        match self.fleet_stack(&req.stack) {
            Some((vmid, _)) => Ok(Material::Vmid(vmid)),
            None => Err(Refusal::new(
                format!("{} {}", req.action.slug(), req.stack),
                "the host's fleet has no such stack (or the dashboard has not read it yet)",
                "wait for the fleet page to show the stack",
            )),
        }
    }

    /// The stack files, after the deploy guard, off the async threads.
    async fn read_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let files = self.inner.files.clone();
        let (stack, kind, commit) = (req.stack.clone(), req.action, req.args.commit.clone());
        let guard_self = self.clone();
        let req2 = req.clone();
        tokio::task::spawn_blocking(move || {
            guard_self.guard(&req2)?;
            files.read(&stack, kind, commit.as_deref())
        })
        .await
        .map_err(|e| Refusal::new("reading the stack files", e.to_string(), "report this"))?
    }

    /// native-first-install-1: the commands of "Install newest" for a stack
    /// declaring natives the host has not adopted (the host would answer
    /// "is a compose stack, not a native service"); None: the verb goes to
    /// the host unchanged.
    pub(super) async fn first_install(
        &self,
        req: &ActionRequest,
    ) -> Result<Option<(Vec<Command>, String)>, Refusal> {
        if req.action != ActionKind::ReleaseUpdateNative {
            return Ok(None);
        }
        let what = format!("{} {}", req.action.slug(), req.stack);
        let files = self.inner.files.clone();
        let stack = req.stack.clone();
        let declared = tokio::task::spawn_blocking(move || files.native_units(&stack))
            .await
            .map_err(|e| Refusal::new(what.clone(), e.to_string(), "report this"))?;
        if !actions::needs_first_install(self.fleet_native(&req.stack), &declared) {
            return Ok(None);
        }
        let mut units = Vec::new();
        for unit in declared {
            units.push(self.unit_install(req, &unit, &what).await?);
        }
        actions::first_install_commands(req, units).map(Some)
    }

    /// One declared unit's manifest, unit file and newest release tag.
    async fn unit_install(
        &self,
        req: &ActionRequest,
        unit: &str,
        what: &str,
    ) -> Result<actions::UnitInstall, Refusal> {
        let files = self.inner.files.clone();
        let (stack, u) = (req.stack.clone(), unit.to_string());
        let (manifest, unit_file, dir) =
            tokio::task::spawn_blocking(move || files.native_release(&stack, Some(&u)))
                .await
                .map_err(|e| Refusal::new(what, e.to_string(), "report this"))??;
        let repo = manifest.release_repo.clone().unwrap_or_default();
        let tag = self.releases()?.latest_tag(&repo).await.map_err(|why| {
            Refusal::new(what, why, "try again later, or install a named release")
        })?;
        Ok(actions::UnitInstall {
            manifest,
            unit_file,
            tag,
            dir,
        })
    }
}
