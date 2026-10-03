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

// The web files are generated from admin/web by build.rs (fix-179 follow-up).
include!(concat!(env!("OUT_DIR"), "/web_files.rs"));

const HELP: &str = "\
The dashboard's own settings are the [admin] table of the config file:
  [admin]
  host = \"10.10.10.250:8443\"                     # homelab-host
  host_token = \"${HOMELAB_ADMIN_HOST_TOKEN}\"    # from the environment
  poll_s = 10  sse_buffer = 256  backoff_min_s = 1  backoff_max_s = 60
  access_team_domain = \"<team>.cloudflareaccess.com\"  access_aud = \"<64 hex>\"
  access_leeway_s = 60  access_certs_refresh_s = 3600
  loki_url = \"http://10.10.10.13:3100\"  loki_timeout_s = 15  ask_timeout_s = 600";

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
    // feat-shell-1 (redesign 3.71.0, Kenny approved 2026-10-03): six areas,
    // in the order a person needs them — `Apps · Inbox · Stacks · Activity
    // │ Backups · System` — are the bar's only links (admin/web/js/areas.js
    // holds the same list; a web test holds the two equal). Every other
    // page belongs to an area (its `group`) and is routable but not listed:
    // System's landing page, the breadcrumbs and the command palette reach
    // them. The brand link opens `/`, which shows the Inbox when it holds
    // something and Apps otherwise. Every address a page had before 3.71.0
    // still works: the web app's router redirects it (router.js REDIRECTS).
    app.page(Page::new("home", "Apps", "/apps"))
        .page(Page::new("inbox", "Inbox", "/inbox"))
        .page(Page::new("overview", "Stacks", "/stacks"))
        .page(Page::new("activity", "Activity", "/activity"))
        .page(Page::new("backups", "Backups", "/backups"))
        .page(Page::new("system", "System", "/system"))
        .page(Page::new("host", "Host", "/host").group("System").hidden())
        // redesign-flows-11: the Update flow's own address; the nav marks
        // Stacks for one stack's update and the Inbox for every app's.
        .page(
            Page::new("update", "Update apps", "/update")
                .group("Inbox")
                .hidden(),
        )
        // fix-206: chassis reserves `/metrics` for its own Prometheus
        // scrape text, unconditionally; "/charts" is this page's path.
        .page(
            Page::new("metrics", "Metrics", "/charts")
                .group("System")
                .hidden(),
        )
        .page(
            Page::new("fleetview", "Map", "/map")
                .group("System")
                .hidden(),
        )
        .page(
            Page::new("firewall", "Firewall", "/firewall")
                .group("System")
                .hidden(),
        )
        .page(
            Page::new("settings", "Host settings", "/settings")
                .group("System")
                .hidden(),
        )
        .page(
            Page::new("presets", "Presets", "/presets")
                .group("System")
                .hidden(),
        )
        .page(
            Page::new(
                "notifications",
                "Notification rules",
                "/system/notifications",
            )
            .group("System")
            .hidden(),
        )
        .page(
            Page::new("shell", "Console", "/console")
                .group("System")
                .hidden(),
        )
        .brand("/")
        .brand_title("Homelab")
        .kit_pages_in_webapp()
        // Passkeys is drawn as Host settings › Sign-in since 3.71.0 (its
        // `/passkeys` address redirects there), so it leaves the bar.
        .kit_page("passkeys", |p| p.hidden().group("System"))
        // Kenny, 2026-10-02: the kit's Status page duplicates Health, and
        // nothing calls this dashboard's client API, so both are switched
        // off; Passkeys stays (it is how Kenny logs in).
        .disable_kit_page("status")
        .disable_kit_page("clients");
    app.dashboard_routes(live.router("/events"));
    app.dashboard_routes(kit_paths_router());
    // Doctor reads for about a minute on pve (fix-68), so the pages wait up
    // to two for an answer.
    let (host_client, host_asks) = host_link::HostClient::new(std::time::Duration::from_secs(120));
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
    // fix-220: the demo host's own made-up Prometheus/Loki (never a real
    // metrics stack — spawn_demo_metrics starts one inside this process),
    // so the charts and traffic pages have something to draw in the browser
    // tests, and the invariants suite can prove the dashboard never shows a
    // raw id as a chart series label, against the actual humanizing code
    // path rather than asserting on it directly. Only fills in what the
    // demo build's own config did NOT already set.
    #[cfg_attr(not(feature = "demo-host"), allow(unused_mut))]
    let mut config = config;
    #[cfg(feature = "demo-host")]
    if demo_host && let Some(c) = config.as_mut() {
        let base = homelab_admin::shell::demo::spawn_demo_metrics().await;
        c.prometheus_url.get_or_insert_with(|| base.clone());
        c.loki_url.get_or_insert_with(|| base.clone());
        c.charts_host.get_or_insert_with(|| "demo".into());
        c.traffic_job.get_or_insert_with(|| "demo-traffic".into());
    }
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
    // milestone act: actions, schedules and notifications.
    if config.is_some()
        && let Err(e) = homelab_admin::shell::actions::mount(
            &mut app,
            host_client.clone(),
            live.clone(),
            shared.clone(),
            demo_host,
            hooks.clone(),
        )
    {
        eprintln!("homelab-admin: {e}");
        return std::process::ExitCode::FAILURE;
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

/// fix-256 (design review, 2026-10-03): chassis 3.4.0's root fallback
/// answers "no such route" for every path the kit reserves, `/status` and
/// `/passkeys` included, even with `kit_pages_in_webapp` (which only stops
/// the kit drawing those pages itself) — so the Passkeys page and the old
/// Status address were dead, in the real build as much as the demo. These
/// two GET routes hand both to the web app's `index.html`, the same file
/// the fallback serves for every other page: the client router draws
/// Passkeys and sends `/status` on to Health (`router.js` `redirectFor`).
/// Behind the same login as every other dashboard route.
fn kit_paths_router() -> axum::Router {
    use axum::http::header;
    use axum::response::IntoResponse;
    async fn index() -> axum::response::Response {
        let body = FILES
            .iter()
            .find(|(p, _)| *p == "index.html")
            .map(|(_, b)| *b)
            .unwrap_or_default();
        (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    chassis::shell::webapp::DEFAULT_CSP,
                ),
            ],
            body,
        )
            .into_response()
    }
    axum::Router::new()
        .route("/status", axum::routing::get(index))
        .route("/passkeys", axum::routing::get(index))
}
