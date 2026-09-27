//! fix-67 · one way to open the CLIENT↔HOST line.
//!
//! The command line and the TUI each wrote their own connection setup, and
//! the TUI's drifted (tui-connection-skips-guards, 2026-09-27): it trusted
//! the first certificate even where the repository names the fingerprint,
//! kept tungstenite's 16 MiB frame default that fix-30 had raised, and sent
//! mutating commands to a host older than itself. Every guard now lives here,
//! and both callers go through it.

use std::sync::Arc;

use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use homelab_proto::Command;

pub type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A certificate this connection pinned, which the caller tells the operator
/// about in its own way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pinned {
    /// The machine had no pin and took the repository's.
    FromRepo(String),
    /// Nothing named a fingerprint, so the first one seen was trusted.
    FirstUse(String),
}

pub struct Link {
    pub ws: WsStream,
    /// The fingerprint the host presented.
    pub fingerprint: Option<String>,
    pub pinned: Option<Pinned>,
}

/// Open the line: the machine's pin reconciled with the repository's, the
/// certificate held to it, and the same message ceiling the host accepts.
pub async fn connect(host: &str, token: &str, repo_pin: Option<&str>) -> Result<Link, String> {
    let url = format!("wss://{}/api/ws", host);
    let mut request = url
        .clone()
        .into_client_request()
        .map_err(|e| format!("bad url {}: {}", url, e))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token)
            .parse()
            .map_err(|e| format!("the token cannot go in a header: {}", e))?,
    );

    // A4: pin the host certificate (TOFU on first connect) — unless the
    // repository names the fingerprint (feat-client-1), in which case a
    // fresh machine pins that instead of trusting whatever answers first.
    let decision = crate::repo_config::reconcile_pin(crate::load_pin(), repo_pin)?;
    let mut pinned = None;
    if decision.adopted_from_repo {
        if let Some(fp) = decision.pin.as_deref() {
            crate::save_pin(fp);
            pinned = Some(Pinned::FromRepo(fp.to_string()));
        }
    }
    let first_connect = decision.pin.is_none();
    let verifier = crate::tls::PinnedVerifier::new(decision.pin);
    let tls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let connector = Connector::Rustls(Arc::new(tls_config));

    let (ws, _) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        // fix-30: the same ceiling the host accepts. tungstenite's default
        // frame limit is 16 MiB, so any larger event from the host closed
        // the link mid-operation.
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(crate::version::MAX_WS_FRAME))
                .max_frame_size(Some(crate::version::MAX_WS_FRAME)),
        ),
        false,
        Some(connector),
    )
    .await
    .map_err(|e| format!("connect {}: {}", url, e))?;

    let fingerprint = verifier.observed();
    if first_connect {
        if let Some(fp) = &fingerprint {
            crate::save_pin(fp);
            pinned = Some(Pinned::FirstUse(fp.clone()));
        }
    }
    Ok(Link {
        ws,
        fingerprint,
        pinned,
    })
}

/// Why `command` may not go to a host at `host_version`, if it may not.
///
/// A client newer than the host loses whatever the host does not know about.
/// Serde drops an unknown field silently, so the deploy succeeds and simply
/// does less than it was asked to: on 2026-08-31 a host one release behind
/// ignored the `data_mounts` block, the downloader came up without its disks,
/// and 73 torrents went to `missingFiles`. Nothing said a word.
pub fn refuse_older_host(command: &Command, host_version: &str) -> Option<String> {
    let client = env!("CARGO_PKG_VERSION");
    (crate::version::mutates(command) && crate::version::older(host_version, client)).then(|| {
        format!(
            "host is v{} and this client is v{} :: a host that predates a \
             field ignores it silently, which is how a deploy quietly does \
             less than you asked — run 'homelab release-update' first",
            host_version, client
        )
    })
}
