//! homelab-admin — the browser dashboard for the homelab HOST daemon.
//!
//! Built on chassis-rs (configuration, logging, login, health, the static
//! app and the live channel come from the kit). This file only wires the
//! host link and the dashboard's own routes into it.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use chassis::shell::live::Live;
use chassis::shell::webapp::WebApp;
use chassis::{App, AppSpec};
use tokio::sync::RwLock;

use homelab_admin::shell::host_link::{self, HostTarget, LinkConfig, Snapshot};
use homelab_admin::shell::routes;

const FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../web/index.html")),
    ("js/main.js", include_bytes!("../web/js/main.js")),
    ("js/fleet.js", include_bytes!("../web/js/fleet.js")),
    ("css/app.css", include_bytes!("../web/css/app.css")),
];

const HELP: &str = "\
Host link (skeleton; moves into the config file with arch-config):
  HOMELAB_ADMIN_HOST        host:port of homelab-host, e.g. 10.10.10.250:8443
  HOMELAB_ADMIN_HOST_TOKEN  the token this dashboard presents to the host
  HOMELAB_ADMIN_POLL_S      seconds between two fleet reads (default 10)";

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

    let target = match (
        std::env::var("HOMELAB_ADMIN_HOST"),
        std::env::var("HOMELAB_ADMIN_HOST_TOKEN"),
    ) {
        (Ok(addr), Ok(token)) if !addr.is_empty() && !token.is_empty() => {
            HostTarget { addr, token }
        }
        _ => {
            eprintln!(
                "homelab-admin: HOMELAB_ADMIN_HOST and HOMELAB_ADMIN_HOST_TOKEN must both be set \
                 (the host's address and the token this dashboard presents to it)"
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    let poll_s = std::env::var("HOMELAB_ADMIN_POLL_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10);

    let shared = Arc::new(RwLock::new(Snapshot::default()));
    let live = Live::new(256);
    app.webapp(WebApp::embedded(FILES));
    app.nav_entry("Fleet", "/app/");
    app.dashboard_routes(live.router("/events"));
    app.dashboard_routes(routes::router(shared.clone()));

    tokio::spawn(host_link::run(
        target,
        LinkConfig {
            poll: Duration::from_secs(poll_s),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
        },
        shared,
        live,
    ));
    app.run().await
}
