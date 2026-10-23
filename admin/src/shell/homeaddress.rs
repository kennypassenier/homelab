//! eten-home-ip-1: the house's public address against the Cloudflare Access
//! bypass policies (decisions: `core::homeaddress`). This module is the I/O
//! half: it asks a "what is my address" service and Cloudflare's Access API
//! (both behind traits, so the tests answer them from memory), keeps the
//! last reading for the Inbox, and writes the new address into the policies
//! that hold the old one. The write is the dashboard action
//! `update-home-address` (`shell::actions`, child module `actions_home`),
//! never a call a page makes by itself.
//!
//! The Cloudflare token comes from the environment (`core::actions_config`)
//! and is only ever sent as the bearer of a request to the configured API
//! base; no error text carries it.

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use crate::core::actions_config::ActConfig;
use crate::core::homeaddress::{self, Change, Place, Status};

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where the house's public address is asked.
pub trait AddressSource: Send + Sync + 'static {
    fn current(&self) -> BoxFut<'_, Result<IpAddr, String>>;
}

/// Cloudflare's Access API, as far as this feature needs it.
pub trait AccessApi: Send + Sync + 'static {
    /// The account's reusable policies, as Cloudflare returns them.
    fn policies(&self) -> BoxFut<'_, Result<Vec<Value>, String>>;
    /// The account's Access applications (each with its `policies`).
    fn apps(&self) -> BoxFut<'_, Result<Vec<Value>, String>>;
    /// Replace one policy with `body`.
    fn put(&self, place: &Place, id: &str, body: &Value) -> BoxFut<'_, Result<(), String>>;
    /// The token's own state from Cloudflare's token check: (status,
    /// expires_on). None when it cannot be asked.
    fn token(&self) -> BoxFut<'_, Option<(String, String)>> {
        Box::pin(async { None })
    }
}

/// The "what is my address" service over HTTPS: the answer is the bare
/// address as text.
pub struct HttpAddress {
    url: String,
    http: reqwest::Client,
}

impl HttpAddress {
    pub fn new(url: String) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("the address client did not start: {e}"))?;
        Ok(HttpAddress { url, http })
    }
}

impl AddressSource for HttpAddress {
    fn current(&self) -> BoxFut<'_, Result<IpAddr, String>> {
        Box::pin(async move {
            let res =
                self.http.get(&self.url).send().await.map_err(|e| {
                    format!("The address service at {} did not answer: {e}", self.url)
                })?;
            if !res.status().is_success() {
                return Err(format!(
                    "The address service at {} answered HTTP {}",
                    self.url,
                    res.status().as_u16()
                ));
            }
            let text = res
                .text()
                .await
                .map_err(|e| format!("The address service's answer did not read: {e}"))?;
            homeaddress::parse_public_address(&text.chars().take(200).collect::<String>())
        })
    }
}

/// A fixed answer: the demo host's, so a demo run never calls out.
pub struct StaticAddress(pub IpAddr);

impl AddressSource for StaticAddress {
    fn current(&self) -> BoxFut<'_, Result<IpAddr, String>> {
        Box::pin(async move { Ok(self.0) })
    }
}

/// Cloudflare's API over HTTPS with a bearer token.
pub struct HttpAccess {
    base: String,
    account: String,
    token: String,
    http: reqwest::Client,
}

impl HttpAccess {
    pub fn new(base: String, account: String, token: String) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("the Cloudflare client did not start: {e}"))?;
        Ok(HttpAccess {
            base,
            account,
            token,
            http,
        })
    }

    fn url(&self, tail: &str) -> String {
        format!("{}/accounts/{}/access/{tail}", self.base, self.account)
    }

    /// Cloudflare's own message from an error body, with the cause and the
    /// fix (`core::homeaddress::cloudflare_failure`).
    fn refusal(what: &str, status: u16, body: &Value) -> String {
        let errors = body["errors"].as_array().cloned().unwrap_or_default();
        let said = errors
            .first()
            .and_then(|e| e["message"].as_str())
            .unwrap_or("");
        let codes: Vec<i64> = errors.iter().filter_map(|e| e["code"].as_i64()).collect();
        homeaddress::cloudflare_failure(what, status, &codes, said)
    }

    async fn get_all(&self, tail: &str, what: &str) -> Result<Vec<Value>, String> {
        let mut out = Vec::new();
        for page in 1..=20u32 {
            let res = self
                .http
                .get(self.url(tail))
                .bearer_auth(&self.token)
                .query(&[("per_page", "100".to_string()), ("page", page.to_string())])
                .send()
                .await
                .map_err(|e| homeaddress::cloudflare_unreachable(what, &e.to_string()))?;
            let status = res.status().as_u16();
            let body: Value = res.json().await.map_err(|_| {
                format!("Cloudflare's answer to {what} did not read (HTTP {status})")
            })?;
            if !(200..300).contains(&status) || body["success"] == Value::Bool(false) {
                return Err(Self::refusal(what, status, &body));
            }
            if let Some(rows) = body["result"].as_array() {
                out.extend(rows.iter().cloned());
            }
            let pages = body["result_info"]["total_pages"].as_u64().unwrap_or(1);
            if u64::from(page) >= pages {
                break;
            }
        }
        Ok(out)
    }
}

impl AccessApi for HttpAccess {
    fn token(&self) -> BoxFut<'_, Option<(String, String)>> {
        Box::pin(async move {
            let res = self
                .http
                .get(format!("{}/user/tokens/verify", self.base))
                .bearer_auth(&self.token)
                .send()
                .await
                .ok()?;
            let body: Value = res.json().await.ok()?;
            if body["success"] != Value::Bool(true) {
                return Some(("not accepted".into(), String::new()));
            }
            Some((
                body["result"]["status"].as_str().unwrap_or("").to_string(),
                body["result"]["expires_on"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            ))
        })
    }

    fn policies(&self) -> BoxFut<'_, Result<Vec<Value>, String>> {
        Box::pin(self.get_all("policies", "list the Access policies"))
    }

    fn apps(&self) -> BoxFut<'_, Result<Vec<Value>, String>> {
        Box::pin(self.get_all("apps", "list the Access applications"))
    }

    fn put(&self, place: &Place, id: &str, body: &Value) -> BoxFut<'_, Result<(), String>> {
        let tail = match place {
            Place::Reusable => format!("policies/{id}"),
            Place::App { app_id, .. } => format!("apps/{app_id}/policies/{id}"),
        };
        let body = body.clone();
        Box::pin(async move {
            let what = "update an Access policy";
            let res = self
                .http
                .put(self.url(&tail))
                .bearer_auth(&self.token)
                .json(&body)
                .send()
                .await
                .map_err(|e| homeaddress::cloudflare_unreachable(what, &e.to_string()))?;
            let status = res.status().as_u16();
            let answer: Value = res.json().await.unwrap_or(Value::Null);
            if !(200..300).contains(&status) || answer["success"] == Value::Bool(false) {
                return Err(Self::refusal(what, status, &answer));
            }
            Ok(())
        })
    }
}

/// A step of the update, for the job's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Address,
    Read,
    Write,
    Verify,
}

impl Step {
    pub const ALL: [Step; 4] = [Step::Address, Step::Read, Step::Write, Step::Verify];

    /// The step's name in the job panel.
    pub fn name(self) -> &'static str {
        match self {
            Step::Address => "Read the house's address",
            Step::Read => "Read the Cloudflare policies",
            Step::Write => "Write the new address",
            Step::Verify => "Check what Cloudflare holds now",
        }
    }
}

/// What the job reports while it runs.
pub trait Report: Send {
    fn step(&mut self, step: Step, finished: bool, changed: bool);
    fn line(&mut self, text: &str);
}

/// The feature's service: both readings, the last one, the write.
pub struct HomeAddress {
    source: Arc<dyn AddressSource>,
    access: Option<Arc<dyn AccessApi>>,
    configure: Option<String>,
    lookup_url: String,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    last: Mutex<Option<Status>>,
    /// The Cloudflare account, named in the token steps.
    account_id: Option<String>,
}

impl HomeAddress {
    pub fn new(
        source: Arc<dyn AddressSource>,
        access: Option<Arc<dyn AccessApi>>,
        configure: Option<String>,
        lookup_url: String,
        clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(HomeAddress {
            source,
            access,
            configure,
            lookup_url,
            clock,
            last: Mutex::new(None),
            account_id: None,
        })
    }

    /// The same service, naming `account` in the token steps.
    pub fn with_account(self: Arc<Self>, account: Option<String>) -> Arc<Self> {
        match Arc::try_unwrap(self) {
            Ok(mut me) => {
                me.account_id = account;
                Arc::new(me)
            }
            Err(shared) => shared,
        }
    }

    /// The service for the settings the dashboard started with: the real
    /// address service, and Cloudflare when both of its values are set.
    pub fn from_config(
        cfg: &ActConfig,
        clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Result<Arc<Self>, String> {
        let source = Arc::new(HttpAddress::new(cfg.home_address_url.clone())?);
        let configure = homeaddress::config_problem(
            cfg.cloudflare_account_id.as_deref(),
            cfg.cloudflare_token.as_deref(),
        );
        let access: Option<Arc<dyn AccessApi>> = match (
            configure.as_ref(),
            cfg.cloudflare_account_id.as_ref(),
            cfg.cloudflare_token.as_ref(),
        ) {
            (None, Some(a), Some(t)) => Some(Arc::new(HttpAccess::new(
                cfg.cloudflare_api.clone(),
                a.clone(),
                t.clone(),
            )?)),
            _ => None,
        };
        Ok(Self::new(
            source,
            access,
            configure,
            cfg.home_address_url.clone(),
            clock,
        )
        .with_account(cfg.cloudflare_account_id.clone()))
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// The last reading, if one was made.
    pub fn last(&self) -> Option<Status> {
        self.last.lock().ok().and_then(|l| l.clone())
    }

    fn keep(&self, s: &Status) {
        if let Ok(mut l) = self.last.lock() {
            *l = Some(s.clone());
        }
    }

    async fn policies(
        access: &Arc<dyn AccessApi>,
    ) -> Result<Vec<homeaddress::BypassPolicy>, String> {
        let reusable = access.policies().await?;
        let apps = access.apps().await?;
        Ok(homeaddress::bypass_policies(&reusable, &apps))
    }

    /// Read both sides now and keep the answer.
    pub async fn read(&self) -> Status {
        let current = self.source.current().await;
        let policies = match &self.access {
            Some(a) => Self::policies(a).await,
            None => Err(self.configure.clone().unwrap_or_default()),
        };
        let mut s = homeaddress::status(
            current,
            policies,
            self.configure.clone(),
            &self.lookup_url,
            self.now(),
        );
        // eten-home-ip-1: the token's own expiry, told before it bites.
        if let Some(a) = &self.access
            && let Some((state, expires)) = a.token().await
        {
            s.token_note = homeaddress::token_note(&state, &expires, self.now());
        }
        let token_trouble = homeaddress::is_token_failure(&s.message)
            || s.configure
                .as_deref()
                .is_some_and(homeaddress::is_token_failure)
            || s.token_note.is_some();
        if token_trouble {
            s.fix_steps = homeaddress::token_steps(self.account_id.as_deref());
        }
        self.keep(&s);
        s
    }

    /// Write the house's current address into every bypass policy that
    /// holds the old one. Ok: one sentence saying what is the case now.
    /// Err: one sentence saying what is wrong and what was or was not
    /// written. Never writes anything other than the one `ip` includes
    /// `core::homeaddress::plan` names.
    pub async fn update(&self, report: &mut dyn Report) -> Result<String, String> {
        if let Some(why) = &self.configure {
            return Err(why.clone());
        }
        let Some(access) = &self.access else {
            return Err(homeaddress::config_problem(None, None).unwrap_or_default());
        };
        report.step(Step::Address, false, false);
        let current = self.source.current().await?;
        report.line(&format!("The house's address is {current}"));
        report.step(Step::Address, true, false);

        report.step(Step::Read, false, false);
        let policies = Self::policies(access).await?;
        let held = homeaddress::held_addresses(&policies);
        report.line(&format!(
            "{} bypass {} with a single address, holding {}",
            policies.len(),
            if policies.len() == 1 {
                "policy"
            } else {
                "policies"
            },
            if held.is_empty() {
                "none".to_string()
            } else {
                held.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        if held.is_empty() {
            return Err(
                "Cloudflare holds no bypass policy with a single address, so there is nothing to replace. Make the policy in Cloudflare first."
                    .into(),
            );
        }
        let changes = homeaddress::plan(&policies, current)?;
        report.step(Step::Read, true, false);

        report.step(Step::Write, false, false);
        if changes.is_empty() {
            report.step(Step::Write, true, false);
            report.step(Step::Verify, false, false);
            report.step(Step::Verify, true, false);
            let s = self.read().await;
            let _ = s;
            return Ok(format!(
                "Cloudflare already lets in {current} without a login; nothing was changed."
            ));
        }
        let total = changes.len();
        for (i, c) in changes.iter().enumerate() {
            self.write_one(access, c, current, report)
                .await
                .map_err(|why| {
                    format!(
                        "{why} {} of {total} could be written before this; the Home address section shows what Cloudflare holds now.",
                        i
                    )
                })?;
        }
        report.step(Step::Write, true, true);

        report.step(Step::Verify, false, false);
        let after = self.read().await;
        report.step(Step::Verify, true, false);
        if after.state != homeaddress::State::Match {
            return Err(format!(
                "The write went through but Cloudflare does not hold {current} everywhere yet: {}",
                after.message
            ));
        }
        let names: Vec<String> = changes.iter().map(|c| c.name.clone()).collect();
        Ok(format!(
            "Cloudflare now lets in {current} without a login (was {}). Updated {total} {}: {}.",
            changes[0].from,
            if total == 1 { "policy" } else { "policies" },
            names.join(", ")
        ))
    }

    async fn write_one(
        &self,
        access: &Arc<dyn AccessApi>,
        c: &Change,
        current: IpAddr,
        report: &mut dyn Report,
    ) -> Result<(), String> {
        access
            .put(&c.place, &c.id, &c.body)
            .await
            .map_err(|e| format!("The policy {:?} was not updated: {e}", c.name))?;
        report.line(&format!(
            "Policy {:?} ({}): {} replaced by {}",
            c.name,
            match &c.place {
                Place::Reusable => "reusable".to_string(),
                Place::App { app_name, .. } => format!("application {app_name}"),
            },
            c.from,
            current
        ));
        Ok(())
    }
}

#[derive(Deserialize)]
struct ReadQuery {
    /// `last=1`: the kept reading (read once when there is none yet) and
    /// never a new round of calls; the Inbox asks this way.
    #[serde(default)]
    last: Option<String>,
}

/// `GET /data/home-address`: the Home address section's reading, and the
/// Inbox check's kept one.
async fn read(State(h): State<Arc<HomeAddress>>, Query(q): Query<ReadQuery>) -> Json<Status> {
    if q.last.as_deref().is_some_and(|v| v == "1")
        && let Some(s) = h.last()
    {
        return Json(s);
    }
    Json(h.read().await)
}

pub fn router(h: Arc<HomeAddress>) -> Router {
    Router::new()
        .route("/data/home-address", get(read))
        .with_state(h)
}

/// Read both sides every `every` and keep the answer, so the Inbox shows a
/// moved address without anybody opening the section.
pub fn spawn_watch(h: Arc<HomeAddress>, every: Duration) {
    tokio::spawn(async move {
        // Let the dashboard finish starting before the first calls go out.
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let s = h.read().await;
            tracing::debug!(state = ?s.state, "the home address was compared");
            tokio::time::sleep(every).await;
        }
    });
}
