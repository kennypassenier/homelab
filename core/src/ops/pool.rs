//! Bounded concurrency for the long reads: `homelab check`, `homelab doctor`
//! and `homelab today`.
//!
//! Measured on pve on 2026-09-29 (read-only, v3.63.2): doctor 27 s, the fleet
//! check 67 s, and the dashboard's Today, which asked one after the other,
//! 93 s. Almost all of it was waiting: one `pct exec` at a time into 17
//! containers, three rounds of them, then one route probe at a time, then one
//! Prometheus and one Loki question per stack. Each of those questions is
//! independent of the others, so they are asked [`READ_CONCURRENCY`] at a
//! time. What is asked, and of whom, is unchanged; only the waiting overlaps.
//!
//! Results always come back in the order the items went in, whatever order
//! the answers arrive in, so every list the check judges (and every line it
//! prints) reads the same as it did when the questions were asked one by one.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_util::stream::{self, StreamExt};

/// How many questions of one round are in flight at once. Eight keeps the
/// hypervisor's `pct exec` fan-out modest (17 containers take three waves)
/// while taking most of the waiting out.
pub const READ_CONCURRENCY: usize = 8;

/// Drive `futures`, at most `limit` at a time, and return their outputs in
/// the order the futures were given. A `limit` of 0 is treated as 1
/// (sequential).
///
/// Takes the futures already built (`items.iter().map(|x| async move { … })
/// .collect()`) rather than a closure: a closure inside the stream ends up in
/// the caller's future type, and the host's request handler, which must be
/// `Send` for every lifetime, then fails to type-check (rust-lang/rust#102211).
///
/// Each future stands alone: one that returns an error value, or a `None`,
/// does not stop the others. (A panic would, as it would have in the
/// sequential loop; nothing here panics on a failed command.)
pub async fn bounded<Fut: Future>(futures: Vec<Fut>, limit: usize) -> Vec<Fut::Output> {
    stream::iter(futures).buffered(limit.max(1)).collect().await
}

/// Counts completions for progress lines: `n/total` where `n` is how many
/// items of the round have FINISHED, so the count only ever goes up even
/// though the items finish out of order.
#[derive(Debug)]
pub struct Done {
    n: AtomicUsize,
    total: usize,
}

impl Done {
    pub fn new(total: usize) -> Self {
        Self {
            n: AtomicUsize::new(0),
            total,
        }
    }

    /// Mark one item finished and return its `n/total` label.
    pub fn tick(&self) -> String {
        let n = self.n.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{}/{}", n, self.total)
    }
}
