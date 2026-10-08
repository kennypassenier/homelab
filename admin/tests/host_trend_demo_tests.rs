//! fix-371-1 (Kenny approved the Host tiles demo 2026-10-04): the Host
//! page's eight KPI tiles read `/data/host/trend`, one Prometheus series
//! per figure. The demo host's made-up Prometheus answers each of the
//! eight queries with exactly one series over the window asked for, so the
//! tiles are real in the demo and the browser tests.
#![cfg(feature = "demo-host")]

use std::time::Duration;

use homelab_admin::shell::demo::spawn_demo_metrics;
use homelab_admin::shell::prometheus::Prometheus;

#[tokio::test]
async fn fix_371_1_every_host_tile_figure_is_one_series_over_a_day() {
    let base = spawn_demo_metrics().await;
    let prom = Prometheus::new(base, Duration::from_secs(5), Some("demo".into())).unwrap();
    let end = 1_790_000_000u64;
    let figures = homelab_core::charts::host_kpi_queries("demo");
    assert_eq!(
        figures.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        [
            "cpu", "memory", "load", "disk", "diskio", "network", "temp", "swap"
        ]
    );
    for (key, q) in figures {
        let series = prom
            .range(&q, end - 86_400, end, 600, None)
            .await
            .expect("the demo Prometheus answers");
        assert_eq!(series.len(), 1, "{key}: {} series", series.len());
        let points = series[0]["points"].as_array().unwrap();
        assert!(points.len() >= 144, "{key}: {} points", points.len());
        let last = points.last().unwrap()[1].as_f64().unwrap();
        match key {
            "memory" => assert!((last - 39.7).abs() < 1e-6, "{last}"),
            "disk" => assert!((last - 31.0).abs() < 1e-6, "{last}"),
            "cpu" | "swap" => assert!((0.0..=100.0).contains(&last), "{key}: {last}"),
            _ => assert!(last > 0.0, "{key}: {last}"),
        }
    }
}

/// fix-371-1: the Disk traffic, Network and Swap tiles land on a chart of
/// their own; the demo host's Prometheus draws each of them (read and
/// written, in and out, one swap line).
#[tokio::test]
async fn fix_371_1_the_tiles_own_charts_answer_in_the_demo() {
    let base = spawn_demo_metrics().await;
    let prom = Prometheus::new(base, Duration::from_secs(5), Some("demo".into())).unwrap();
    let end = 1_790_000_000u64;
    let panels = homelab_core::charts::host_panels("demo");
    for (title, labels) in [
        ("Disk traffic", vec!["read", "written"]),
        ("Network in and out", vec!["in", "out"]),
        ("Swap used", vec![""]),
    ] {
        let p = panels.iter().find(|p| p.title == title).expect(title);
        let series = prom
            .range(&p.query, end - 86_400, end, 600, p.legend.as_deref())
            .await
            .expect("the demo Prometheus answers");
        let got: Vec<&str> = series
            .iter()
            .map(|s| s["label"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(got, labels, "{title}");
        for s in &series {
            let points = s["points"].as_array().unwrap();
            assert!(points.len() >= 144, "{title}: {} points", points.len());
        }
    }
}
