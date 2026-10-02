//! fix-180: a host-side cache of each restic repository's snapshot list, so
//! a read-only RPC answers from memory instead of shelling out to restic
//! over rclone/Google Drive on every request.
//!
//! Audit 2026-10-02: `Rpc::BackupCalendar` and `Rpc::GetBackups` each read
//! every owning app's repository straight from restic, sequentially, on
//! every call. fix-177 made the dashboard ask once per stack, so a fleet of
//! thirteen stacks fires thirteen of these in a burst; live on CT 120 one of
//! them hit a 180 s budget and answered 502 with nothing to say which
//! repository it was stuck on. Shelling out to restic is also not free of
//! the OTHER thirteen: rclone's Google Drive remote is rate-limited, so
//! thirteen `restic snapshots` at once contend with each other as much as
//! with the budget.
//!
//! Keyed by repository OWNER, not by stack: D25 (`backup::owner_groups`)
//! names a restic repository after the owning app, and two stacks can share
//! one repository, so keying here the same way dedupes for free instead of
//! needing to know which stack asked.
//!
//! `get` never does I/O and never blocks: a repository that has not been
//! read yet answers `None` at once ("not read yet" is the caller's to say),
//! and the caller may [`SnapshotCache::kick`] a background refresh for it.
//! `refresh` is what talks to restic; concurrent callers for the SAME owner
//! join the one restic read already on its way (coalescing) instead of
//! starting a second, and at most `concurrency` owners are read at once
//! across the whole cache (rclone to Google Drive is rate-limited) — one
//! [`tokio::sync::Semaphore`] shared by every path that can trigger a
//! refresh: an RPC's cache miss, the periodic sweep, and the refresh fired
//! right after a backup/prune/restore, so the bound holds everywhere at
//! once rather than per call site.
//!
//! Zero-I/O like the rest of `homelab-core` (AR1): `refresh` takes the
//! `Executor` and the unix clock reading as parameters, same as every other
//! operation here — this module never reads a clock or spawns a task of its
//! own. The host decides when and on what runtime to call it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::{Notify, Semaphore};

use super::backup::{BackupCfg, SnapRun, list_snapshots, repo_size_bytes};
use crate::executor::Executor;

/// One repository's answer, as held in the cache.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct CachedRepo {
    /// Newest first, same order `list_snapshots` already returns.
    pub snapshots: Vec<SnapRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Unix seconds the restic read that produced this answer finished —
    /// the UI's "read N min ago".
    pub measured_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

struct State {
    by_owner: HashMap<String, CachedRepo>,
    /// An owner already being refreshed: a second caller joins this instead
    /// of starting a second restic read for the same repository.
    in_flight: HashMap<String, Arc<Notify>>,
}

/// Bound on cache memory (rule 20: nothing balloons): repositories that
/// nobody has asked about or refreshed in this many sweeps are dropped by
/// [`SnapshotCache::forget_except`], which the host calls with the fleet's
/// current owner set after every destroy and on the periodic sweep — a
/// cache entry lives exactly as long as a repository that still exists.
pub struct SnapshotCache {
    state: Mutex<State>,
    /// How many restic reads this cache allows in flight at once, across
    /// every caller. 2-3 is the measured-safe band for one Google Drive
    /// remote (see `backup::phase_duration_line`'s nightly-backup note).
    limit: Semaphore,
}

impl SnapshotCache {
    pub fn new(concurrency: usize) -> Self {
        Self {
            state: Mutex::new(State {
                by_owner: HashMap::new(),
                in_flight: HashMap::new(),
            }),
            limit: Semaphore::new(concurrency.max(1)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The cached answer for one repository, or `None` when it has never
    /// been read (or was dropped by [`Self::forget_except`]). Never does
    /// I/O, never blocks on an in-flight refresh — a request that wants
    /// fresher data calls [`Self::refresh`] itself, or [`Self::kick`] one
    /// in the background and answers with what is here now.
    pub fn get(&self, owner: &str) -> Option<CachedRepo> {
        self.lock().by_owner.get(owner).cloned()
    }

    /// Every owner this cache currently holds an answer for (for a sweep
    /// that wants to know what is stale, and for tests).
    pub fn cached_owners(&self) -> Vec<String> {
        self.lock().by_owner.keys().cloned().collect()
    }

    /// Read one repository from restic and store the answer, replacing
    /// whatever was cached. A second call for the same `owner` made while
    /// this one is still running joins it and returns the SAME answer,
    /// rather than starting a second restic read (coalescing); either way
    /// at most [`Self::new`]'s `concurrency` reads, cache-wide, run at once.
    pub async fn refresh(
        &self,
        exec: &dyn Executor,
        cfg: &BackupCfg,
        owner: &str,
        now_unix: u64,
    ) -> CachedRepo {
        // Join an in-flight read for the same owner instead of starting one.
        let notify = {
            let mut s = self.lock();
            if let Some(n) = s.in_flight.get(owner) {
                Some(n.clone())
            } else {
                s.in_flight
                    .insert(owner.to_string(), Arc::new(Notify::new()));
                None
            }
        };
        if let Some(n) = notify {
            n.notified().await;
            // The owner that ran the read just stored it; absent only if it
            // failed to store at all, which does not happen below.
            return self.get(owner).unwrap_or(CachedRepo {
                snapshots: Vec::new(),
                size_bytes: None,
                measured_at: now_unix,
                error: Some("a concurrent refresh did not record an answer".into()),
            });
        }

        // Only the caller that won the in-flight race reaches here, so the
        // semaphore bounds real restic reads, not waiters.
        let _permit = self.limit.acquire().await;
        let entry = match list_snapshots(exec, cfg, owner).await {
            Ok(snapshots) => CachedRepo {
                size_bytes: repo_size_bytes(exec, cfg, owner).await,
                snapshots,
                measured_at: now_unix,
                error: None,
            },
            Err(e) => CachedRepo {
                snapshots: Vec::new(),
                size_bytes: None,
                measured_at: now_unix,
                error: Some(e.to_string()),
            },
        };

        let waiters = {
            let mut s = self.lock();
            s.by_owner.insert(owner.to_string(), entry.clone());
            s.in_flight.remove(owner)
        };
        if let Some(n) = waiters {
            n.notify_waiters();
        }
        entry
    }

    /// Drop every cached repository NOT in `keep` — a stack destroyed, or a
    /// repository no manifest names any more, does not sit in memory
    /// forever (rule 20). Two stacks may share an owner (D25), so this is
    /// driven by the fleet's current owner set, never by one stack alone.
    pub fn forget_except(&self, keep: &HashSet<String>) {
        self.lock().by_owner.retain(|owner, _| keep.contains(owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use crate::ops::backup::BackupCfg;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Wraps a `MockExecutor` and sleeps before every `run`, so a test can
    /// watch concurrency and coalescing the way a slow restic-over-rclone
    /// call actually behaves, instead of a mock that answers instantly and
    /// proves nothing about ordering.
    struct SlowExecutor {
        inner: crate::mock::MockExecutor,
        delay: Duration,
        calls: AtomicUsize,
        concurrent: AtomicUsize,
        max_concurrent: AtomicUsize,
    }

    impl SlowExecutor {
        fn new(delay: Duration) -> Self {
            Self {
                inner: crate::mock::MockExecutor::new(),
                delay,
                calls: AtomicUsize::new(0),
                concurrent: AtomicUsize::new(0),
                max_concurrent: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Executor for SlowExecutor {
        async fn run(
            &self,
            cmd: &crate::executor::Cmd,
        ) -> Result<crate::executor::CmdOutput, CoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let now = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_concurrent.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.concurrent.fetch_sub(1, Ordering::SeqCst);
            self.inner.run(cmd).await
        }

        async fn read_file(&self, path: &str) -> Result<String, CoreError> {
            self.inner.read_file(path).await
        }

        async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
            self.inner.write_file(path, content, mode).await
        }

        async fn sleep_ms(&self, ms: u64) {
            self.inner.sleep_ms(ms).await
        }
    }

    fn cfg() -> BackupCfg {
        BackupCfg::default()
    }

    /// covers fix-180: an owner nobody has ever read answers `None` at
    /// once — no restic call, no wait — so a cache-miss RPC can answer "not
    /// read yet" instead of blocking behind a restic read that can take
    /// minutes.
    #[tokio::test]
    async fn empty_cache_answers_fast_with_no_entry() {
        let cache = SnapshotCache::new(3);
        let started = std::time::Instant::now();
        assert_eq!(cache.get("jellyfin"), None);
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "a cache miss must never block on I/O"
        );
    }

    /// covers fix-180: two callers that ask to refresh the SAME owner at
    /// the same time get one restic read, not two — the second joins the
    /// first's in-flight read (coalescing) and sees its answer.
    #[tokio::test]
    async fn concurrent_refreshes_of_one_owner_coalesce_into_one_restic_call() {
        let exec = SlowExecutor::new(Duration::from_millis(100));
        let cache = SnapshotCache::new(3);
        let cfg = cfg();

        let r1 = cache.refresh(&exec, &cfg, "jobtracker", 1000);
        let r2 = cache.refresh(&exec, &cfg, "jobtracker", 1000);
        let (r1, r2) = tokio::join!(r1, r2);
        assert_eq!(r1, r2);
        // One `refresh` issues two restic commands (snapshots, then stats);
        // two concurrent refreshes of the SAME owner must still cost only
        // that one pair, not two.
        assert_eq!(
            exec.calls.load(Ordering::SeqCst),
            2,
            "two concurrent refreshes of the same owner must cost one restic read, not two"
        );
        assert_eq!(cache.get("jobtracker"), Some(r1));
    }

    /// covers fix-180: refreshing several DIFFERENT owners never runs more
    /// than `concurrency` restic reads at once — rclone to Google Drive is
    /// rate-limited, and an unbounded burst is what produced the live 502.
    #[tokio::test]
    async fn refreshing_many_owners_is_bounded_by_concurrency() {
        let exec = SlowExecutor::new(Duration::from_millis(80));
        let cache = SnapshotCache::new(2);
        let cfg = cfg();
        let owners = ["a", "b", "c", "d", "e"];

        // Driven concurrently with `join_all` (not spawned) so every future
        // can borrow the same `&exec`/`&cache`/`&cfg`.
        let futs: Vec<_> = owners
            .iter()
            .map(|o| cache.refresh(&exec, &cfg, o, 2000))
            .collect();
        futures_util::future::join_all(futs).await;

        // Two restic commands (snapshots, stats) per owner.
        assert_eq!(exec.calls.load(Ordering::SeqCst), owners.len() * 2);
        assert!(
            exec.max_concurrent.load(Ordering::SeqCst) <= 2,
            "at most 2 restic reads may run at once, saw {}",
            exec.max_concurrent.load(Ordering::SeqCst)
        );
        for o in owners {
            assert!(cache.get(o).is_some());
        }
    }

    /// covers fix-180: a backup/prune/restore that just touched a
    /// repository refreshes it — `refresh` always overwrites whatever was
    /// cached with the newest read, so a repository read once as empty and
    /// then backed up is not stuck showing "no snapshots" forever.
    #[tokio::test]
    async fn refresh_replaces_a_stale_cached_answer() {
        let cache = SnapshotCache::new(3);
        let exec = crate::mock::MockExecutor::new();
        let cfg = cfg();
        exec.enqueue(
            "snapshots --json",
            crate::executor::CmdOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: 0,
            },
        );
        let first = cache.refresh(&exec, &cfg, "gateway", 1).await;
        assert!(first.snapshots.is_empty());

        exec.enqueue(
            "snapshots --json",
            crate::executor::CmdOutput {
                stdout:
                    r#"[{"time":"2026-10-02T03:00:00Z","short_id":"ab12cd34","id":"ab12cd34ef"}]"#
                        .into(),
                stderr: String::new(),
                code: 0,
            },
        );
        let second = cache.refresh(&exec, &cfg, "gateway", 2).await;
        assert_eq!(second.measured_at, 2);
        assert_eq!(cache.get("gateway"), Some(second));
    }

    /// covers fix-180 (rule 20, nothing balloons): a repository no stack
    /// names any more is dropped from the cache; one still named by another
    /// stack's manifest survives — D25 lets two stacks share one owner, so
    /// pruning a cache entry must follow the fleet's owner SET, not one
    /// stack's destroy alone.
    #[tokio::test]
    async fn forget_except_drops_only_owners_outside_the_kept_set() {
        let cache = SnapshotCache::new(3);
        let exec = crate::mock::MockExecutor::new();
        let cfg = cfg();
        cache.refresh(&exec, &cfg, "kept", 1).await;
        cache.refresh(&exec, &cfg, "destroyed", 1).await;

        cache.forget_except(&HashSet::from(["kept".to_string()]));

        assert!(cache.get("kept").is_some());
        assert!(cache.get("destroyed").is_none());
    }
}
