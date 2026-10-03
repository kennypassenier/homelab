//! redesign-371-metrics review (finding 11): the Traffic tab's reads of the
//! access log. A made-up Loki on loopback answers every read after a delay
//! and records what it was asked, so these prove the reads run together,
//! the top errors never ask Loki for one series per path, and the window
//! before a week or a month is read once, not every 30 s.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use homelab_admin::shell::loki::Loki;
use homelab_admin::shell::traffic;

#[derive(serde::Deserialize)]
struct Q {
    query: String,
}

type Asked = Arc<Mutex<Vec<String>>>;

const DELAY_MS: u64 = 300;

async fn answer(State(asked): State<Asked>, Query(q): Query<Q>) -> Json<serde_json::Value> {
    asked.lock().unwrap().push(q.query.clone());
    tokio::time::sleep(Duration::from_millis(DELAY_MS)).await;
    let row = |metric: serde_json::Value, n: f64| serde_json::json!({ "metric": metric, "values": [[1, n.to_string()]] });
    let result = if q.query.contains("by (path)") {
        vec![row(serde_json::json!({ "path": "/wp-login.php" }), 40.0)]
    } else if q.query.contains("by (DownstreamStatus, RequestHost)") {
        vec![
            row(
                serde_json::json!({ "DownstreamStatus": "404", "RequestHost": "a.example.org" }),
                90.0,
            ),
            row(
                serde_json::json!({ "DownstreamStatus": "403", "RequestHost": "b.example.org" }),
                50.0,
            ),
        ]
    } else {
        vec![row(serde_json::json!({}), 7.0)]
    };
    Json(
        serde_json::json!({ "status": "success", "data": { "resultType": "matrix", "result": result } }),
    )
}

async fn fake_loki() -> (Loki, Asked) {
    let asked: Asked = Arc::default();
    let app = Router::new()
        .route("/loki/api/v1/query_range", get(answer))
        .with_state(asked.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let loki = Loki::new(base, Duration::from_secs(10), 3_600, 100).unwrap();
    (loki, asked)
}

#[tokio::test]
async fn redesign_metrics_11_the_traffic_reads_run_together() {
    let (loki, _) = fake_loki().await;
    let t0 = Instant::now();
    let _ = traffic::read(&loki, "proxy", 86_400, 360, 1_790_000_000).await;
    let took = t0.elapsed();
    // Seven reads one after another take seven delays; together, the first
    // round and the top errors' paths take two.
    assert!(
        took < Duration::from_millis(DELAY_MS * 4),
        "the reads took {took:?}, as if one after another"
    );
}

#[tokio::test]
async fn redesign_metrics_11_the_top_errors_group_by_status_and_host_then_read_one_path() {
    let (loki, asked) = fake_loki().await;
    let body = traffic::read(&loki, "proxy", 86_400, 360, 1_790_000_000).await;
    let asked = asked.lock().unwrap().clone();
    assert!(
        !asked.iter().any(|q| q.contains("RequestPath)")),
        "no read groups by the raw path (Loki's series limit): {asked:#?}"
    );
    let paths: Vec<&String> = asked.iter().filter(|q| q.contains("by (path)")).collect();
    assert_eq!(paths.len(), 2, "one path read per winning pair: {asked:#?}");
    for q in &paths {
        assert!(q.contains("topk(1,"), "{q}");
        assert!(
            q.contains("regexReplaceAll"),
            "the query string is stripped: {q}"
        );
    }
    assert!(
        paths.iter().any(|q| q.contains("DownstreamStatus=\"404\"")
            && q.contains("RequestHost=\"a.example.org\""))
    );
    let rows = body["errors"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["status"], "404");
    assert_eq!(rows[0]["host"], "a.example.org");
    assert_eq!(rows[0]["path"], "/wp-login.php");
    assert_eq!(rows[0]["n"], 90.0);
}

#[tokio::test]
async fn redesign_metrics_11_the_window_before_a_week_or_a_month_is_read_once() {
    let (loki, asked) = fake_loki().await;
    let prev = |a: &Asked| {
        a.lock()
            .unwrap()
            .iter()
            .filter(|q| q.contains("offset"))
            .count()
    };
    for span in [7 * 86_400u64, 30 * 86_400] {
        let first = traffic::read(&loki, "proxy", span, span / 240, 1_790_000_000).await;
        let again = traffic::read(&loki, "proxy", span, span / 240, 1_790_000_030).await;
        assert_eq!(first["prev_total"], again["prev_total"]);
    }
    assert_eq!(prev(&asked), 2, "7d and 30d: one read each");
    let _ = traffic::read(&loki, "proxy", 86_400, 360, 1_790_000_000).await;
    let _ = traffic::read(&loki, "proxy", 86_400, 360, 1_790_000_030).await;
    assert_eq!(prev(&asked), 4, "a day is read every time");
}
