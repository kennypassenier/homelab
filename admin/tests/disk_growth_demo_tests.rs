//! redesign-host-4 (3.71.0): the Host page's growth line reads
//! `/data/disk-growth`, which fits the hypervisor's own filesystems from
//! Prometheus history. The demo host's made-up Prometheus answers that
//! query with a week of history, so the line is real in the demo and the
//! browser tests; this proves the same client and fit the route uses turn
//! it into a growth for `/`.
#![cfg(feature = "demo-host")]

use std::time::Duration;

use homelab_admin::shell::demo::spawn_demo_metrics;
use homelab_admin::shell::prometheus::Prometheus;

#[tokio::test]
async fn redesign_host_4_the_demo_prometheus_has_a_week_of_root_history_that_fits_a_growth() {
    let base = spawn_demo_metrics().await;
    let prom = Prometheus::new(base, Duration::from_secs(5), Some("demo".into())).unwrap();
    let panel = homelab_core::charts::host_disk_growth_query("demo");
    let end = 1_790_000_000u64;
    let series = prom
        .range(
            &panel.query,
            end - 7 * 86_400,
            end,
            3_600,
            panel.legend.as_deref(),
        )
        .await
        .expect("the demo Prometheus answers");
    let root = series
        .iter()
        .find(|s| s["label"] == "/")
        .expect("the root filesystem is a series of its own");
    let points: Vec<(f64, f64)> = root["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
        .collect();
    assert!(points.len() >= 7 * 4, "a week of points: {}", points.len());
    let fit = homelab_core::diskgrowth::fit(&points).expect("enough history to fit");
    assert!(
        (fit.pct_per_day_robust - 0.03).abs() < 1e-6,
        "{}",
        fit.pct_per_day_robust
    );
    assert!((fit.pct_now - 31.0).abs() < 1e-6);
    assert!(fit.days_to_full.is_some_and(|d| d > 365.0));
    // The other queries keep their one-point answer (the charts pages).
    let other = prom
        .instant("node_load1{host=\"demo\"}", None)
        .await
        .unwrap();
    assert_eq!(other.len(), 1);
}
