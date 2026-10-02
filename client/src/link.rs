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

/// fix-199 (replaces fix-105's refusal; Kenny, 2026-10-02: "het enige wat
/// een versie check moet doen is om ons te laten weten welke pagina's of
/// commandos we kunnen gebruiken voor die versie, dat moet niks
/// tegenhouden" — a version check informs, it never blocks): what to print
/// when this client is older than the host, for ANY command, mutating or
/// not. A stale client's struct parses the stack files with an older
/// reader, and used to drop a field the host then read as "no longer
/// declared, remove" (the 2026-08-31 incident) — `core::manifest`'s
/// field-keeping rule fixes the HOST side of that directly
/// (`DeploySpec.client_schema`, `client_knows`): a field this client's own
/// struct has no room for is now kept, never removed, so sending is safe
/// and this is purely informative.
pub fn older_client_notice(host_version: &str) -> Option<String> {
    let client = env!("CARGO_PKG_VERSION");
    crate::version::older(client, host_version).then(|| {
        format!(
            "this client is v{} and the host is v{} — 'homelab self-install' updates it; \
             a field this client's own build has no room for is kept by the host, not \
             removed, so nothing here is refused, but updating still gets you everything \
             this host can do",
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

/// fix-211 (Kenny, 2026-10-02, decision "Host weigert enkel onbekende
/// velden"): the first release whose HOST half knows how to refuse a
/// mutating command for a field its own build has no room for, naming it
/// (`homelab_core::wire::unknown_fields`, called from `host/src/main.rs`
/// before a command reaches its handler). A host at or above this version
/// is trusted to say so itself, so [`refuse_older_host`] stops refusing on
/// its behalf and lets the command go, whatever the client's own version
/// is — the host's own refusal, if any, comes back as an ordinary failed
/// reply and is relayed exactly like any other. A host older than this
/// still cannot tell "unknown" from "my struct never had a field for
/// this" apart, so it is still refused, now saying why rather than only
/// comparing version numbers.
///
/// This is a capability check, not a version gate against the client's own
/// build: a client far newer than this constant still sends everything to
/// a host at or above it.
pub const UNKNOWN_FIELD_CHECK_SINCE: &str = "3.70.4";

/// Why `command` may not go to a host at `host_version`, if it may not.
///
/// Before fix-211, a client newer than the host lost whatever the host did
/// not know about: serde dropped an unknown field silently, so the deploy
/// succeeded and simply did less than it was asked to — on 2026-08-31 a
/// host one release behind ignored the `data_mounts` block, the downloader
/// came up without its disks, and 73 torrents went to `missingFiles`.
/// Nothing said a word. [`UNKNOWN_FIELD_CHECK_SINCE`] is the host-side fix
/// for that: a host at or above it refuses the unknown field itself and
/// names it, so this function only still refuses for a host that predates
/// that check and genuinely cannot tell the difference.
pub fn refuse_older_host(command: &Command, host_version: &str) -> Option<String> {
    if !crate::version::mutates(command) {
        return None;
    }
    if !crate::version::older(host_version, UNKNOWN_FIELD_CHECK_SINCE) {
        // The host itself can tell an unknown field from one it simply has
        // no room for, and will refuse (naming the field) if it finds one.
        return None;
    }
    Some(format!(
        "host is v{} and cannot yet tell an unknown field from one its own \
         build simply has no room for — that check landed in v{}; a field \
         this old a host predates is dropped silently, which is how a \
         deploy quietly does less than you asked — run \
         'homelab release-update' first",
        host_version, UNKNOWN_FIELD_CHECK_SINCE
    ))
}
