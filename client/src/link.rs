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
    /// fix-149: the machine had no pin and took the one this client was
    /// built with.
    BuiltIn(String),
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

/// Open the line: the machine's pin reconciled with the repository's and
/// the one this client was built with, the certificate held to it, and the
/// same message ceiling the host accepts.
///
/// `built_in`: the pin compiled into the client (`repo_config::built_in_pin`),
/// passed in so a test can present a certificate of its own.
pub async fn connect(
    host: &str,
    token: &str,
    repo_pin: Option<&str>,
    built_in: Option<&str>,
) -> Result<Link, String> {
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
    // fix-149 (first-connect-pin, Kenny 2026-09-27: "Pin in de client"): with
    // a pin built into the client, that is the only certificate trusted, a
    // first connection included — the handshake fails before the request
    // that carries the bearer token is ever sent.
    let decision =
        crate::repo_config::reconcile_pin_built_in(built_in, crate::load_pin(), repo_pin)?;
    // Saved only once the handshake has held the host to it (below): saving
    // it first left a pin behind for a host that was never reached.
    let adopted = decision
        .adopted_from_repo
        .then(|| decision.pin.clone())
        .flatten();
    let mut pinned = None;
    let first_connect = decision.pin.is_none();
    let verifier = crate::tls::PinnedVerifier::new(decision.pin);
    // The provider is named, not taken from the process default: in a build
    // that also compiles chassis-rs (the admin dashboard), rustls carries
    // both `ring` and `aws-lc-rs` and cannot pick a default, and the first
    // handshake panicked (measured 2026-09-28, remote_backend_tests).
    let tls_config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| format!("tls setup: {}", e))?
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
    if let Some(fp) = adopted {
        crate::save_pin(&fp);
        pinned = Some(if built_in.is_some() {
            Pinned::BuiltIn(fp)
        } else {
            Pinned::FromRepo(fp)
        });
    }
    if first_connect && let Some(fp) = &fingerprint {
        crate::save_pin(fp);
        pinned = Some(Pinned::FirstUse(fp.clone()));
    }
    Ok(Link {
        ws,
        fingerprint,
        pinned,
    })
}

/// fix-105 (older-client-no-warning, 2026-09-27): why `command` may not go
/// from this client to a host at `host_version`, if it may not.
///
/// The mirror image of [`refuse_older_host`]. The gate refused only a client
/// newer than the host; a stale client talking to a newer one parses the
/// stack files with an older reader, and since ask-8 a field it drops is a
/// field the host reads as "no longer declared, remove". Read-only commands
/// still go through, so the mismatch can be looked at.
pub fn refuse_older_client(command: &Command, host_version: &str) -> Option<String> {
    let client = env!("CARGO_PKG_VERSION");
    (crate::version::mutates(command) && crate::version::older(client, host_version)).then(|| {
        format!(
            "this client is v{} and the host is v{} :: an older client drops what it does \
             not know, and the host reads a dropped field as \"no longer declared, remove\" \
             — run 'homelab self-install' first",
            client, host_version
        )
    })
}

/// fix-105: the warning a read-only command prints when this client is
/// older than the host.
pub fn older_client_warning(host_version: &str) -> Option<String> {
    let client = env!("CARGO_PKG_VERSION");
    crate::version::older(client, host_version).then(|| {
        format!(
            "this client is v{} and the host is v{} — 'homelab self-install' updates it; \
             commands that change anything are refused until then",
            client, host_version
        )
    })
}

/// fix-141 (see REGISTER.md): a version with the build it was made from,
/// `v3.60.0 (v3.60.0-2-gabc1234-dirty)`. A host from before this change
/// reports no build.
pub fn version_label(version: &str, build: Option<&str>) -> String {
    format!(
        "v{} ({})",
        version,
        build
            .filter(|b| !b.is_empty())
            .unwrap_or("build not reported")
    )
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
