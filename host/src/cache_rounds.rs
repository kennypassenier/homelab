//! Background rounds that keep caches warm: the image prefetch, the keep round and refreshing running images.

use crate::*;

/// fix-registry-1: one prefetch round at a time.
pub(crate) static PREFETCH_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// fix-registry-1 (Kenny, 2026-10-09): pull the newer pinned versions the
/// round just learned about into the pull-through cache, so the update that
/// follows finds them there. A detached task: it holds no operation lock and
/// the nightly round does not wait for it; one round at a time, at most
/// `PREFETCH_MAX_IMAGES` images, each bounded (see `pins::prefetch`).
pub(crate) fn spawn_cache_prefetch(state: &AppState, hs: &homelab_core::state::HostState) {
    let Some(cache) = state.config.registry_cache.as_ref() else {
        return;
    };
    // The cache lives on the `registry` stack's guest.
    let Some(vmid) = hs.stacks.get("registry").map(|s| s.vmid) else {
        return;
    };
    let refs = homelab_core::ops::pins::prefetch_refs(hs, cache);
    if refs.is_empty() {
        return;
    }
    if PREFETCH_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        info!("[prefetch] a round is still running — not starting another");
        return;
    }
    info!(
        "[prefetch] {} newer pinned image(s) for the cache",
        refs.len()
    );
    tokio::spawn(async move {
        for line in homelab_core::ops::pins::prefetch(&RealExecutor, vmid, &refs).await {
            info!("{}", line);
        }
        PREFETCH_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// pinned-digest-pruned-1: one keep round at a time.
pub(crate) static KEEP_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// pinned-digest-pruned-1: make sure the keep registry on the cache host holds
/// every digest the deployed compose files pin, and drop what none pins any
/// more (`homelab_core::ops::keep`). A detached task like the prefetch: no
/// operation lock, never in the nightly round's way, one at a time.
pub(crate) fn spawn_keep_round(state: &AppState, hs: &homelab_core::state::HostState) {
    let Some(cache) = state.config.registry_cache.clone() else {
        return;
    };
    if cache.keep_port.is_none() {
        return;
    }
    let Some(vmid) = hs.stacks.get("registry").map(|s| s.vmid) else {
        return;
    };
    if KEEP_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        info!("[keep] a round is still running — not starting another");
        return;
    }
    let hs = hs.clone();
    let no_touch = state.config.safety.no_touch.clone();
    tokio::spawn(async move {
        let (wanted, complete, notes) =
            homelab_core::ops::keep::gather_wanted(&RealExecutor, &hs, &cache, &no_touch).await;
        for n in notes {
            info!("{}", n);
        }
        info!("[keep] {} pinned digest(s) across the fleet", wanted.len());
        for line in
            homelab_core::ops::keep::round(&RealExecutor, &cache, vmid, &wanted, complete).await
        {
            info!("{}", line);
        }
        KEEP_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// fix-stale-refresh-1 (Kenny, 2026-10-09, Cloudflared 2026.10.0): operations
/// after which a stack's running images have changed.
pub(crate) fn refreshes_running_images(label: &str) -> bool {
    matches!(label, "deploy" | "update" | "restart")
}

/// fix-stale-refresh-1: read the stack's running images again and fold them
/// into `running_images`, so the fleet check's "update available" rows for it
/// settle now instead of at the next nightly round. No GitHub call: the
/// cached `upstream_releases` are reused. A failed reading is only logged;
/// the nightly round remains the fallback.
pub(crate) async fn refresh_running_images(state: &AppState, exec: &RealExecutor, stack: &str) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let Ok(snapshot) = store.load().await else {
        return;
    };
    let Some(vmid) = snapshot.stacks.get(stack).map(|s| s.vmid) else {
        return;
    };
    match homelab_core::ops::pins::read_runtime(exec, vmid, unix_now()).await {
        Ok(rt) => {
            let facts = homelab_core::ops::pins::PinFacts {
                images: std::collections::BTreeMap::from([(stack.to_string(), rt.images)]),
                releases: Default::default(),
            };
            record_state(&store, "running_images", |s| {
                homelab_core::ops::pins::apply(s, facts)
            })
            .await;
        }
        Err(why) => tracing::warn!("[pins] {stack}: running images not refreshed: {why}"),
    }
}
