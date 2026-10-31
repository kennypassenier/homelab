//! fix-88: the container's own firewall (flat-vlan-no-east-west-control,
//! traefik-lan-host-header-bypass, 2026-09-27), and the tile watcher's.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};
use crate::firewall::TileWatcher;
use crate::manifest::FirewallSpec;
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// Before the container is created, so a new one starts behind its
    /// rules; written only when the rendering differs from what pve holds,
    /// and the transcript names the lines that came and went. `enabled:
    /// false` is a declaration kept for the stack-by-stack rollout and
    /// touches nothing.
    pub(super) async fn firewall(&self, fwspec: &FirewallSpec) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, _) = self.parts();
        if !fwspec.enabled {
            log_info(
                ctx,
                format!(
                    "[firewall] declared, not enabled — {} is left as it is",
                    crate::firewall::fw_path(m.vmid)
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        let mut changed = self.write_rules(fwspec).await?;
        // Without `firewall=1` on the NIC Proxmox applies none of it.
        // CT 116's flag was set by hand; the create path sets it for a new
        // container.
        if self.exists {
            changed |= self.enable_nic_firewall().await?;
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// The rules file: rendered, compared with pve's copy, written if it moved.
    async fn write_rules(&self, fwspec: &FirewallSpec) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let path = crate::firewall::fw_path(m.vmid);
        // tile-watch (owner decision "Afgeleid uit de tegels",
        // 2026-09-30): a derived inbound rule for the dashboard's own
        // tile watch, appended before rendering so it is written, held
        // against pve and shown in every plan/diff the same way a
        // hand-declared rule is — and gone again the deploy after its
        // tile is.
        let effective = crate::firewall::with_tile_watch(
            fwspec,
            &m.network.ip,
            &m.tiles,
            ctx.tile_watch_source.as_deref().unwrap_or(""),
            &ctx.tile_watch_targets,
            &ctx.trusted_sources,
        );
        let body = crate::firewall::render(&m.stack_name, &effective);
        let old = exec.read_file(&path).await.ok();
        if old.as_deref() == Some(body.as_str()) {
            log_info(ctx, format!("[firewall] {} unchanged", path));
            return Ok(false);
        }
        // 0640: pmxcfs refuses any other mode outside priv/.
        exec.write_file(&path, &body, 0o640).await?;
        let (added, removed) = crate::firewall::line_changes(old.as_deref().unwrap_or(""), &body);
        log_info(
            ctx,
            format!(
                "[firewall] {} written: +{} -{}{}",
                path,
                added.len(),
                removed.len(),
                if old.is_none() { " (new file)" } else { "" }
            ),
        );
        for l in &added {
            log_info(ctx, format!("[firewall]   + {}", l));
        }
        for l in &removed {
            log_info(ctx, format!("[firewall]   - {}", l));
        }
        Ok(true)
    }

    /// Sets `firewall=1` on net0 when the live container lacks it.
    async fn enable_nic_firewall(&self) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        let cfg = run_ok(exec, &Cmd::new("pct", &["config", &vm], 30)).await?;
        let net0 = cfg
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("net0:"))
            .map(str::trim);
        let Some(new) = net0.and_then(crate::firewall::net0_with_firewall) else {
            return Ok(false);
        };
        run_ok(exec, &Cmd::new("pct", &["set", &vm, "--net0", &new], 60)).await?;
        log_info(
            ctx,
            format!(
                "[firewall] net0 of {} now carries firewall=1 — the rules apply from here",
                m.vmid
            ),
        );
        Ok(true)
    }

    /// fix-154: Proxmox drops a vmid's firewall file when it creates that
    /// vmid, so the file written above, before the container existed, is
    /// gone once `pct clone` has made it. A new container gets the file
    /// written again, now that it exists. Story: docs/deployment/REGISTER.md.
    pub(super) async fn firewall_after_create(
        &self,
        fwspec: &FirewallSpec,
    ) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let path = crate::firewall::fw_path(m.vmid);
        let effective = crate::firewall::with_tile_watch(
            fwspec,
            &m.network.ip,
            &m.tiles,
            ctx.tile_watch_source.as_deref().unwrap_or(""),
            &ctx.tile_watch_targets,
            &ctx.trusted_sources,
        );
        let body = crate::firewall::render(&m.stack_name, &effective);
        // Unconditionally: what pmxcfs reports for a vmid it has just
        // created is not something to compare against.
        exec.write_file(&path, &body, 0o640).await?;
        log_info(
            ctx,
            format!(
                "[firewall] {} written again after the container was created (creating a \
                 vmid removes its firewall file)",
                path
            ),
        );
        Ok(StepOutcome::Changed)
    }

    /// tile-watch-watcher-out (found 2026-10-01): this stack's own tiles
    /// may have just changed what the WATCHER (`tile_watch_source`) needs to
    /// reach outbound — a tile that arrived or left here changes the derived
    /// OUT rule on a container this deploy never touches otherwise. Folding
    /// this stack's fresh `m.tiles` into the fleet targets read a moment
    /// earlier (`ctx.tile_watch_targets`, from `HostState`) means the
    /// watcher's file is correct even though its own deploy has not run.
    pub(super) async fn refresh_watcher_firewall(
        &self,
        watcher: &TileWatcher,
        source: &str,
    ) -> Result<StepOutcome, CoreError> {
        let (ctx, _, _, exec) = self.parts();
        let path = crate::firewall::fw_path(watcher.vmid);
        let body = self.watcher_rules(watcher, source);
        let old = exec.read_file(&path).await.ok();
        if old.as_deref() == Some(body.as_str()) {
            log_info(
                ctx,
                format!(
                    "[firewall] tile watcher {} ({}) unchanged",
                    path, watcher.stack
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        exec.write_file(&path, &body, 0o640).await?;
        let (added, removed) = crate::firewall::line_changes(old.as_deref().unwrap_or(""), &body);
        log_info(
            ctx,
            format!(
                "[firewall] tile watcher {} ({}) refreshed by this deploy's tiles: +{} -{}",
                path,
                watcher.stack,
                added.len(),
                removed.len()
            ),
        );
        Ok(StepOutcome::Changed)
    }

    /// The watcher's rules with this stack's fresh tiles folded in.
    fn watcher_rules(&self, watcher: &TileWatcher, source: &str) -> String {
        let (ctx, _, m, _) = self.parts();
        let fresh_targets = crate::firewall::with_fresh_target(
            &ctx.tile_watch_targets,
            &m.network.ip,
            &m.tiles,
            true,
        );
        let effective = crate::firewall::with_tile_watch(
            &watcher.firewall,
            &watcher.own_ip,
            &watcher.tiles,
            source,
            &fresh_targets,
            &ctx.trusted_sources,
        );
        crate::firewall::render(&watcher.stack, &effective)
    }
}
