//! fix-30: a huge command-output line never reaches the transcript whole.

use homelab_core::executor::{
    Cmd, CmdOutput, Executor, MockExecutor, TracingExecutor, TRACE_LINE_MAX,
};
use homelab_core::sink::VecSink;

/// `base64 -w0` of kyu is one 40 MB line. Echoed whole it became one 40 MB
/// websocket message and the client dropped the link while the host went on
/// to finish the install (2026-09-27).
///
/// covers: fix-30
#[tokio::test]
async fn fix_30_a_forty_megabyte_line_is_cut_to_its_start_and_its_size() {
    let exec = MockExecutor::new();
    let blob = "A".repeat(40 * 1024 * 1024);
    exec.respond_always("base64 -w0", CmdOutput::ok(&blob));
    let sink = VecSink::new();
    let tracer = TracingExecutor::new(&exec, &sink);
    let out = tracer
        .run(&Cmd::new(
            "base64",
            &["-w0", "/var/lib/homelab/staged/kyu/kyu.release"],
            60,
        ))
        .await
        .unwrap();
    assert_eq!(
        out.stdout.len(),
        blob.len(),
        "the caller still gets every byte"
    );
    let longest = sink.lines().iter().map(|l| l.len()).max().unwrap_or(0);
    assert!(
        longest < TRACE_LINE_MAX + 200,
        "the transcript carries no line near the blob's size: {longest}"
    );
    assert!(
        sink.lines()
            .iter()
            .any(|l| l.contains("41943040 bytes, truncated")),
        "and it says how much was left out"
    );
}

/// fix-39, second layer: whatever path prints a secret-named assignment
/// (`docker inspect … .Config.Env` did so on 2026-09-01 for paperless, mail
/// and postgres passwords), the transcript masks its value. `Cmd::quiet` is
/// the first layer; this catches the paths nobody marked.
///
/// covers: fix-39
#[test]
fn fix_39_secret_named_assignments_are_masked_in_output_lines() {
    use homelab_core::executor::trace_line;
    let got = trace_line(
        "|PAPERLESS_CONSUMER_POLLING=30 PAPERLESS_SECRET_KEY=abcd1234 POSTGRES_PASSWORD=hunter22",
    );
    assert!(got.contains("PAPERLESS_CONSUMER_POLLING=30"), "{got}");
    assert!(got.contains("PAPERLESS_SECRET_KEY=<redacted>"), "{got}");
    assert!(got.contains("POSTGRES_PASSWORD=<redacted>"), "{got}");
    assert!(
        !got.contains("abcd1234") && !got.contains("hunter22"),
        "{got}"
    );
    assert_eq!(trace_line("KYU_TOKEN=3be9f"), "KYU_TOKEN=<redacted>");
    assert_eq!(
        trace_line("KYU_LISTEN=0.0.0.0:8080"),
        "KYU_LISTEN=0.0.0.0:8080"
    );
    // A pattern that only names the key, as a grep does, has no value to hide.
    assert_eq!(
        trace_line("grep '^REGISTRY_TOKEN=' f"),
        "grep '^REGISTRY_TOKEN=' f"
    );
}
