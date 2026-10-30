//! Homelab HOST daemon — thin shell around homelab-core (AR1).
//!
//! Provides: the real Executor (processes + files), config (TOML + env,
//! AR11), tracing (AR15), the journal file (B5), the WS server with required
//! bearer token, and the broadcast sink feeding connected clients (F2).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use tokio::process::Command;
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info};

use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor};
use homelab_core::ops::{OpCtx, deploy::deploy};
use homelab_core::runner::Journal;
use homelab_core::sink::{PipelineEvent, Sink};

use asker::*;
use auth::*;
use backup_batch::*;
use busy::*;
use cache_rounds::*;
use catchup::*;
use config::*;
use dashboard_watch::*;
use drills::*;
use edge::*;
use exec_guard::*;
use executor::*;
use fleet_state::*;
use gather_capacity::*;
use gather_disk::*;
use gather_snapshots::*;
use gather_today::*;
use http::*;
use incidents::*;
use journal::*;
use logging::*;
use native_state::*;
use nightly::*;
use nightly_tools::*;
use notices::*;
use notify_send::*;
use op_lock::*;
use op_report::*;
use probes::*;
use rpc::*;
use scheduler::*;
use sink::*;
use state::*;
use supervise::*;
use ws::*;

use homelab_core::ops::backup::NightBackup;

use homelab_proto::{Command as Rpc, RpcRequest, RpcResponse, ServerMsg};

mod asker;
mod auth;
mod backup_batch;
mod busy;
mod cache_rounds;
mod catchup;
mod config;
mod dashboard_watch;
mod drills;
mod edge;
mod exec_guard;
mod executor;
mod fleet_state;
mod gather_capacity;
mod gather_disk;
mod gather_snapshots;
mod gather_today;
mod http;
mod incidents;
mod journal;
mod logging;
mod native_state;
mod nightly;
mod nightly_tools;
mod notices;
mod notify_send;
mod op_lock;
mod op_report;
mod probes;
mod rpc;
mod scheduler;
mod secrets;
mod sink;
mod startup;
mod state;
mod supervise;
mod tls;
mod ui_relay;
mod ws;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// fix-141: the tree this binary was built from (see build.rs).
const BUILD: &str = env!("HOMELAB_BUILD");

#[cfg(test)]
mod tests;

#[tokio::main]
async fn main() {
    startup::check_arguments();

    // fix-122 (see REGISTER.md): config loads first, so the subscriber can
    // start with `state_dir` (the ring's home) and `log_level` (its seed
    // filter) already known. `load_config` never logs — a parse failure or a
    // refused value goes to stderr directly and exits — so nothing is lost by
    // moving it ahead of the subscriber.
    let config = load_config();
    let log_filter = init_production_logging(
        &config.state_dir,
        &config.log_level,
        config.log_ring_max_bytes,
    );

    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState::new(config.clone(), log_tx).with_log_filter(log_filter);

    startup::housekeeping(&config);
    let interrupted = startup::interrupted_operations(&config);
    startup::spawn_boot_notice(&state, interrupted);
    startup::spawn_background(&state);
    let scheduler = startup::spawn_scheduler(&state);
    let op_lock = state.op_lock.clone();
    // fix-52 residual: taken before `state` moves into `app_router`.
    let scheduler_heartbeat = state.scheduler_heartbeat.clone();
    let app = app_router(state);

    let tls_config = startup::tls_config(&config).await;
    startup::note_pending_update(&config);

    // fix-52: bind first, so READY=1 means the port is open. It was sent
    // before the socket existed, and a bind failure then looked like a daemon
    // that had started.
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .and_then(|l| l.into_std())
        .expect("bind listen address");
    let server = axum_server::from_tcp_rustls(listener, tls_config).expect("serve");

    // B7: tell systemd we're ready, then feed its watchdog.
    sd_notify("READY=1");
    startup::spawn_watchdog(scheduler_heartbeat);

    let code = supervise(
        // fix-120: with the peer address, so a refused connection says where
        // it came from.
        server.serve(app.into_make_service_with_connect_info::<SocketAddr>()),
        scheduler,
        terminate_signal(),
        op_lock,
        Duration::from_secs(60),
    )
    .await;
    std::process::exit(code);
}
