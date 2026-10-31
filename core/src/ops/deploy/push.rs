//! D1: push files into the container.
//!
//! Which apps had a config file change is remembered, because pushing a
//! file is not the same as the service reading it. `docker compose up -d`
//! sees an unchanged compose definition and leaves the container running,
//! so an edit to a bind-mounted config takes effect at the next unrelated
//! restart — or never. On 2026-08-31 that cost a wrong conclusion: promtail
//! ran four more minutes on the old pipeline and the first verification
//! reported the fix as not working.

use crate::error::CoreError;
use crate::manifest::{self, FileBlob};
use crate::ops::util::push_content;
use crate::runner::StepOutcome;
use crate::sink::PipelineEvent;

use super::run::Run;

impl Run<'_, '_> {
    /// The body of the `push files` step; its proof is `PushCheck`.
    pub(super) async fn push_files(&mut self) -> Result<StepOutcome, CoreError> {
        let rootfs_changed = self.push_stack_files().await?;
        self.activate_rootfs(&rootfs_changed).await?;
        self.push_env_files().await?;
        self.push_secret_files().await?;
        self.restore_missing_env().await?;
        Ok(StepOutcome::Changed)
    }

    /// Every file of the stack; returns the `rootfs/` destinations that changed.
    async fn push_stack_files(&mut self) -> Result<Vec<String>, CoreError> {
        let (_, spec, m, _) = self.parts();
        let mut rootfs_changed: Vec<String> = Vec::new();
        for f in &spec.files {
            // A native unit file goes to /etc/systemd/system and nowhere
            // else. Pushing it here too cost almanac its binary: the stack
            // file's path is `<unit>/<unit>.service`, which lands on
            // /opt/almanac/almanac — and that WAS the binary. The garbage
            // collector removed it, the push then made a directory of the
            // same name, and the service kept running only because the
            // kernel holds a deleted file open. A restart would have found
            // nothing there.
            if m.natives
                .iter()
                .any(|u| f.path == format!("{}/{}.service", u, u))
            {
                continue;
            }
            if let Some(dest) = self.push_one(f).await? {
                rootfs_changed.push(dest);
            }
        }
        Ok(rootfs_changed)
    }

    /// Pushes one file; `Some(dest)` when it is a `rootfs/` file that changed.
    async fn push_one(&mut self, f: &FileBlob) -> Result<Option<String>, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        // ask-2: a `rootfs/` file lands at its absolute path (a unit,
        // a timer, a script on PATH); everything else under
        // /opt/<stack>/. Validated before the first push, so an
        // `Err` here cannot happen — it is mapped rather than
        // unwrapped so a future gap is a refusal, not a panic.
        let (dest, default_mode) =
            manifest::file_destination(&m.stack_name, &f.path).map_err(CoreError::Validation)?;
        let rootfs = f.path.starts_with(manifest::ROOTFS_PREFIX);
        let perms = format!("{:o}", f.mode.unwrap_or(default_mode));
        let content = self.content_for_container(f, &dest, &perms);
        // fix-142: recorded unconditionally, before `changed` is
        // even known, and keyed by `f.path` (not `dest`) so it reads
        // the same as `evaluate_repo_drift`'s `StackDigest::files` —
        // both are "<app>/<file>" against /opt/<stack>.
        if !rootfs {
            self.pushed
                .file_hashes
                .insert(f.path.clone(), manifest::sha256_hex(content.as_bytes()));
        }
        let changed = push_content(exec, m.vmid, &dest, &content, &perms).await?;
        if changed {
            self.note_changed(f, &dest, &content, rootfs);
        }
        ctx.sink.emit(PipelineEvent::Bytes {
            op: self.op.clone(),
            label: dest.clone(),
            done: f.content.len() as u64,
            total: Some(f.content.len() as u64),
        });
        Ok((changed && rootfs).then_some(dest))
    }

    /// D60: the file in the repository names the real origin; what
    /// lands in the container names the cache, but only for the
    /// upstreams that answered a moment ago and never for a registry
    /// this stack signs into — that one is private, and the cache is
    /// anonymous by design.
    fn content_for_container(&mut self, f: &FileBlob, dest: &str, perms: &str) -> String {
        let (ctx, _, m, _) = self.parts();
        let cache = match ctx.registry_cache.as_ref() {
            Some(c) if f.path.ends_with("docker-compose.yml") && !self.cache_up.is_empty() => c,
            _ => return f.content.clone(),
        };
        let rewritten = crate::ops::registry_cache::rewrite_compose(
            &f.content,
            cache,
            &self.cache_up,
            m.registry_login.as_ref().map(|r| r.registry.as_str()),
        );
        // Only a compose the rewrite actually touched can fall
        // back; recording the untouched ones would make the pull
        // step re-push identical bytes for every app in the stack.
        if rewritten != f.content
            && let Some((app, _)) = f.path.split_once('/')
        {
            self.pushed.cache_orig.insert(
                app.to_string(),
                (dest.to_string(), f.content.clone(), perms.to_string()),
            );
        }
        rewritten
    }

    /// Remembers what a changed file means for the start step.
    fn note_changed(&mut self, f: &FileBlob, dest: &str, content: &str, rootfs: bool) {
        if let Ok(mut g) = self.pushed.written.lock() {
            g.push((dest.to_string(), manifest::sha256_hex(content.as_bytes())));
        }
        // The path is "<app>/<file>"; a file outside an app directory
        // belongs to no service and needs nothing restarted. A
        // rootfs file is not an app either: "rootfs" must never
        // reach the restart step as a service name.
        if let Some((app, name)) = f.path.split_once('/').filter(|_| !rootfs) {
            if name == "docker-compose.yml" {
                // compose up -d recreates this one by itself.
                self.pushed.recreated.insert(app.to_string());
            } else {
                self.pushed.needs_restart.insert(app.to_string());
            }
        }
    }
}
