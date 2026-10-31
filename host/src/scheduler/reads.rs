//! What the night reads and keeps tidy after its work: pinned digests, the edge comparison, managed tools and the daemon's own records.

use super::Night;
use crate::*;

/// fix-83: record the digest every manual container runs and the latest upstream release.
pub(super) async fn pins(n: &Night<'_>) {
    let (state, exec, store) = (n.state, n.exec, &n.store);
    let now = n.now;
    // fix-83 (manual-images-latest-unpinned, 2026-09-27): record the
    // digest every `manual` container runs, and ask each declared
    // upstream for its latest release, so the fleet check below can say
    // when a pinned app has fallen behind. Read-only on the containers;
    // GitHub is asked at most once a night (the answer is cached in
    // state), every call is bounded, and state is loaded again after the
    // reading so a deploy that saved meanwhile is not overwritten.
    if let Ok(snapshot) = store.load().await {
        let (facts, notes) = homelab_core::ops::pins::gather(
            exec,
            &snapshot,
            &state.config.safety.no_touch,
            now,
            state.config.upstream_max_age_s,
        )
        .await;
        for n in notes {
            info!("{}", n);
        }
        if let Ok(mut s) = store.load().await {
            homelab_core::ops::pins::apply(&mut s, facts);
            let _ = store.save(s.clone()).await;
            // fix-registry-1: a newer pinned version was learned; warm the
            // cache in the background, never in this round's way.
            spawn_cache_prefetch(state, &s);
            // pinned-digest-pruned-1: keep a copy of every pinned digest.
            spawn_keep_round(state, &s);
        }
    }
}

/// fix-143: the Cloudflare comparison, once a night.
pub(super) async fn edge(n: &Night<'_>) {
    let (state, exec, store) = (n.state, n.exec, &n.store);
    let now = n.now;
    // fix-143 (Cloudflare nightly comparison, owner decision
    // 2026-10-01): once a night, like the integrity check — this used
    // to run only from the workstation (`homelab check`), so a night
    // nobody ran it went unwatched.
    if let Ok(fresh) = store.load().await
        && homelab_core::ops::restoredrill::due(fresh.last_edge_check, now, 24 * 3600)
    {
        run_nightly_edge_check(state, exec, now).await;
    }
}

/// fix-131: the daemon's own records, bounded every night.
pub(super) async fn housekeeping(n: &Night<'_>) {
    let (state, now) = (n.state, n.now);
    // fix-131: the daemon's own records, bounded every night. Under the
    // operation lock, so no operation appends to the journal while it
    // is cut; off the async workers, since both are plain file work.
    {
        let _guard = state.op_lock.lock().await;
        let dir = state.config.state_dir.clone();
        let max_age_days = state.config.incident_bundle_max_age_days;
        let max_count = state.config.incident_bundle_max_count;
        let journal_max_bytes = state.config.journal_max_bytes;
        let _ = tokio::task::spawn_blocking(move || {
            compact_journal_file(&dir, journal_max_bytes);
            prune_incidents(&dir, now, max_age_days, max_count);
        })
        .await;
    }
}
