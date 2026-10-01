//! homelab-admin — the browser dashboard for the homelab HOST daemon.
//!
//! Built on chassis-rs (configuration, logging, login, health, the static
//! app and the live channel come from the kit). This file only wires the
//! host link and the dashboard's own routes into it.

use std::sync::Arc;

use chassis::shell::live::Live;
use chassis::shell::pages::Page;
use chassis::shell::webapp::WebApp;
use chassis::{App, AppSpec};
use tokio::sync::RwLock;

use homelab_admin::core::config;
use homelab_admin::shell::guard::{self, Guard};
use homelab_admin::shell::host_link::{self, HostTarget, LinkConfig, Snapshot};
use homelab_admin::shell::routes;

const FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../web/index.html")),
    ("js/main.js", include_bytes!("../web/js/main.js")),
    ("js/fleet.js", include_bytes!("../web/js/fleet.js")),
    ("js/format.js", include_bytes!("../web/js/format.js")),
    ("js/sortkeys.js", include_bytes!("../web/js/sortkeys.js")),
    ("js/router.js", include_bytes!("../web/js/router.js")),
    ("js/store.js", include_bytes!("../web/js/store.js")),
    ("js/dom.js", include_bytes!("../web/js/dom.js")),
    // The words a data table shows while it loads, is empty or failed.
    (
        "js/tablestate.js",
        include_bytes!("../web/js/tablestate.js"),
    ),
    ("js/slowread.js", include_bytes!("../web/js/slowread.js")),
    ("js/activity.js", include_bytes!("../web/js/activity.js")),
    ("js/checks.js", include_bytes!("../web/js/checks.js")),
    ("js/doctor.js", include_bytes!("../web/js/doctor.js")),
    // The read milestone's modules (feat-overview-2/3/4/8, feat-stacks-1,
    // feat-ops-2/4/7, feat-settings-2).
    ("js/ago.js", include_bytes!("../web/js/ago.js")),
    ("js/asks.js", include_bytes!("../web/js/asks.js")),
    ("js/commands.js", include_bytes!("../web/js/commands.js")),
    ("js/host.js", include_bytes!("../web/js/host.js")),
    ("js/logs.js", include_bytes!("../web/js/logs.js")),
    ("js/shortcuts.js", include_bytes!("../web/js/shortcuts.js")),
    ("js/stacktabs.js", include_bytes!("../web/js/stacktabs.js")),
    ("js/timeline.js", include_bytes!("../web/js/timeline.js")),
    ("js/urlstate.js", include_bytes!("../web/js/urlstate.js")),
    ("js/chrome.js", include_bytes!("../web/js/chrome.js")),
    // Milestone act (feat-stacks-4/5/6/7/8, feat-ops-6/8/9, feat-overview-5).
    ("js/act.js", include_bytes!("../web/js/act.js")),
    (
        "js/actionforms.js",
        include_bytes!("../web/js/actionforms.js"),
    ),
    // Milestone follow (feat-platform-10): the form descriptions the server
    // reads too, the driven replay and its pure half.
    (
        "js/formspec.json",
        include_bytes!("../web/js/formspec.json"),
    ),
    ("js/drive.js", include_bytes!("../web/js/drive.js")),
    ("js/driveview.js", include_bytes!("../web/js/driveview.js")),
    // Live view: the announcement bar, the plan, the target's mark.
    (
        "js/driveannounce.js",
        include_bytes!("../web/js/driveannounce.js"),
    ),
    (
        "js/drivehooks.js",
        include_bytes!("../web/js/drivehooks.js"),
    ),
    // Live view cursor: Claude's simulated pointer.
    (
        "js/drivecursor.js",
        include_bytes!("../web/js/drivecursor.js"),
    ),
    // Live view pace: how fast Claude types and picks.
    ("js/drivepace.js", include_bytes!("../web/js/drivepace.js")),
    ("js/editdrive.js", include_bytes!("../web/js/editdrive.js")),
    (
        "js/actiondialog.js",
        include_bytes!("../web/js/actiondialog.js"),
    ),
    (
        "js/actionsarea.js",
        include_bytes!("../web/js/actionsarea.js"),
    ),
    ("js/actui.js", include_bytes!("../web/js/actui.js")),
    ("js/jobpanel.js", include_bytes!("../web/js/jobpanel.js")),
    ("js/jobs.js", include_bytes!("../web/js/jobs.js")),
    ("js/notices.js", include_bytes!("../web/js/notices.js")),
    ("js/rollback.js", include_bytes!("../web/js/rollback.js")),
    (
        "js/rollbackdialog.js",
        include_bytes!("../web/js/rollbackdialog.js"),
    ),
    ("js/schedules.js", include_bytes!("../web/js/schedules.js")),
    (
        "js/pages/host.js",
        include_bytes!("../web/js/pages/host.js"),
    ),
    (
        "js/pages/timeline.js",
        include_bytes!("../web/js/pages/timeline.js"),
    ),
    (
        "js/pages/overview.js",
        include_bytes!("../web/js/pages/overview.js"),
    ),
    (
        "js/pages/stack.js",
        include_bytes!("../web/js/pages/stack.js"),
    ),
    (
        "js/pages/activity.js",
        include_bytes!("../web/js/pages/activity.js"),
    ),
    (
        "js/pages/checks.js",
        include_bytes!("../web/js/pages/checks.js"),
    ),
    (
        "js/pages/doctor.js",
        include_bytes!("../web/js/pages/doctor.js"),
    ),
    (
        "js/pages/jobs.js",
        include_bytes!("../web/js/pages/jobs.js"),
    ),
    (
        "js/pages/schedules.js",
        include_bytes!("../web/js/pages/schedules.js"),
    ),
    (
        "js/pages/notifications.js",
        include_bytes!("../web/js/pages/notifications.js"),
    ),
    // Milestone edit (feat-stacks-2/3, feat-firewall-1/2, feat-settings-1).
    ("js/editforms.js", include_bytes!("../web/js/editforms.js")),
    ("js/editui.js", include_bytes!("../web/js/editui.js")),
    (
        "js/editpanels.js",
        include_bytes!("../web/js/editpanels.js"),
    ),
    ("js/fwview.js", include_bytes!("../web/js/fwview.js")),
    ("js/newstack.js", include_bytes!("../web/js/newstack.js")),
    ("js/plan.js", include_bytes!("../web/js/plan.js")),
    (
        "js/presetseditor.js",
        include_bytes!("../web/js/presetseditor.js"),
    ),
    (
        "js/pages/firewall.js",
        include_bytes!("../web/js/pages/firewall.js"),
    ),
    (
        "js/pages/settings.js",
        include_bytes!("../web/js/pages/settings.js"),
    ),
    // The TUI parity round: today, the live log, the shell, apply, the
    // presets, the version warnings, incident bundles and check answers.
    ("js/parity.js", include_bytes!("../web/js/parity.js")),
    ("js/versions.js", include_bytes!("../web/js/versions.js")),
    ("js/incident.js", include_bytes!("../web/js/incident.js")),
    ("js/answer.js", include_bytes!("../web/js/answer.js")),
    (
        "js/importstack.js",
        include_bytes!("../web/js/importstack.js"),
    ),
    (
        "js/pages/today.js",
        include_bytes!("../web/js/pages/today.js"),
    ),
    ("js/pages/log.js", include_bytes!("../web/js/pages/log.js")),
    (
        "js/pages/shell.js",
        include_bytes!("../web/js/pages/shell.js"),
    ),
    (
        "js/pages/apply.js",
        include_bytes!("../web/js/pages/apply.js"),
    ),
    (
        "js/pages/presets.js",
        include_bytes!("../web/js/pages/presets.js"),
    ),
    (
        "js/pages/home.js",
        include_bytes!("../web/js/pages/home.js"),
    ),
    (
        "js/pages/health.js",
        include_bytes!("../web/js/pages/health.js"),
    ),
    (
        "js/pages/metrics.js",
        include_bytes!("../web/js/pages/metrics.js"),
    ),
    ("js/charts.js", include_bytes!("../web/js/charts.js")),
    // feat-pages-1 (chassis-rs 3.1.0): the page registry itself, and the
    // kit's own pages (Status, Clients, Passkeys) drawn by this web app
    // from /api/kit/status|clients|passkeys (`kit_pages_in_webapp`).
    ("js/pages.js", include_bytes!("../web/js/pages.js")),
    (
        "js/pages/status.js",
        include_bytes!("../web/js/pages/status.js"),
    ),
    (
        "js/pages/clients.js",
        include_bytes!("../web/js/pages/clients.js"),
    ),
    (
        "js/pages/passkeys.js",
        include_bytes!("../web/js/pages/passkeys.js"),
    ),
    ("css/app.css", include_bytes!("../web/css/app.css")),
];

const HELP: &str = "\
The dashboard's own settings are the [admin] table of the config file:
  [admin]
  host = \"10.10.10.250:8443\"                     # homelab-host
  host_token = \"${HOMELAB_ADMIN_HOST_TOKEN}\"    # from the environment
  poll_s = 10  sse_buffer = 256  backoff_min_s = 1  backoff_max_s = 60
  access_team_domain = \"<team>.cloudflareaccess.com\"  access_aud = \"<64 hex>\"
  access_leeway_s = 60  access_certs_refresh_s = 3600
  loki_url = \"http://10.10.10.13:3100\"  loki_timeout_s = 15  ask_timeout_s = 120";

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Two rustls crypto providers are in this binary (chassis: ring, the
    // host link: aws-lc-rs); without a process default the first TLS
    // handshake panics. Measured 2026-09-28 on the first run against the host.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let spec = AppSpec {
        name: "homelab-admin",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/homelab"),
        default_listen: "127.0.0.1:8090",
        help_extra: Some(HELP),
        ..Default::default()
    };
    // Decision notify-routing (2026-09-30): Alertmanager's hook is a public
    // route with its own bearer token; chassis takes public routes here,
    // before the notification centre exists, so it is filled in later.
    let hooks = homelab_admin::shell::actions_notify::HookSlot::default();
    let public = homelab_admin::shell::actions_notify::hooks_router(hooks.clone());
    let mut app = match App::from_env_and_args(spec, public) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let shared = Arc::new(RwLock::new(Snapshot::default()));
    // arch-config: read and check the [admin] table only when this run needs
    // it (a start or --check); --help and --version work without a file.
    let config = if app.needs_project_config() {
        // A developer's file ([admin] table) wins; CT 120 has none and
        // takes every setting from the environment (HOMELAB_ADMIN_*).
        let has_file = app
            .project_table()
            .map(|t| t.contains_key("admin"))
            .unwrap_or(false);
        let source = if has_file {
            "the [admin] table of the config file (host)"
        } else {
            "the environment (HOMELAB_ADMIN_HOST)"
        };
        let loaded = if has_file {
            app.project_config::<config::File>()
                .map_err(|e| e.to_string())
                .and_then(|f| f.admin.expanded(&|k| std::env::var(k).ok()))
                .and_then(|c| c.validate())
        } else {
            config::from_env(&|k| std::env::var(k).ok())
        };
        match loaded {
            Ok(c) => {
                // TUI parity (ping): the address and where it was set.
                let mut s = shared.write().await;
                s.host_addr = Some(c.host.clone());
                s.host_addr_source = Some(source.to_string());
                drop(s);
                Some(c)
            }
            Err(e) => {
                eprintln!("homelab-admin: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let live = Live::new(config.as_ref().map(|c| c.sse_buffer).unwrap_or(256));
    // feat-pages-1 (chassis-rs 3.1.0): the web app mounts at the root by
    // default, is the router's fallback, and `index.html` answers for every
    // extensionless path that is not the kit's. `/app` and `/app/…` from
    // before 3.1.0 get a 308 to the same path at the root (the kit's own).
    app.webapp(WebApp::embedded(FILES));
    // nav-decisions (Kenny, 2026-10-01): Apps (today's tile page, renamed
    // from Home) is the root; Overview is reachable but hidden from the
    // nav, and the brand link ("Homelab") opens it instead. Every other
    // page registers its group (a bar dropdown) and keeps its registration
    // order, which `Page` sorts stably among ties. The kit's own pages
    // (Status, Clients, Passkeys) register themselves; `kit_pages_in_webapp`
    // below has this web app draw them too, in the same bar.
    app.page(Page::new("home", "Apps", "/"))
        .page(Page::new("overview", "Overview", "/overview").hidden())
        .page(Page::new("health", "Health", "/health"))
        .page(Page::new("metrics", "Metrics", "/metrics"))
        .page(Page::new("activity", "Activity", "/activity"))
        .page(Page::new("host", "Host", "/host"))
        .page(Page::new("log", "Live log", "/log").group("Operations"))
        .page(Page::new("jobs", "Jobs", "/jobs").group("Operations"))
        .page(Page::new("apply", "Apply", "/apply").group("Operations"))
        .page(Page::new("schedules", "Schedules", "/schedules").group("Operations"))
        .page(Page::new("firewall", "Firewall", "/firewall").group("Configure"))
        .page(Page::new("backups", "Backups", "/backups").group("Configure"))
        .page(Page::new("secrets", "Secrets", "/secrets").group("Configure"))
        .page(Page::new("settings", "Settings", "/settings").group("Configure"))
        .page(Page::new("fleetview", "Fleet view", "/fleetview").group("Visuals"))
        .page(Page::new("backupcalendar", "Backup calendar", "/backupcalendar").group("Visuals"))
        // Reachable at a fixed address, not shown in the bar (the bell,
        // the shell button and the new-stack wizard link them).
        .page(Page::new("notifications", "Notifications", "/notifications").hidden())
        .page(Page::new("shell", "Shell", "/shell").hidden())
        .page(Page::new("presets", "Presets", "/presets").hidden())
        .brand("/overview")
        .kit_pages_in_webapp();
    app.dashboard_routes(live.router("/events"));
    // Doctor reads for about a minute on pve (fix-68), so the pages wait up
    // to two for an answer.
    let (host_client, host_asks) = host_link::HostClient::new(std::time::Duration::from_secs(120));
    // feat-overview-2, feat-ops-2, feat-ops-4: host facts, the host's
    // questions and the logs from Loki.
    app.dashboard_routes(routes::read_router(routes::ReadCtx {
        shared: shared.clone(),
        host: host_client.clone(),
        live: live.clone(),
        loki: homelab_admin::shell::loki::Loki::from_config(config.as_ref()),
        prometheus: homelab_admin::shell::prometheus::Prometheus::from_config(config.as_ref()),
        traffic_job: config
            .as_ref()
            .and_then(|c| c.traffic_job.clone())
            .filter(|j| !j.trim().is_empty()),
    }));
    // feat-platform-10: a demo host inside this process instead of the real
    // line, for the browser tests; nothing is sent anywhere. Only a build
    // with the `demo-host` feature has one (Kenny: "Only in test builds").
    let demo_host = match demo_requested() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("homelab-admin: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    // milestone act: actions, schedules and notifications.
    if config.is_some() {
        if let Err(e) = homelab_admin::shell::actions::mount(
            &mut app,
            host_client.clone(),
            live.clone(),
            shared.clone(),
            demo_host,
            hooks.clone(),
        ) {
            eprintln!("homelab-admin: {e}");
            return std::process::ExitCode::FAILURE;
        }
    }
    app.dashboard_routes(routes::router(
        shared.clone(),
        host_client,
        std::sync::Arc::new(live.clone()),
    ));

    if let Some(c) = &config {
        // arch-exposure: the two locks, before every route but /healthz.
        if c.dev_without_locks {
            tracing::warn!("dev_without_locks is set: no Access token or home address is checked");
        } else {
            // The refusal page needs its module and kp-themes' files, which
            // are public, before any lock can have passed.
            app.request_guard(|r| async move { guard::refusal_script(&r) });
            app.request_guard_exempt("/static");
            // Alertmanager on CT 113 comes straight in (its firewall rule),
            // without Access or the house's address; the hook's own bearer
            // token decides.
            app.request_guard_exempt("/hooks");
            let guard = Guard::new(c, shared.clone());
            let g = guard.clone();
            app.request_guard(move |r| {
                let g = g.clone();
                async move { g.access(&r).await }
            });
            let g = guard.clone();
            app.request_guard(move |r| {
                let g = g.clone();
                async move { g.home(&r).await }
            });
        }
    }

    if let Some(c) = config {
        // Started only on the serving path: --check never opens the line.
        app.on_start(move || {
            #[cfg(feature = "demo-host")]
            if demo_host {
                let repo =
                    homelab_admin::core::actions_config::from_env(&|k| std::env::var(k).ok())
                        .map(|a| a.repo)
                        .unwrap_or_default();
                let stacks = std::env::var("HOMELAB_ADMIN_DEMO_STACKS")
                    .unwrap_or_else(|_| "films,notes,oldstack".into())
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                tokio::spawn(homelab_admin::shell::demo::run_demo(
                    shared, live, host_asks, repo, stacks,
                ));
                return;
            }
            let _ = demo_host;
            if c.dev_without_locks {
                tracing::warn!(
                    "dev_without_locks is set: this dashboard does not attach for `homelab ui` steps"
                );
            }
            tokio::spawn(host_link::run(
                HostTarget {
                    addr: c.host.clone(),
                    token: c.host_token.clone(),
                },
                LinkConfig::from_admin(&c),
                shared,
                live,
                host_asks,
            ));
        });
    }
    app.run().await
}

/// `HOMELAB_ADMIN_DEMO_HOST=1`: the demo host, in a build that has one. A
/// release build (without the `demo-host` feature) refuses to start rather
/// than quietly open the line to the real host someone meant to avoid.
fn demo_requested() -> Result<bool, String> {
    let asked = std::env::var("HOMELAB_ADMIN_DEMO_HOST").is_ok_and(|v| v == "1");
    if asked && !cfg!(feature = "demo-host") {
        return Err(
            "HOMELAB_ADMIN_DEMO_HOST=1, but this build has no demo host (it is built only with \
             `--features demo-host`, never by make release); unset it to use the real host"
                .into(),
        );
    }
    Ok(asked)
}
