//! gap-12 and the F129 fallbacks: pulling an app's images.

use crate::error::CoreError;
use crate::executor::{CmdOutput, pct_sh};
use crate::manifest;
use crate::ops::util::push_content;

use super::run::{Run, log_info, log_warn};

/// What the pull of one app is allowed to do when the registry fails.
struct PullPlan {
    cmd: String,
    /// Seconds the first attempt may take.
    budget: u64,
    /// F129: the compose file as it was before it was pointed at the cache:
    /// `(destination, original content, mode)`.
    fallback: Option<(String, String, String)>,
    keep_port: Option<u16>,
}

impl Run<'_, '_> {
    pub(super) async fn pull_images(&self, app: &str, dir: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let plan = self.pull_plan(app, dir);
        let all_present = self.all_images_present(app, dir).await;
        // Not `?`: a timeout is an Err by the Executor contract, and a
        // timeout is precisely the case the fallback exists for. Taking
        // the error here would step over it.
        let first = if all_present {
            Ok(CmdOutput::ok(""))
        } else {
            pct_sh(exec, m.vmid, &plan.cmd, plan.budget).await
        };
        let mut pull = Self::first_attempt(first, &plan, app)?;
        if !pull.success()
            && let Some((dest, original, perms)) = &plan.fallback
        {
            pull = self
                .pull_from_origin(app, &plan, dest, original, perms)
                .await?;
        }
        if !pull.success() {
            pull = self.pull_from_keep(app, &plan, pull).await?;
        }
        if !pull.success() {
            return Err(CoreError::Command {
                rendered: format!("compose pull {}", app),
                detail: pull.stderr,
            });
        }
        Ok(())
    }

    /// F129 · the fallback half of C1. An app whose compose was
    /// pointed at the cache gets a bounded first attempt; anything
    /// else keeps the full step budget, because then there is nowhere
    /// to fall back TO and cutting it short would only turn a slow
    /// registry into a failed deploy.
    fn pull_plan(&self, app: &str, dir: &str) -> PullPlan {
        let cache = self.ctx.registry_cache.as_ref();
        let fallback = self.pushed.cache_orig.get(app).cloned();
        let budget = match (&fallback, cache) {
            (Some(_), Some(c)) => c.pull_timeout_secs,
            _ => 900,
        };
        PullPlan {
            cmd: format!("cd {} && docker compose pull -q", dir),
            budget,
            fallback,
            keep_port: cache.and_then(|c| c.keep_port),
        }
    }

    /// gap-12: a deploy fetches only what the container does not
    /// have. Updating stays where the rollback is; a deploy deploys.
    /// Story: docs/deployment/REGISTER.md.
    ///
    /// The container is asked per image the compose file names (after
    /// the cache rewrite, so the name asked about is the name `up -d`
    /// will start). Standing rule 12: no usable answer means pull —
    /// the old behaviour — never "assume it is there".
    async fn all_images_present(&self, app: &str, dir: &str) -> bool {
        let (ctx, _, m, exec) = self.parts();
        let present = pct_sh(
            exec,
            m.vmid,
            &format!(
                "cd {} && for i in $(docker compose config --images 2>/dev/null); do \
                 docker image inspect \"$i\" >/dev/null 2>&1 && echo \"present $i\" \
                 || echo \"missing $i\"; done",
                dir
            ),
            60,
        )
        .await;
        let all_present = match &present {
            Ok(o) if o.success() => {
                let lines: Vec<&str> = o.stdout.lines().map(str::trim).collect();
                !lines.is_empty() && lines.iter().all(|l| l.starts_with("present "))
            }
            _ => false,
        };
        if all_present {
            log_info(
                ctx,
                format!(
                    "[images] {} :: every image is already on the container — not pulled \
                     (a deploy never replaces an image; `homelab update` does)",
                    app
                ),
            );
        }
        all_present
    }

    /// Turns the first attempt into an output the fallbacks can judge.
    fn first_attempt(
        first: Result<CmdOutput, CoreError>,
        plan: &PullPlan,
        app: &str,
    ) -> Result<CmdOutput, CoreError> {
        match first {
            Ok(o) if o.success() => Ok(o),
            other => {
                if plan.fallback.is_none() && plan.keep_port.is_none() {
                    // Nothing to fall back to — the original behaviour,
                    // error and all.
                    let o = other?;
                    return Err(CoreError::Command {
                        rendered: format!("compose pull {}", app),
                        detail: o.stderr,
                    });
                }
                Ok(match other {
                    Ok(o) => o,
                    Err(e) => CmdOutput {
                        code: 1,
                        stdout: String::new(),
                        stderr: e.to_string(),
                    },
                })
            }
        }
    }

    /// The cache did not deliver. Put the real registry back in
    /// the file and pull that — and LEAVE it there, so the
    /// `up -d` after this starts the image we actually
    /// fetched instead of one the cache still cannot serve.
    async fn pull_from_origin(
        &self,
        app: &str,
        plan: &PullPlan,
        dest: &str,
        original: &str,
        perms: &str,
    ) -> Result<CmdOutput, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        log_warn(
            ctx,
            format!(
                "[cache] {} did not deliver within {}s — falling back to its own registry",
                app, plan.budget
            ),
        );
        push_content(exec, m.vmid, dest, original, perms).await?;
        // A half-written layer from the abandoned attempt
        // makes the direct pull fail on a digest mismatch,
        // which reads like the image itself is broken. It is
        // not: it is the truncated blob the cache left behind.
        pct_sh(exec, m.vmid, "docker system prune -f", 300).await?;
        pct_sh(exec, m.vmid, &plan.cmd, 900).await
    }

    /// pinned-digest-pruned-1: the origin (and the cache) could not
    /// deliver. If this app pins digests, the keep registry on the
    /// cache host may hold them: point only those `image:` lines at
    /// it and pull once more. Pulling by digest is verified by docker,
    /// so a kept copy is exactly the pinned image or nothing.
    async fn pull_from_keep(
        &self,
        app: &str,
        plan: &PullPlan,
        failed: CmdOutput,
    ) -> Result<CmdOutput, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let (Some(port), Some(cache)) = (plan.keep_port, ctx.registry_cache.as_ref()) else {
            return Ok(failed);
        };
        let Some((dest, kept, perms)) = self.kept_compose(app, &cache.host, port)? else {
            return Ok(failed);
        };
        log_warn(
            ctx,
            format!(
                "[keep] {} :: the registry did not deliver ({}) — pulling the \
                 pinned digests from the kept copies on {}:{}",
                app,
                failed
                    .stderr
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or("no reason given"),
                cache.host,
                port
            ),
        );
        push_content(exec, m.vmid, &dest, &kept, &perms).await?;
        pct_sh(exec, m.vmid, &plan.cmd, 900).await
    }

    /// The app's compose file with its pinned digests pointed at the keep
    /// registry: `(destination, content, mode)`, or `None` when the app has
    /// no such file or pins nothing the keep registry could hold.
    fn kept_compose(
        &self,
        app: &str,
        host: &str,
        port: u16,
    ) -> Result<Option<(String, String, String)>, CoreError> {
        let (_, spec, m, _) = self.parts();
        let Some(f) = spec
            .files
            .iter()
            .find(|f| f.path == format!("{}/docker-compose.yml", app))
        else {
            return Ok(None);
        };
        let kept = crate::ops::keep::rewrite_to_keep(
            &f.content,
            host,
            port,
            m.registry_login.as_ref().map(|r| r.registry.as_str()),
        );
        if kept == f.content {
            return Ok(None);
        }
        let (dest, perms) = manifest::file_destination(&m.stack_name, &f.path)
            .map(|(d, mode)| (d, format!("{:o}", f.mode.unwrap_or(mode))))
            .map_err(CoreError::Validation)?;
        Ok(Some((dest, kept, perms)))
    }
}
