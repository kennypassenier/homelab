//! Adoption's writes: the `homelab` tag, the container description, the
//! sealed env file and the stack's record in state. Everything here is
//! additive — nothing in the container's service is touched.

use super::seal::seal_one;
use crate::error::CoreError;
use crate::executor::{Cmd, Executor, run_ok};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::runner::StepOutcome;
use crate::state::{StackState, StateStore};

/// "tag as managed": the `homelab` tag is what makes "managed" visible in
/// the Proxmox list.
/// It was applied only where a container is CREATED, so the two adopted
/// ones carried no tag and a filter on it silently missed them — a signal
/// that means "built by the orchestrator" while it reads as "managed by
/// the orchestrator" (Kenny's question, 2026-08-31). Adoption is the other
/// way in, so it sets the tag too. Additive: any tag somebody else put
/// there stays.
pub(super) async fn tag_as_managed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let vm = m.vmid.to_string();
    let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
    let tags: Vec<String> = cfg
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("tags:"))
        .map(|v| v.split(';').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();
    if tags.iter().any(|t| t == "homelab") {
        return Ok(StepOutcome::Unchanged);
    }
    let mut all: Vec<String> = tags.into_iter().filter(|t| !t.is_empty()).collect();
    all.push("homelab".into());
    let joined = all.join(";");
    run_ok(exec, &Cmd::new("pct", &["set", &vm, "--tags", &joined], 30)).await?;
    Ok(StepOutcome::Changed)
}

/// "describe": the description is the tag's fault one field over. `pct set
/// --description` runs where a container is CREATED (deploy/provision_create.rs), so an
/// adopted container keeps whatever it had — and what CT 109 had was
/// "mailbox 1.0.0 — durable message hub", naming a program that no longer
/// runs and two paths that no longer exist (`/var/lib/mailbox`,
/// `/etc/mailbox/mailbox.env`; the store moved to /appdata on 2026-09-01).
///
/// Both hand-written descriptions in the fleet had drifted by the time
/// this was measured, which is the argument against fixing them by hand:
/// almanac's still pointed at `/etc/almanac/latch.env` while its service
/// file had moved to `/appdata/almanac/almanac-config/latch.env`. A
/// description nobody derives is a description nobody keeps true.
///
/// So it says the one thing that stays true — which stack owns this
/// container — and points at the file that carries the detail, instead of
/// copying facts that will move again. Idempotent by construction: every
/// service in a shared container renders the same string (T5), so the
/// three natives on CT 109 do not fight over it.
pub(super) async fn describe(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let vm = m.vmid.to_string();
    let desc = format!(
        "managed by homelab v2 :: stack {} :: native service(s) under systemd. \
         Authoritative detail — units, binaries, data and env paths — lives in \
         stacks/{}/service.yml in the intent repo, not here.",
        m.stack_name, m.stack_name
    );
    let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
    let current = cfg
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("description:"))
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    // pct reports the description percent-encoded; compare on the marker
    // rather than round-tripping the encoding.
    let marker = format!("stack%20{}%20", m.stack_name);
    if current.contains(&marker) || current.contains(&format!("stack {} ", m.stack_name)) {
        return Ok(StepOutcome::Unchanged);
    }
    let set = Cmd::new("pct", &["set", &vm, "--description", &desc], 30);
    run_ok(exec, &set).await?;
    Ok(StepOutcome::Changed)
}

/// Where a unit's env file is sealed in the host vault.
pub(super) fn vault_path(ctx: &OpCtx<'_>, m: &NativeServiceManifest, env: &str) -> String {
    format!(
        "{}/secrets/{}/{}",
        ctx.state_dir,
        m.stack_name,
        crate::ops::deploy::vault_key(env)
    )
}

/// "seal env file": gap-27: an adopted service's env file gets a copy in
/// the host's vault, like a deployed one, so a lost container can get it
/// back. Read without echoing it (fix-39); nothing in the container is
/// written.
pub(super) async fn seal_env_file(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let Some(env) = &m.env_file else {
        return Ok(StepOutcome::Unchanged);
    };
    let vault = vault_path(ctx, m, env);
    Ok(if seal_one(exec, m.vmid, env, &vault).await? {
        StepOutcome::Changed
    } else {
        StepOutcome::Unchanged
    })
}

/// T5: several native services share one container, so adoption adds to the
/// list rather than replacing it. Re-adopting the same unit replaces just
/// that entry — which is how a manifest correction, like the mailbox→kyu
/// rename, lands without disturbing its neighbours.
fn merged_natives(
    previous: Option<&StackState>,
    m: &NativeServiceManifest,
) -> Vec<NativeServiceManifest> {
    let mut natives: Vec<NativeServiceManifest> =
        previous.map(|s| s.natives.clone()).unwrap_or_default();
    match natives.iter_mut().find(|n| n.unit == m.unit) {
        Some(slot) => *slot = m.clone(),
        None => natives.push(m.clone()),
    }
    natives
}

/// The stack's record after adoption. fix-164: install-native re-adopts
/// every time; a stack a deploy already recorded keeps its manifest, hash,
/// routes, enabled flag and the rest, and only its natives, apps, vmid and
/// hostname change.
fn adopted_record(
    now_unix: u64,
    m: &NativeServiceManifest,
    previous: Option<&StackState>,
) -> StackState {
    let natives = merged_natives(previous, m);
    let mut apps: Vec<String> = natives.iter().map(|n| n.unit.clone()).collect();
    apps.sort();
    apps.dedup();
    match previous {
        Some(prev) => StackState {
            vmid: m.vmid,
            hostname: m.hostname.clone(),
            apps,
            natives,
            ..prev.clone()
        },
        None => StackState {
            applied_source: None,
            vmid: m.vmid,
            hostname: m.hostname.clone(),
            apps,
            applied_at: now_unix,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: None,
            natives,
            enabled: true,
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    }
}

/// "record state": the stack lands in state so the nightly machinery picks
/// it up; a vmid clash with an existing stack is refused.
pub(super) async fn record_state(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let store = StateStore::new(exec, &ctx.state_dir);
    let mut state = store.load().await?;
    if let Some(existing) = state.stacks.get(&m.stack_name)
        && existing.vmid != m.vmid
    {
        return Err(CoreError::SafetyAbort(format!(
            "stack '{}' already exists on vmid {} — refusing to re-point it to {}",
            m.stack_name, existing.vmid, m.vmid
        )));
    }
    let record = adopted_record(ctx.now_unix, m, state.stacks.get(&m.stack_name));
    state.stacks.insert(m.stack_name.clone(), record);
    store.save(state).await?;
    Ok(StepOutcome::Changed)
}
