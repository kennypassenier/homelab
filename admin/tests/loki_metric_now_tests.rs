//! fix-221 (Kenny, 2026-10-02): the Traffic page's instant reads
//! (`Loki::metric_now`, used for "Busiest hostnames"/"Busiest client
//! addresses") must ask `query_range` — the only endpoint
//! `stacks/metrics/loki-push/nginx.conf` lets this dashboard through —
//! never `query`, which the gateway answers with a bare HTTP 403. These
//! tests pin the request path with a tiny raw-TCP server (the same
//! technique `admin/tests/tile_watch_watch_tests.rs` uses for a fake
//! backend) and pin that an HTML error body never reaches a caller as page
//! text.

use std::time::Duration;

use homelab_admin::shell::loki::Loki;

/// A one-shot HTTP/1.1 server: answers the first request with `body` and a
/// fixed 200 (or `status` when given), and hands the request's start line
/// back over `tx` so the test can assert which path was actually asked.
async fn fake_loki(
    status_line: &'static str,
    body: &'static str,
) -> (String, tokio::sync::oneshot::Receiver<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        if let Ok((sock, _)) = listener.accept().await {
            let mut reader = BufReader::new(sock);
            let mut line = String::new();
            let _ = reader.read_line(&mut line).await;
            // Drain the rest of the request headers so the client's write
            // does not block on a full send buffer.
            loop {
                let mut hdr = String::new();
                match reader.read_line(&mut hdr).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) if hdr == "\r\n" => break,
                    Ok(_) => continue,
                }
            }
            let resp = format!(
                "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut sock = reader.into_inner();
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            let _ = tx.send(line.trim_end().to_string());
        }
    });
    (format!("http://{addr}"), rx)
}

#[tokio::test]
async fn metric_now_asks_query_range_never_the_bare_query_endpoint() {
    let (base, rx) = fake_loki(
        "200 OK",
        r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"RequestHost":"a.example.com"},"values":[[1700000000,"5"],[1700000060,"7"]]}]}}"#,
    )
    .await;
    let loki = Loki::new(base, Duration::from_secs(5), 604_800, 5000).unwrap();
    let rows = loki
        .metric_now(
            "topk(5, sum by (RequestHost) (count_over_time({job=\"x\"}[60s])))",
            1_700_000_060,
            "RequestHost",
        )
        .await
        .expect("the mock answers 200");
    assert_eq!(rows, vec![("a.example.com".to_string(), 7.0)]);

    let line = rx.await.expect("the server saw one request");
    assert!(
        line.starts_with("GET /loki/api/v1/query_range?"),
        "metric_now must ask query_range (the gateway's only allowed read endpoint), got: {line}"
    );
    assert!(
        !line.contains("/loki/api/v1/query?") && !line.contains("/loki/api/v1/query&"),
        "metric_now must never ask the bare /query endpoint (fix-221, nginx 403s it), got: {line}"
    );
}

#[tokio::test]
async fn an_html_error_body_becomes_a_one_line_reason_never_raw_html() {
    let (base, _rx) = fake_loki(
        "403 Forbidden",
        "<html>\r\n<head><title>403 Forbidden</title></head>\r\n<body>\r\n<center>403 Forbidden</center>\r\n<hr><center>nginx</center>\r\n</body>\r\n</html>\r\n",
    )
    .await;
    let loki = Loki::new(base, Duration::from_secs(5), 604_800, 5000).unwrap();
    let err = loki
        .metric_now(
            "topk(5, sum by (RequestHost) (count_over_time({job=\"x\"}[60s])))",
            1_700_000_060,
            "RequestHost",
        )
        .await
        .expect_err("a 403 is an error");
    assert!(
        !err.to_ascii_lowercase().contains("<html"),
        "the error must never carry the raw HTML body, got: {err}"
    );
    assert!(
        err.contains("403"),
        "the error should still say the status: {err}"
    );
}
