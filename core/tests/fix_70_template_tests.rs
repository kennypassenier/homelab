//! fix-70 (native-uptime-panel-wrong, 2026-10-01): the generated dashboards'
//! "Service uptime" panel now reads `node_systemd_unit_state` and each
//! service's own `*_uptime_seconds`, but `node_systemd_unit_state` only
//! carries a unit's start time when node_exporter runs with
//! `--collector.systemd.enable-start-time-metrics` — a flag the Debian
//! package's default ARGS file does not set. This proves the golden
//! template build declares that flag, rather than leaving it to be
//! rediscovered the next time a dashboard shows nothing.

use homelab_core::executor::MockExecutor;
use homelab_core::ops::template::{build_template, TemplateCfg, NODE_EXPORTER_ARGS};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

#[tokio::test]
async fn fix_70_the_template_declares_systemd_start_time_metrics() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "systemctl is-system-running",
        homelab_core::executor::CmdOutput::ok("running"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let octx = ctx(&exec, &sink, &journal);
    let cfg = TemplateCfg {
        temp_vmid: 999,
        ..Default::default()
    };

    let report = build_template(&octx, &cfg).await;
    assert!(report.ok, "template build failed: {:?}", report);

    assert_eq!(
        exec.ran("systemctl", &["restart", "prometheus-node-exporter"]),
        1,
        "node_exporter must be restarted after its ARGS file is rewritten"
    );
    let configured = exec.calls_containing("prometheus-node-exporter");
    assert!(
        configured
            .iter()
            .any(|c| c.contains("enable-start-time-metrics")),
        "the declared flag must reach the container, not just this file: {:?}",
        configured
    );
    // The exact flag text is declared once (`NODE_EXPORTER_ARGS`) and used
    // here and in production — this guards against a copy drifting from it.
    assert!(configured
        .iter()
        .any(|c| c.contains(NODE_EXPORTER_ARGS.trim())));
}
