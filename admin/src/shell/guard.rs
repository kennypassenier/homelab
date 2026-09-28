//! arch-exposure: the two locks, run by chassis before every route
//! (`request-guard`, chassis-rs 2.4.0), `/healthz` exempt.
//!
//! Lock 1 checks the Cloudflare Access token: its claims (pure,
//! `core::access`) and its RS256 signature against the team's published keys,
//! fetched with the HTTP client chassis already carries and kept for
//! `access_certs_refresh_s`; an unknown key id fetches again at once, at most
//! once a minute. Lock 2 compares `Cf-Connecting-IP`, believed only from the
//! trusted proxy (Traefik), with the house's address the host reported.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chassis::shell::request_guard::GuardRequest;
use tokio::sync::Mutex;

use crate::core::access::{self, Refusal, RsaKey};
use crate::core::config::AdminConfig;

use super::host_link::Shared;

struct Keys {
    keys: Vec<RsaKey>,
    fetched: Option<Instant>,
    last_try: Option<Instant>,
}

pub struct Guard {
    team: String,
    aud: String,
    leeway_s: u64,
    refresh: Duration,
    http: reqwest::Client,
    keys: Mutex<Keys>,
    shared: Shared,
}

fn refuse(r: Refusal) -> Response {
    tracing::info!(reason = ?r, "request refused before the login");
    (
        StatusCode::FORBIDDEN,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        access::refusal_page(&r),
    )
        .into_response()
}

/// The refusal page's module, answered before the locks: a refused visitor
/// never reaches `/app` (behind the login), and the script is the same
/// public kp-themes glue for everyone.
// The Err is chassis' request-guard contract: the response to send instead.
#[allow(clippy::result_large_err)]
pub fn refusal_script(r: &GuardRequest) -> Result<(), Response> {
    if r.path != access::REFUSAL_SCRIPT {
        return Ok(());
    }
    Err((
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        include_str!("../../web/refused/refused.js"),
    )
        .into_response())
}

impl Guard {
    pub fn new(c: &AdminConfig, shared: Shared) -> Arc<Self> {
        Arc::new(Guard {
            team: c.access_team_domain.clone(),
            aud: c.access_aud.clone(),
            leeway_s: c.access_leeway_s,
            refresh: Duration::from_secs(c.access_certs_refresh_s),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            keys: Mutex::new(Keys {
                keys: Vec::new(),
                fetched: None,
                last_try: None,
            }),
            shared,
        })
    }

    async fn fetch(&self) -> Result<Vec<RsaKey>, String> {
        let url = format!("https://{}/cdn-cgi/access/certs", self.team);
        let body = self
            .http
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("{url}: {e}"))?
            .text()
            .await
            .map_err(|e| format!("{url}: {e}"))?;
        access::parse_jwks(&body)
    }

    /// The key with this id, fetching the set when it is old or lacks it.
    async fn key(&self, kid: &str) -> Option<RsaKey> {
        let mut k = self.keys.lock().await;
        let stale = k.fetched.is_none_or(|t| t.elapsed() > self.refresh);
        let missing = !k.keys.iter().any(|x| x.kid == kid);
        let may_try = k
            .last_try
            .is_none_or(|t| t.elapsed() > Duration::from_secs(60));
        if (stale || missing) && may_try {
            k.last_try = Some(Instant::now());
            match self.fetch().await {
                Ok(keys) => {
                    k.keys = keys;
                    k.fetched = Some(Instant::now());
                }
                Err(e) => tracing::warn!("Access key set not refreshed :: {}", e),
            }
        }
        k.keys.iter().find(|x| x.kid == kid).cloned()
    }

    /// Lock 1.
    pub async fn access(&self, r: &GuardRequest) -> Result<(), Response> {
        let token = r
            .headers
            .get("cf-access-jwt-assertion")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| refuse(Refusal::NoToken))?;
        let jwt = access::parse_jwt(token).map_err(refuse)?;
        if jwt.alg != "RS256" {
            return Err(refuse(Refusal::Malformed(format!("algorithm {}", jwt.alg))));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        access::check_claims(&jwt.claims, &self.aud, &self.team, now, self.leeway_s)
            .map_err(refuse)?;
        let key = self
            .key(&jwt.kid)
            .await
            .ok_or_else(|| refuse(Refusal::UnknownKey))?;
        access::verify_rs256(&key, jwt.signed.as_bytes(), &jwt.signature).map_err(refuse)
    }

    /// Lock 2.
    pub async fn home(&self, r: &GuardRequest) -> Result<(), Response> {
        let client = r
            .headers
            .get("cf-connecting-ip")
            .filter(|_| r.from_trusted_proxy)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let home = self.shared.read().await.home_address.clone();
        access::from_home(client.as_deref(), home.as_deref()).map_err(refuse)
    }
}
