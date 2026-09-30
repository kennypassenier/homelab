//! Milestone `read`: the host's questions (feat-ops-2), the host's
//! containers (feat-overview-2), the Loki query (feat-ops-4) and their
//! settings (arch-config).

use std::sync::{Arc, Mutex};

use axum::body::to_bytes;
use axum::http::StatusCode;
use chassis::shell::live::Live;
use homelab_admin::core::asks::{AnswerRequest, Asks, Refusal};
use homelab_admin::core::config::from_table;
use homelab_admin::core::guests::parse_status;
use homelab_admin::core::logs::{logql, parse_answer, window, LogQuery, MAX_LIMIT, MAX_SINCE_S};
use homelab_admin::shell::host_link::{Shared, Snapshot};
use homelab_admin::shell::routes::answer_ask;
use homelab_proto::{Command, RpcResponse};
use tokio::sync::RwLock;

fn hear(asks: &mut Asks, id: u64, boot: &str, step: &str, now: u64) {
    asks.heard(
        id,
        Some(boot.into()),
        "deploy media".into(),
        step.into(),
        "the container restarted twice".into(),
        "the deploy goes on".into(),
        "the deploy stops here".into(),
        now,
        120,
    );
}

fn req(id: u64, boot: &str, step: &str, allow: bool) -> AnswerRequest {
    AnswerRequest {
        id,
        boot: Some(boot.into()),
        op: "deploy media".into(),
        step: step.into(),
        allow,
    }
}

#[test]
fn feat_ops_2_an_open_question_becomes_the_hosts_answer_command() {
    let mut asks = Asks::default();
    hear(&mut asks, 3, "b1", "native units", 1000);
    let cmd = asks
        .check(&req(3, "b1", "native units", true), 1010)
        .unwrap();
    match cmd {
        Command::Answer { id, allow, boot } => {
            assert_eq!((id, allow, boot.as_deref()), (3, true, Some("b1")));
        }
        other => panic!("not an answer: {other:?}"),
    }
    assert_eq!(asks.open(1010).len(), 1);
    assert_eq!(asks.open(1010)[0].deadline, 1120);
}

#[test]
fn feat_ops_2_a_stale_answer_is_refused_for_each_reason() {
    let mut asks = Asks::default();
    hear(&mut asks, 3, "b1", "native units", 1000);
    // Never heard.
    assert_eq!(
        asks.check(&req(9, "b1", "native units", true), 1001).err(),
        Some(Refusal::NotOpen)
    );
    // Same id from another start of the host.
    assert_eq!(
        asks.check(&req(3, "b0", "native units", true), 1001).err(),
        Some(Refusal::OtherStart)
    );
    // Same id, another step: the page shows an older question.
    assert_eq!(
        asks.check(&req(3, "b1", "firewall", true), 1001).err(),
        Some(Refusal::OtherStep)
    );
    // Past the host's wait.
    assert_eq!(
        asks.check(&req(3, "b1", "native units", false), 1120).err(),
        Some(Refusal::TimedOut)
    );
    assert!(asks.open(1120).is_empty());
    assert!(asks.prune(1120));
    assert!(!asks.prune(1121));
}

#[test]
fn feat_ops_2_a_new_start_of_the_host_drops_the_old_questions() {
    let mut asks = Asks::default();
    hear(&mut asks, 1, "b1", "a", 1000);
    hear(&mut asks, 2, "b1", "b", 1000);
    hear(&mut asks, 1, "b2", "c", 1005);
    let open = asks.open(1006);
    assert_eq!(open.len(), 1);
    assert_eq!(
        (open[0].id, open[0].boot.as_deref(), open[0].step.as_str()),
        (1, Some("b2"), "c")
    );
    asks.forget(&Some("b2".into()), 1);
    assert!(asks.open(1006).is_empty());
}

fn shared_with(asks: Asks) -> Shared {
    Arc::new(RwLock::new(Snapshot {
        asks,
        ..Default::default()
    }))
}

async fn body(res: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn feat_ops_2_the_route_sends_a_checked_answer_and_closes_the_question() {
    let mut asks = Asks::default();
    let now = homelab_admin::shell::host_link::now_s();
    hear(&mut asks, 4, "b1", "native units", now);
    let shared = shared_with(asks);
    let live = Live::new(16);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let s2 = sent.clone();
    let res = answer_ask(
        &shared,
        &live,
        req(4, "b1", "native units", false),
        now,
        |cmd| async move {
            s2.lock().unwrap().push(cmd);
            Ok(RpcResponse {
                id: 77,
                ok: true,
                message: "answer delivered to question 4".into(),
                deferred: None,
            })
        },
    )
    .await;
    let (status, v) = body(res).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["ok"], true);
    let sent = sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert!(matches!(
        &sent[0],
        Command::Answer { id: 4, allow: false, boot: Some(b) } if b == "b1"
    ));
    assert!(shared.read().await.asks.open(now).is_empty());
}

#[tokio::test]
async fn feat_ops_2_the_route_refuses_a_stale_answer_without_sending() {
    let mut asks = Asks::default();
    hear(&mut asks, 4, "b1", "native units", 1000);
    let shared = shared_with(asks);
    let live = Live::new(16);
    let called = Arc::new(Mutex::new(false));
    let c2 = called.clone();
    let res = answer_ask(
        &shared,
        &live,
        req(4, "b0", "native units", true),
        1001,
        |_cmd| async move {
            *c2.lock().unwrap() = true;
            Err::<RpcResponse, String>("must not be sent".into())
        },
    )
    .await;
    let (status, v) = body(res).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["what"], "the answer");
    assert!(v["why"].as_str().unwrap().contains("earlier start"));
    assert!(!*called.lock().unwrap());
    // The question stays open: the refusal was about the page, not the host.
    assert_eq!(shared.read().await.asks.open(1001).len(), 1);
}

#[tokio::test]
async fn feat_ops_2_a_question_the_host_no_longer_waits_on_is_closed() {
    let mut asks = Asks::default();
    hear(&mut asks, 5, "b1", "s", 1000);
    let shared = shared_with(asks);
    let live = Live::new(16);
    let res = answer_ask(
        &shared,
        &live,
        req(5, "b1", "s", true),
        1001,
        |_cmd| async move {
            Ok(RpcResponse {
                id: 1,
                ok: false,
                message: "question 5 is no longer waiting — it timed out or was answered".into(),
                deferred: None,
            })
        },
    )
    .await;
    let (status, v) = body(res).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(v["why"].as_str().unwrap().contains("no longer waiting"));
    assert!(shared.read().await.asks.open(1001).is_empty());
}

#[test]
fn feat_overview_2_status_reads_as_the_guest_list_without_the_managed_state() {
    let msg = "pct list:\nVMID       Status     Lock         Name                \n\
               104        running                 104-app-gateway     \n\
               998        stopped    backup       debian-12-homelab   \n\
               106        running                 106-app-media       \n\n\
               managed state:\n{\"stacks\":{\"secret\":1}}";
    let g = parse_status(msg);
    assert_eq!(
        g.iter()
            .map(|g| (g.vmid, g.status.as_str(), g.lock.as_str(), g.name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (104, "running", "", "104-app-gateway"),
            (106, "running", "", "106-app-media"),
            (998, "stopped", "backup", "debian-12-homelab"),
        ]
    );
    assert!(parse_status("pct list:\nnothing here").is_empty());
}

fn q(stack: &str, app: Option<&str>, text: Option<&str>) -> LogQuery {
    LogQuery {
        stack: stack.into(),
        app: app.map(Into::into),
        q: text.map(Into::into),
        since: 3600,
        limit: 500,
    }
}

#[test]
fn feat_ops_4_the_logql_names_the_stack_the_app_and_the_text() {
    assert_eq!(logql(&q("media", None, None)).unwrap(), "{stack=\"media\"}");
    assert_eq!(
        logql(&q("media", Some("sonarr"), Some("say \"hi\" \\ now"))).unwrap(),
        "{stack=\"media\", container_name=\"sonarr\"} |= \"say \\\"hi\\\" \\\\ now\""
    );
    assert_eq!(
        logql(&q("media", Some("journal"), None)).unwrap(),
        "{stack=\"media\", job=\"systemd-journal\"}"
    );
    // A name that could leave the selector is refused, never escaped.
    assert!(logql(&q("media\"} or {x=\"", None, None)).is_err());
    assert!(logql(&q("media", Some("a b"), None)).is_err());
    assert!(logql(&q("", None, None)).is_err());
}

#[test]
fn feat_ops_4_the_window_and_line_count_are_clamped() {
    let mut l = q("media", None, None);
    l.since = 10 * 86400;
    l.limit = 99_999;
    assert_eq!(
        window(&l, 1_000_000, MAX_SINCE_S, MAX_LIMIT),
        (1_000_000 - 7 * 86400, 1_000_000, 5000)
    );
    l.since = 1;
    l.limit = 0;
    assert_eq!(
        window(&l, 1_000_000, MAX_SINCE_S, MAX_LIMIT),
        (1_000_000 - 60, 1_000_000, 1)
    );
}

#[test]
fn feat_ops_4_lokis_answer_reads_as_lines_newest_first() {
    // The shape Loki 3.7 answered on 2026-09-28 (CT 113), shortened.
    let body = r#"{"status":"success","data":{"resultType":"streams","result":[
      {"stream":{"container_name":"seerr","detected_level":"unknown","host":"106-app-media","job":"docker","stack":"media","stream":"stdout"},
       "values":[["1790606580006786129","2026-09-28T14:43:00.006Z [\u001b[34mdebug\u001b[39m][Jobs]: Starting \n"],
                 ["1790606520007079630","older line"]]},
      {"stream":{"job":"systemd-journal","stack":"media","unit":"cron.service","detected_level":"info"},
       "values":[["1790606585948680825","[Info] journal line"]]}]}}"#;
    let lines = parse_answer(body, 10).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].source, "cron.service");
    assert_eq!(lines[0].level, "info");
    assert_eq!(lines[0].ts_ms, 1790606585948);
    assert_eq!(lines[1].source, "seerr");
    assert_eq!(lines[1].level, "");
    assert_eq!(lines[1].stream, "stdout");
    assert_eq!(
        lines[1].line,
        "2026-09-28T14:43:00.006Z [debug][Jobs]: Starting"
    );
    assert_eq!(parse_answer(body, 2).unwrap().len(), 2);
    assert!(parse_answer(r#"{"status":"error","data":{}}"#, 5).is_err());
    assert!(parse_answer("not json", 5).is_err());
}

const BASE: &str = "[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\naccess_team_domain = \"example.cloudflareaccess.com\"\naccess_aud = \"e76eb5aa00000000000000000000000000000000000000000000000000000000\"\n";

#[test]
fn arch_config_loki_and_the_ask_wait_have_defaults_and_are_checked() {
    let c = from_table(BASE.parse().unwrap()).unwrap();
    assert_eq!(
        (c.loki_url.clone(), c.loki_timeout_s, c.ask_timeout_s),
        (None, 15, 120)
    );
    assert_eq!(c.loki_base(), None);
    let c = from_table(
        format!("{BASE}loki_url = \"http://10.10.10.13:3100/\"\n")
            .parse()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(c.loki_base().as_deref(), Some("http://10.10.10.13:3100"));
    let e = from_table(
        format!("{BASE}loki_url = \"10.10.10.13:3100\"\nask_timeout_s = 0\nloki_timeout_s = 0\n")
            .parse()
            .unwrap(),
    )
    .unwrap_err();
    assert!(e.contains("admin.loki_url"), "{e}");
    assert!(e.contains("admin.ask_timeout_s"), "{e}");
    assert!(e.contains("admin.loki_timeout_s"), "{e}");
}

#[test]
fn arch_config_loki_comes_from_the_environment_too() {
    let env = |k: &str| -> Option<String> {
        match k {
            "HOMELAB_ADMIN_HOST" => Some("10.10.10.250:8443".into()),
            "HOMELAB_ADMIN_HOST_TOKEN" => Some("0123456789abcdef0123".into()),
            "HOMELAB_ADMIN_ACCESS_TEAM_DOMAIN" => Some("example.cloudflareaccess.com".into()),
            "HOMELAB_ADMIN_ACCESS_AUD" => Some("e".repeat(64)),
            "HOMELAB_ADMIN_LOKI_URL" => Some("http://10.10.10.13:3100".into()),
            "HOMELAB_ADMIN_ASK_TIMEOUT_S" => Some("90".into()),
            _ => None,
        }
    };
    let c = homelab_admin::core::config::from_env(&env).unwrap();
    assert_eq!(c.loki_url.as_deref(), Some("http://10.10.10.13:3100"));
    assert_eq!(c.ask_timeout_s, 90);
}
