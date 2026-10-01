//! slow-reads (2026-09-29): the long reads ask their questions a few at a
//! time instead of one by one, and read-only probes skip pct's Perl start-up.
//!
//! Measured on pve that day, read-only: `homelab doctor` 27 s, `homelab check`
//! 67 s, the dashboard's Today 93 s; `pct exec 109 -- true` 0.45-0.48 s against
//! `lxc-attach -n 109 -- true` 0.01 s. These tests hold the concurrent version
//! to the sequential one: the same facts in the same order, a failing
//! container costing only itself, the no-touch guests never asked, and no
//! `pct` start-up paid per container.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor, MockExecutor};
use homelab_core::ops::facts::{gather_live_facts, gather_live_facts_with, FactsInputs};
use homelab_core::ops::pool::{bounded, Done, READ_CONCURRENCY};

/// The mock with a wall-clock delay on every command and file read, and a
/// count of what was asked about each container. A delay per call is what
/// the real host has (pct's start-up, a container answering), so the order
/// in which answers arrive differs from the order in which they were asked.
struct Slow {
    inner: MockExecutor,
    /// Delay for a call whose rendered form contains the key; else `default`.
    delays: Vec<(String, u64)>,
    default_ms: u64,
    log: Mutex<Vec<String>>,
}

impl Slow {
    fn new(inner: MockExecutor, default_ms: u64) -> Self {
        Self {
            inner,
            delays: Vec::new(),
            default_ms,
            log: Mutex::new(Vec::new()),
        }
    }
    fn delay_for(&self, what: &str) -> u64 {
        self.delays
            .iter()
            .find(|(k, _)| what.contains(k.as_str()))
            .map(|(_, ms)| *ms)
            .unwrap_or(self.default_ms)
    }
    fn logged(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

#[async_trait]
impl Executor for Slow {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        let r = cmd.rendered();
        self.log.lock().unwrap().push(r.clone());
        tokio::time::sleep(Duration::from_millis(self.delay_for(&r))).await;
        self.inner.run(cmd).await
    }
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        self.inner.write_file(path, content, mode).await
    }
    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        self.log.lock().unwrap().push(format!("read {}", path));
        tokio::time::sleep(Duration::from_millis(self.delay_for(path))).await;
        self.inner.read_file(path).await
    }
    async fn sleep_ms(&self, _ms: u64) {}
}

fn inputs() -> FactsInputs {
    FactsInputs {
        watched_backups: vec![],
        state_dir: "/var/lib/homelab".into(),
        gateway_vmid: 104,
        gateway_routes_dir: "/appdata/gateway/traefik-config/routes".into(),
        no_touch: vec![100, 101, 102, 103],
        prometheus_url: None,
        loki_url: None,
        loki_vmid: None,
        logs_window: "24h".into(),
        now_unix: 1_789_704_000,
        watched_fresh: true,
    }
}

/// The fleet as pve had it on 2026-09-29: the four no-touch guests and 17
/// managed containers. 107 does not answer at all (a failed attach), 111 is
/// stopped (no output from inside, but its configuration is there).
fn fleet() -> MockExecutor {
    let exec = MockExecutor::new();
    let mut list = String::from("VMID Status Lock Name\n");
    for v in [100u16, 101, 102, 103] {
        list.push_str(&format!("{v} running {v}-legacy\n"));
    }
    for v in 104u16..=120 {
        list.push_str(&format!("{v} running {v}-app\n"));
    }
    exec.respond_always("pct list", CmdOutput::ok(&list));
    for v in 104u16..=120 {
        let (growth, patch) = match v {
            107 => (
                CmdOutput::failed(1, "lxc-attach: failed to get init pid"),
                CmdOutput::failed(1, "lxc-attach: failed to get init pid"),
            ),
            111 => (CmdOutput::ok(""), CmdOutput::failed(1, "not running")),
            _ => (
                CmdOutput::ok(&format!(
                    "disk={}\nmem=37\nswap=0\njournal=12\ndockerlogs=3\nguards=1\n",
                    v - 60
                )),
                CmdOutput::ok(&format!("{}\n-\n1789700000\n1789000000\n", v - 100)),
            ),
        };
        let at = format!(
            "lxc-attach -n {v} --clear-env --set-var {} -- sh -c ",
            homelab_core::executor::ATTACH_PATH
        );
        exec.respond_always(&format!("{at}df"), growth);
        exec.respond_always(&format!("{at}apt-get"), patch);
        exec.seed_file(
            &format!("/etc/pve/lxc/{v}.conf"),
            &format!(
                "onboot: 1\nmemory: {}\n\n[snap1]\nmemory: 1\n\n[pve:pending]\nmemory: 2\n",
                v as u32 * 10
            ),
        );
    }
    exec
}

/// The pool itself: whatever order the answers come in, the results are in
/// the order the work went in, exactly as the sequential loop (limit 1)
/// returns them; an item that fails costs only itself.
#[tokio::test]
async fn slow_reads_the_pool_keeps_input_order_and_a_failure_stops_nothing() {
    // Later items finish first: 17 → 1 ms down to 1 ms.
    let items: Vec<u64> = (1..=17).collect();
    let work = |i: u64| async move {
        tokio::time::sleep(Duration::from_millis(40 - 2 * i)).await;
        if i == 5 {
            Err(format!("container {i} did not answer"))
        } else {
            Ok(i * 10)
        }
    };
    let sequential = bounded(items.iter().map(|i| work(*i)).collect(), 1).await;
    let started = Instant::now();
    let parallel = bounded(items.iter().map(|i| work(*i)).collect(), READ_CONCURRENCY).await;
    let took = started.elapsed();
    assert_eq!(parallel, sequential, "same results, same order");
    assert_eq!(parallel.len(), 17);
    assert!(parallel[4].is_err(), "{:?}", parallel[4]);
    assert_eq!(
        parallel.iter().filter(|r| r.is_ok()).count(),
        16,
        "the other sixteen still answered"
    );
    // Sequential is ~0.4 s; eight at a time is a fraction of that.
    assert!(took < Duration::from_millis(250), "took {took:?}");

    // The progress count climbs 1..n whatever order items finish in.
    let done = Done::new(3);
    assert_eq!(
        [done.tick(), done.tick(), done.tick()],
        ["1/3".to_string(), "2/3".into(), "3/3".into()]
    );
}

/// The whole gatherer, run with answers that arrive out of order, against the
/// same fleet answered instantly (which the pool drives strictly in order):
/// identical facts. Growth, configuration and patch facts come out sorted by
/// vmid as before; the container that failed and the stopped one are missing
/// only where they were missing before.
#[tokio::test]
async fn slow_reads_the_concurrent_gather_equals_the_sequential_one() {
    let (instant, _) = gather_live_facts(&fleet(), &inputs(), &[]).await;

    let mut slow = Slow::new(fleet(), 5);
    // Every container answers slower the lower its vmid, so the pool gets
    // the answers back in reverse.
    for v in 104u16..=120 {
        slow.delays
            .push((format!("-n {v} "), 5 + (121 - v as u64) * 3));
    }
    let lines = Mutex::new(Vec::<String>::new());
    let progress = |l: &str| lines.lock().unwrap().push(l.to_string());
    let (facts, _) = gather_live_facts_with(&slow, &inputs(), &[], &progress).await;

    assert_eq!(format!("{:?}", facts), format!("{:?}", instant));
    let growth: Vec<u16> = facts.growth.iter().map(|g| g.vmid).collect();
    let want: Vec<u16> = (104..=120).filter(|v| *v != 107 && *v != 111).collect();
    assert_eq!(growth, want, "sorted by vmid; 107 failed, 111 stopped");
    let boot: Vec<u16> = facts.boot.iter().map(|b| b.vmid).collect();
    assert_eq!(boot, (104..=120).collect::<Vec<_>>(), "config read for all");
    assert_eq!(
        facts.boot[0].live.memory_mb,
        Some(1040),
        "the current section, not a snapshot's or the pending one"
    );
    let patch: Vec<u16> = facts.patch.iter().map(|p| p.vmid).collect();
    assert_eq!(patch, want);

    // Progress: one line per container, the count climbing 1..17 as they
    // answer (in reverse vmid order here), the phase lines unchanged.
    let lines = lines.into_inner().unwrap();
    let counted: Vec<&String> = lines.iter().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(counted.len(), 17, "{lines:?}");
    for (i, l) in counted.iter().enumerate() {
        assert!(l.starts_with(&format!("  {}/17 ", i + 1)), "{lines:?}");
    }
    // Eight in flight: 104..=111 go first and 111, the fastest of them,
    // answers first; the answers really did arrive out of vmid order.
    assert!(counted[0].ends_with(" 111-app"), "{lines:?}");
    assert!(lines
        .iter()
        .any(|l| l == "probing 17 container(s): disk, memory, logs, guards…"));
    assert!(lines
        .iter()
        .any(|l| l == "asking each container about pending updates…"));
}

/// The speed-up, measured on the mock with pve's per-call cost: 0.45 s for a
/// `pct` call before, 0.01 s for `lxc-attach` and ~0 for a file read after,
/// scaled down 1:10 so the test stays fast. Before: three rounds of 17 pct
/// calls one by one = 51 × 45 ms ≈ 2.3 s. After: three rounds, eight at a
/// time, no pct start-up.
#[tokio::test]
async fn slow_reads_the_container_rounds_take_a_fraction_of_the_sequential_time() {
    let mut slow = Slow::new(fleet(), 1);
    slow.delays.push(("lxc-attach".into(), 1));
    slow.delays.push(("/etc/pve/lxc/".into(), 0));
    slow.delays.push(("pct ".into(), 45));
    let started = Instant::now();
    let _ = gather_live_facts(&slow, &inputs(), &[]).await;
    let took = started.elapsed();
    let sequential_pct = Duration::from_millis(51 * 45);
    eprintln!("gather on the 1:10 pve model: {took:?}; the old rounds alone: {sequential_pct:?}");
    assert!(
        took * 10 < sequential_pct,
        "{took:?} against {sequential_pct:?} for the old rounds"
    );
}

/// Per managed container, what the check asks: two `lxc-attach` probes
/// (growth, patch) and one configuration read, and no `pct` call at all.
/// Before this change it was three `pct` calls (exec, config, exec), each
/// paying ~0.45 s of Perl start-up. The no-touch guests in the pct list get
/// nothing: no attach, no configuration read, no pct.
#[tokio::test]
async fn slow_reads_no_pct_per_container_and_nothing_at_all_for_the_no_touch_guests() {
    let slow = Slow::new(fleet(), 0);
    let _ = gather_live_facts(&slow, &inputs(), &[]).await;
    let log = slow.logged();
    let mut per: HashMap<u16, (usize, usize, usize)> = HashMap::new();
    for v in 100u16..=120 {
        let pct = log
            .iter()
            .filter(|c| c.starts_with("pct ") && c.contains(&format!(" {v} ")))
            .count();
        let attach = log
            .iter()
            .filter(|c| c.starts_with(&format!("lxc-attach -n {v} ")))
            .count();
        let conf = log
            .iter()
            .filter(|c| c.contains(&format!("/{v}.conf")))
            .count();
        per.insert(v, (pct, attach, conf));
    }
    for v in 100u16..=103 {
        assert_eq!(per[&v], (0, 0, 0), "no-touch {v} was asked: {log:?}");
    }
    for v in 105u16..=120 {
        assert_eq!(per[&v], (0, 2, 1), "container {v}: {log:?}");
    }
    // The gateway (104) also carries the route listing and fragment read.
    assert_eq!(per[&104].0, 0, "{log:?}");
    assert_eq!(
        log.iter().filter(|c| c.starts_with("pct ")).count(),
        1,
        "one `pct list` for the whole fleet: {log:?}"
    );
}
