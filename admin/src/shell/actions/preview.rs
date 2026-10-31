//! feat-stacks-7 before the press: the CLI line, the guard's refusal and
//! what the press would change, without secrets or downloads.

use std::collections::BTreeMap;

use super::Actions;
use crate::core::actions::{self, ActionArgs, ActionKind, ActionRequest, Material, Needs, Refusal};
use crate::core::actions_cli::cli_line_typed;

impl Actions {
    /// feat-stacks-7 before the press: the CLI line (or why there is
    /// none), the deploy guard's refusal, whether it restarts the dashboard.
    /// `typed`: the form's typed name matched (cli-yes: the line then
    /// carries `--yes`).
    pub async fn preview_of(
        &self,
        mut req: ActionRequest,
        typed: bool,
    ) -> (Result<String, String>, Option<Refusal>, bool) {
        let restarts = actions::restarts_dashboard(&req.stack, req.action);
        // dashboard-latest: the review line never shows the word "latest"
        // (nor an empty tag) — resolve it to the concrete tag first, the
        // same tag the press would install.
        if matches!(
            req.action.needs(),
            Needs::HostRelease | Needs::NativeRelease
        ) {
            match self.resolve_preview_tag(&req).await {
                Ok(tag) => req.args.tag = Some(tag),
                Err(why) => return (Err(why), None, restarts),
            }
        }
        let a2 = self.clone();
        let (line, guard) = tokio::task::spawn_blocking(move || {
            let guard = a2.guard(&req);
            (preview_line(&a2, &req, typed), guard)
        })
        .await
        .unwrap_or((Err("internal".into()), Ok(())));
        (line, guard.err(), restarts)
    }

    /// dashboard-latest: the concrete tag `req.args.tag` resolves to — the
    /// value already typed/picked (unless it is empty or "latest"), else
    /// the repository's newest release.
    pub(super) async fn resolve_preview_tag(&self, req: &ActionRequest) -> Result<String, String> {
        if let Some(t) = given_tag(&req.args) {
            return Ok(t.to_string());
        }
        let repo = match req.action.needs() {
            Needs::HostRelease => crate::shell::releases::HOMELAB_REPO.to_string(),
            Needs::NativeRelease => {
                let files = self.inner.files.clone();
                let (stack, unit) = (req.stack.clone(), req.args.unit.clone());
                let (m, _, _) = tokio::task::spawn_blocking(move || {
                    files.native_release(&stack, unit.as_deref())
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|r| r.why)?;
                m.release_repo.unwrap_or_default()
            }
            _ => return Err("internal".into()),
        };
        self.releases().map_err(|r| r.why)?.latest_tag(&repo).await
    }
}

/// dashboard-latest: the tag the form sent, when it names one — empty and
/// the literal "latest" (the dropdown's top option, the back-compat value
/// of a driven `ui type act-tag latest`) both mean "resolve it", so neither
/// is a given tag.
pub(super) fn given_tag(args: &ActionArgs) -> Option<&str> {
    args.tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != crate::core::releaseoptions::LATEST)
}

/// The tag `req.args.tag` names, trimmed; empty when it names none.
fn trimmed_tag(req: &ActionRequest) -> String {
    req.args
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("")
        .to_string()
}

/// The CLI lines that name more than the command carries, without a
/// download: bundle-release-rollout's two parts, apply's flags and
/// (dashboard-latest) the tag — `preview_of` has already resolved
/// "latest"/empty to the concrete tag before this runs, so the CLI line
/// never shows the word "latest".
fn release_line(a: &Actions, req: &ActionRequest) -> Result<Option<String>, String> {
    let tag = trimmed_tag(req);
    if req.action == ActionKind::RollOutRelease {
        return Ok(Some(
            format!(
                "homelab release-update {tag} && homelab release-update-native {}",
                actions::SELF_STACK
            )
            .replace("  ", " "),
        ));
    }
    Ok(match req.action.needs() {
        Needs::HostRelease => Some(
            format!("homelab release-update {tag}")
                .trim_end()
                .to_string(),
        ),
        Needs::NativeRelease => {
            let (_, _, dir) = a
                .inner
                .files
                .native_release(&req.stack, req.args.unit.as_deref())
                .map_err(|r| r.why)?;
            Some(
                format!("homelab install-native {dir} {tag}")
                    .trim_end()
                    .to_string(),
            )
        }
        Needs::Apply => Some(
            actions::cli_override(req, &Material::None).ok_or_else(|| "internal".to_string())?,
        ),
        _ => None,
    })
}

/// The line names the stack; a spec with the manifest alone says the same
/// thing without reading secrets or releases.
fn spec_probe(a: &Actions, req: &ActionRequest) -> Result<Material, String> {
    let probe = ActionRequest {
        action: ActionKind::Backup,
        ..req.clone()
    };
    match a
        .inner
        .files
        .read(&probe.stack, probe.action, None)
        .map_err(|r| r.why)?
    {
        Material::Manifest(m) => Ok(Material::Spec(Box::new(homelab_proto::DeploySpec {
            secret_files: Vec::new(),
            backup_first: false,
            client_schema: homelab_core::manifest::CURRENT_CLIENT_SCHEMA,
            manifest: *m,
            files: Vec::new(),
            env: BTreeMap::new(),
            gateway_route: None,
            extra_routes: Vec::new(),
            checks: BTreeMap::new(),
            native_binaries: BTreeMap::new(),
            native_manifests: BTreeMap::new(),
            source: None,
        }))),
        _ => Err("the stack files did not read".into()),
    }
}

fn preview_material(a: &Actions, req: &ActionRequest) -> Result<Material, String> {
    match req.action.needs() {
        Needs::Spec => spec_probe(a, req),
        Needs::Vmid => Ok(Material::Vmid(
            a.fleet_stack(&req.stack)
                .map(|(v, _)| v)
                .ok_or("the fleet has no such stack")?,
        )),
        _ => a
            .inner
            .files
            .read(&req.stack, req.action, None)
            .map_err(|r| r.why),
    }
}

pub(super) fn preview_line(
    a: &Actions,
    req: &ActionRequest,
    typed: bool,
) -> Result<String, String> {
    if let Some(line) = release_line(a, req)? {
        return Ok(line);
    }
    let material = preview_material(a, req)?;
    let commands = actions::commands(req, material).map_err(|r| r.why)?;
    let main = commands.last().ok_or("no command")?;
    let line = cli_line_typed(main, req.args.force, typed).ok_or("no CLI verb sends this")?;
    Ok(match &req.args.commit {
        Some(c) if req.action == ActionKind::DeployCommit => {
            format!("git checkout {c} -- stacks/{} && {line}", req.stack)
        }
        _ => line,
    })
}
