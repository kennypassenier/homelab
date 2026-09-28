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
use homelab_admin::shell::host_link::{self, HostTarget, LinkConfig, Snapshot};
use homelab_admin::shell::routes;

const FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../web/index.html")),
    ("js/main.js", include_bytes!("../web/js/main.js")),
    ("js/fleet.js", include_bytes!("../web/js/fleet.js")),
    ("css/app.css", include_bytes!("../web/css/app.css")),
];

const HELP: &str = "\
The dashboard's own settings are the [admin] table of the config file:
  [admin]
  host = \"10.10.10.250:8443\"                     # homelab-host
  host_token = \"${HOMELAB_ADMIN_HOST_TOKEN}\"    # from the environment
  poll_s = 10  sse_buffer = 256  backoff_min_s = 1  backoff_max_s = 60";

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
        match app
            .project_config::<config::File>()
            .map_err(|e| e.to_string())
            .and_then(|f| f.admin.expanded(&|k| std::env::var(k).ok()))
            .and_then(|c| c.validate())
        {
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
    app.dashboard_routes(routes::router(shared.clone()));

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
            ));
        });
    }
    app.run().await
}
