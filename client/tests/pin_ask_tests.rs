//! Decision "Fleet check speed" (2026-09-29): the pinned-digest check asks
//! the registries 8 at a time, keeps the order, asks an identical reference
//! once and stops asking a registry that did not answer.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use homelab_client::pinexists::{answer_pins, PIN_ASK_WIDTH};
use homelab_core::ops::pinexists::{PinAnswer, PinnedDigest};

fn pin(registry: &str, n: usize) -> PinnedDigest {
    PinnedDigest {
        stack: format!("s{}", n),
        registry: registry.to_string(),
        repository: format!("owner/app{}", n),
        digest: format!("sha256:{:064x}", n),
        reference: format!("{}/owner/app{}@sha256:{:064x}", registry, n, n),
    }
}

#[test]
fn pins_are_asked_eight_at_a_time_and_answered_in_order() {
    let pins: Vec<PinnedDigest> = (0..16).map(|n| pin("ghcr.io", n)).collect();
    let running = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let started = Instant::now();
    let answers = answer_pins(pins.clone(), PIN_ASK_WIDTH, |p| {
        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        // Later pins answer faster, so a completion-order bug shows.
        let n: u64 = p.stack[1..].parse().unwrap();
        std::thread::sleep(Duration::from_millis(200 - n * 10));
        running.fetch_sub(1, Ordering::SeqCst);
        if n.is_multiple_of(3) {
            PinAnswer::Missing
        } else {
            PinAnswer::Present
        }
    });
    let took = started.elapsed();
    assert_eq!(PIN_ASK_WIDTH, 8);
    assert_eq!(
        peak.load(Ordering::SeqCst),
        8,
        "never more, and really 8, at once"
    );
    assert!(took < Duration::from_millis(1500), "took {:?}", took);
    let got: Vec<_> = answers.iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(got, pins);
    for (p, a) in &answers {
        let n: usize = p.stack[1..].parse().unwrap();
        let want = if n.is_multiple_of(3) {
            PinAnswer::Missing
        } else {
            PinAnswer::Present
        };
        assert_eq!(a, &want, "{}", p.reference);
    }
}

#[test]
fn an_identical_reference_is_asked_once() {
    let mut pins: Vec<PinnedDigest> = (0..4).map(|n| pin("ghcr.io", n)).collect();
    let mut again = pin("ghcr.io", 1);
    again.stack = "other".into();
    pins.push(again);
    let asked = Mutex::new(Vec::new());
    let answers = answer_pins(pins, PIN_ASK_WIDTH, |p| {
        asked.lock().unwrap().push(p.reference.clone());
        PinAnswer::Present
    });
    let mut asked = asked.into_inner().unwrap();
    asked.sort();
    asked.dedup();
    assert_eq!(asked.len(), 4);
    assert_eq!(answers.len(), 5);
    assert_eq!(answers[4].0.stack, "other");
    assert_eq!(answers[4].1, PinAnswer::Present);
}

#[test]
fn a_registry_that_did_not_answer_is_reported_the_same_for_every_pin() {
    let mut pins = Vec::new();
    for n in 0..20usize {
        pins.push(pin(
            if n.is_multiple_of(2) {
                "dead.io"
            } else {
                "ghcr.io"
            },
            n,
        ));
    }
    let dead_asks = AtomicUsize::new(0);
    for _ in 0..5 {
        let answers = answer_pins(pins.clone(), PIN_ASK_WIDTH, |p| {
            if p.registry == "dead.io" {
                dead_asks.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                PinAnswer::NotAsked("dead.io did not answer".into())
            } else {
                PinAnswer::Present
            }
        });
        for (p, a) in &answers {
            if p.registry == "dead.io" {
                assert_eq!(a, &PinAnswer::NotAsked("dead.io did not answer".into()));
            } else {
                assert_eq!(a, &PinAnswer::Present);
            }
        }
    }
    // Best effort with 8 at a time: at most one wave of the 10 dead pins is
    // asked before the registry is known dead, never all of them each run.
    assert!(dead_asks.load(Ordering::SeqCst) <= 5 * PIN_ASK_WIDTH);
}
