//! homelab-admin — the browser dashboard for the homelab HOST daemon.
//!
//! Built on chassis-rs (configuration, logging, login, health, the static
//! app and the live channel come from the kit). This file only wires the
//! host link and the dashboard's own routes into it.

use std::sync::Arc;

use axum::Router;
use chassis::shell::live::Live;
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
    ("js/activity.js", include_bytes!("../web/js/activity.js")),
    ("js/checks.js", include_bytes!("../web/js/checks.js")),
    ("js/doctor.js", include_bytes!("../web/js/doctor.js")),
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
    ("css/app.css", include_bytes!("../web/css/app.css")),
];

const HELP: &str = "\
The dashboard's own settings are the [admin] table of the config file:
  [admin]
  host = \"10.10.10.250:8443\"                     # homelab-host
  host_token = \"${HOMELAB_ADMIN_HOST_TOKEN}\"    # from the environment
  poll_s = 10  sse_buffer = 256  backoff_min_s = 1  backoff_max_s = 60
  access_team_domain = \"<team>.cloudflareaccess.com\"  access_aud = \"<64 hex>\"
  access_leeway_s = 60  access_certs_refresh_s = 3600";

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
    let mut app = match App::from_env_and_args(spec, Router::new()) {
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
        let loaded = if has_file {
            app.project_config::<config::File>()
                .map_err(|e| e.to_string())
                .and_then(|f| f.admin.expanded(&|k| std::env::var(k).ok()))
                .and_then(|c| c.validate())
        } else {
            config::from_env(&|k| std::env::var(k).ok())
        };
        match loaded {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("homelab-admin: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let live = Live::new(config.as_ref().map(|c| c.sse_buffer).unwrap_or(256));
    app.webapp(WebApp::embedded(FILES));
    app.nav_entry("Fleet", "/app/");
    app.dashboard_routes(live.router("/events"));
    // Doctor reads for about a minute on pve (fix-68), so the pages wait up
    // to two for an answer.
    let (host_client, host_asks) = host_link::HostClient::new(std::time::Duration::from_secs(120));
    app.dashboard_routes(routes::router(shared.clone(), host_client));

    if let Some(c) = &config {
        // arch-exposure: the two locks, before every route but /healthz.
        if c.dev_without_locks {
            tracing::warn!("dev_without_locks is set: no Access token or home address is checked");
        } else {
            // The refusal page needs its module and kp-themes' files, which
            // are public, before any lock can have passed.
            app.request_guard(|r| async move { guard::refusal_script(&r) });
            app.request_guard_exempt("/static");
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
            let (backoff_min, backoff_max) = c.backoff();
            tokio::spawn(host_link::run(
                HostTarget {
                    addr: c.host.clone(),
                    token: c.host_token.clone(),
                },
                LinkConfig {
                    poll: c.poll(),
                    backoff_min,
                    backoff_max,
                },
                shared,
                live,
                host_asks,
            ));
        });
    }
    app.run().await
}
