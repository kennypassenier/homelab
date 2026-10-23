//! eten-home-ip-1: the house's public address against the Cloudflare Access
//! bypass policies. The address service and Cloudflare are answered from
//! memory; the decisions themselves are unit-tested in
//! `core::homeaddress`.

mod act_support;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, shared, until};
use homelab_admin::core::actions::{ActionArgs, validate};
use homelab_admin::core::actions_config::from_env;
use homelab_admin::core::homeaddress::{ENV_ACCOUNT, ENV_TOKEN, Place, State};
use homelab_admin::shell::actions::{Actions, ActionsDeps, JobState, Origin};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::homeaddress::{AccessApi, AddressSource, HomeAddress, Report, Step};
use serde_json::{Value, json};

type BoxFut<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

struct Fixed(Result<IpAddr, String>);

impl AddressSource for Fixed {
    fn current(&self) -> BoxFut<'_, Result<IpAddr, String>> {
        let r = self.0.clone();
        Box::pin(async move { r })
    }
}

/// Cloudflare in memory: a put changes what the next read returns.
struct Cloudflare {
    policies: Mutex<Vec<Value>>,
    apps: Mutex<Vec<Value>>,
    puts: Mutex<Vec<(Place, String, Value)>>,
    refuse_put: bool,
}

impl Cloudflare {
    fn new(policies: Vec<Value>, apps: Vec<Value>) -> Arc<Self> {
        Arc::new(Cloudflare {
            policies: Mutex::new(policies),
            apps: Mutex::new(apps),
            puts: Mutex::new(Vec::new()),
            refuse_put: false,
        })
    }
}

impl AccessApi for Cloudflare {
    fn policies(&self) -> BoxFut<'_, Result<Vec<Value>, String>> {
        let p = self.policies.lock().unwrap().clone();
        Box::pin(async move { Ok(p) })
    }
    fn apps(&self) -> BoxFut<'_, Result<Vec<Value>, String>> {
        let a = self.apps.lock().unwrap().clone();
        Box::pin(async move { Ok(a) })
    }
    fn put(&self, place: &Place, id: &str, body: &Value) -> BoxFut<'_, Result<(), String>> {
        let (place, id, body) = (place.clone(), id.to_string(), body.clone());
        Box::pin(async move {
            if self.refuse_put {
                return Err("Cloudflare refused to update an Access policy (HTTP 403).".into());
            }
            self.puts
                .lock()
                .unwrap()
                .push((place.clone(), id.clone(), body.clone()));
            match &place {
                Place::Reusable => {
                    for p in self.policies.lock().unwrap().iter_mut() {
                        if p["id"] == id.as_str() {
                            p["include"] = body["include"].clone();
                        }
                    }
                }
                Place::App { app_id, .. } => {
                    for a in self.apps.lock().unwrap().iter_mut() {
                        if a["id"] == app_id.as_str() {
                            for p in a["policies"].as_array_mut().unwrap() {
                                if p["id"] == id.as_str() {
                                    p["include"] = body["include"].clone();
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn rule(r: &str) -> Value {
    json!({ "ip": { "ip": r } })
}

fn policy(id: &str, decision: &str, include: Vec<Value>) -> Value {
    json!({ "id": id, "name": format!("policy {id}"), "decision": decision,
            "include": include, "exclude": [], "require": [], "reusable": true,
            "created_at": "2026-10-22T00:00:00Z" })
}

fn service(now: &str, cf: Option<Arc<Cloudflare>>, configure: Option<String>) -> Arc<HomeAddress> {
    let clock = TestClock::at(1_790_000_000);
    HomeAddress::new(
        Arc::new(Fixed(Ok(ip(now)))),
        cf.map(|c| c as Arc<dyn AccessApi>),
        configure,
        "https://address.example".into(),
        clock.clock(),
    )
}

#[derive(Default)]
struct Quiet(Vec<(Step, bool)>);

impl Report for Quiet {
    fn step(&mut self, step: Step, finished: bool, _changed: bool) {
        self.0.push((step, finished));
    }
    fn line(&mut self, _text: &str) {}
}

/// Matching and mismatching addresses are told apart, and only a mismatch
/// lets the button work.
#[tokio::test]
async fn home_address_matching_and_mismatching_addresses() {
    let cf = Cloudflare::new(
        vec![policy("home", "bypass", vec![rule("203.0.113.7/32")])],
        vec![],
    );
    let same = service("203.0.113.7", Some(cf.clone()), None).read().await;
    assert_eq!(same.state, State::Match);
    assert!(!same.can_update);
    assert_eq!(same.current.as_deref(), Some("203.0.113.7"));
    let moved = service("198.51.100.9", Some(cf), None).read().await;
    assert_eq!(moved.state, State::Differs);
    assert!(moved.can_update);
    assert_eq!(moved.held, vec!["203.0.113.7".to_string()]);
}

/// Only bypass `ip` includes that hold the old address are rewritten:
/// an allow policy with the same address, a wider range, another address
/// that is already current, and every non-ip rule stay as they were.
#[tokio::test]
async fn home_address_only_bypass_ip_includes_holding_the_old_address_are_rewritten() {
    let cf = Cloudflare::new(
        vec![
            policy(
                "home",
                "bypass",
                vec![
                    rule("203.0.113.7/32"),
                    json!({ "email": { "email": "a@b.c" } }),
                ],
            ),
            policy("allow", "allow", vec![rule("203.0.113.7/32")]),
            policy("wide", "bypass", vec![rule("203.0.113.0/24")]),
        ],
        vec![json!({ "id": "eten", "name": "eten", "policies": [
            { "id": "own", "name": "eten home", "decision": "bypass",
              "include": [rule("203.0.113.7/32")], "exclude": [], "require": [] },
            { "id": "deny", "name": "everyone else", "decision": "deny",
              "include": [{ "everyone": {} }], "exclude": [], "require": [] },
        ]})],
    );
    let svc = service("198.51.100.9", Some(cf.clone()), None);
    let mut steps = Quiet::default();
    let said = svc.update(&mut steps).await.unwrap();
    assert!(said.contains("198.51.100.9"), "{said}");
    // The four steps each started and finished, in order.
    assert_eq!(steps.0.len(), 8);
    let puts = cf.puts.lock().unwrap();
    let mut written: Vec<&str> = puts.iter().map(|(_, id, _)| id.as_str()).collect();
    written.sort();
    assert_eq!(written, vec!["home", "own"]);
    for (_, id, body) in puts.iter() {
        assert_eq!(body["decision"], "bypass", "{id}");
        assert!(!body.to_string().contains("everyone"), "{id}");
    }
    let home = puts.iter().find(|(_, id, _)| id == "home").unwrap();
    assert_eq!(
        home.2["include"],
        json!([rule("198.51.100.9/32"), { "email": { "email": "a@b.c" } }])
    );
    // The allow policy, the wider range and the deny rule were not touched.
    let held = cf.policies.lock().unwrap();
    assert_eq!(held[1]["include"], json!([rule("203.0.113.7/32")]));
    assert_eq!(held[2]["include"], json!([rule("203.0.113.0/24")]));
    drop(held);
    assert_eq!(svc.last().unwrap().state, State::Match);
}

/// Nothing is written when the policies already hold the address.
#[tokio::test]
async fn home_address_nothing_is_written_when_they_match() {
    let cf = Cloudflare::new(
        vec![policy("home", "bypass", vec![rule("203.0.113.7/32")])],
        vec![],
    );
    let said = service("203.0.113.7", Some(cf.clone()), None)
        .update(&mut Quiet::default())
        .await
        .unwrap();
    assert!(said.contains("nothing was changed"), "{said}");
    assert!(cf.puts.lock().unwrap().is_empty());
}

/// A refused write says which policy and how many were written before.
#[tokio::test]
async fn home_address_a_refused_write_is_reported() {
    let mut cf = Cloudflare::new(
        vec![policy("home", "bypass", vec![rule("203.0.113.7/32")])],
        vec![],
    );
    Arc::get_mut(&mut cf).unwrap().refuse_put = true;
    let why = service("198.51.100.9", Some(cf), None)
        .update(&mut Quiet::default())
        .await
        .unwrap_err();
    assert!(
        why.contains("policy home") && why.contains("HTTP 403"),
        "{why}"
    );
    assert!(why.contains("0 of 1"), "{why}");
}

/// No token: the section and the button say what to configure, and nothing
/// is called.
#[tokio::test]
async fn home_address_a_missing_token_says_what_to_configure() {
    let cfg = from_env(&|k| match k {
        "HOMELAB_ADMIN_CLOUDFLARE_ACCOUNT_ID" => Some("acct".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(cfg.cloudflare_account_id.as_deref(), Some("acct"));
    assert!(cfg.cloudflare_token.is_none());
    assert_eq!(cfg.home_address_url, "https://api.ipify.org");
    let svc = HomeAddress::from_config(&cfg, TestClock::at(1_790_000_000).clock()).unwrap();
    let why = svc.update(&mut Quiet::default()).await.unwrap_err();
    assert!(
        why.contains(ENV_TOKEN) && !why.contains(ENV_ACCOUNT),
        "{why}"
    );
    assert!(why.contains("admin.env"), "{why}");
}

/// The job: a normal dashboard action with progress and a result, never in
/// the catalog, never sending anything to the host.
#[tokio::test]
async fn home_address_runs_as_a_dashboard_job() {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_c: &homelab_proto::Command| Script::ok(&[])),
        json!({ "entries": [] }),
        Arc::new(Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let db = homelab_admin::shell::store::memory_db();
    let notify = NotifyCenter::load(
        db,
        Arc::new(RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(MemFiles::default()),
        shared: shared(&[]),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let cf = Cloudflare::new(
        vec![policy("home", "bypass", vec![rule("203.0.113.7/32")])],
        vec![],
    );
    actions.set_home_address(service("198.51.100.9", Some(cf.clone()), None));
    assert!(
        !homelab_admin::core::actions::catalog()
            .iter()
            .any(|e| e.action.slug() == "update-home-address")
    );
    let req = validate("_host", "update-home-address", ActionArgs::default()).unwrap();
    let job = actions.press(req, Origin::Manual).await.unwrap().job;
    until("the home address job to finish", || {
        actions.job(job).is_some_and(|j| j.state.finished())
    })
    .await;
    let j = actions.job(job).unwrap();
    assert_eq!(j.state, JobState::Done, "{:?}", j.message);
    assert!(j.message.unwrap().contains("198.51.100.9"));
    assert!(host.ran().is_empty(), "the host was asked nothing");
    assert!(j.progress.unwrap().finished);
    assert_eq!(cf.puts.lock().unwrap().len(), 1);
}
