//! The routes that read or stop jobs and batches.

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::routes::refusal;
use super::{Actions, JOB_LOG_MAX, JobState};
use crate::core::actions::Refusal;

pub(super) async fn jobs(State(a): State<Actions>) -> Json<serde_json::Value> {
    // feat-batch-1: `batches` lets Activity reopen a batch.
    Json(serde_json::json!({ "jobs": a.jobs(), "batches": a.batches() }))
}

/// feat-batch-1: one batch, the shape the `action_batch` event carries.
pub(super) async fn get_batch(State(a): State<Actions>, UrlPath(batch): UrlPath<u64>) -> Response {
    match a.batch_view(batch) {
        Some(v) => Json(v).into_response(),
        None => refusal(
            StatusCode::NOT_FOUND,
            Refusal::new(
                "open the batch",
                format!("this dashboard knows no batch {batch}"),
                "the jobs page lists what ran",
            ),
        ),
    }
}

/// feat-logs-1: the whole kept log of a job.
pub(super) async fn job_log(State(a): State<Actions>, UrlPath(job): UrlPath<u64>) -> Response {
    match a.job_log_lines(job) {
        Some(lines) => Json(serde_json::json!({
            "job": job, "lines": lines, "max": JOB_LOG_MAX,
        }))
        .into_response(),
        None => refusal(
            StatusCode::NOT_FOUND,
            Refusal::new(
                "read the job's log",
                format!("this dashboard keeps no job {job}"),
                "the jobs page lists what is kept",
            ),
        ),
    }
}

/// feat-batch-1: cancel one queued job.
pub(super) async fn cancel_job(State(a): State<Actions>, UrlPath(job): UrlPath<u64>) -> Response {
    // A running command is stopped on the host; a queued job just leaves.
    let result = if a.job(job).is_some_and(|v| v.state == JobState::Running) {
        a.cancel_running(job, "a viewer").await
    } else {
        a.cancel_job(job, "a viewer")
    };
    match result {
        Ok(v) => Json(serde_json::json!({ "job": v.job, "state": v.state, "message": v.message }))
            .into_response(),
        Err(why) if a.job(job).is_none() => refusal(
            StatusCode::NOT_FOUND,
            Refusal::new("cancel the job", why, "the jobs page lists what is kept"),
        ),
        Err(why) => refusal(
            StatusCode::CONFLICT,
            Refusal::new(
                "cancel the job",
                why,
                "a queued job and a running command can be cancelled; other running jobs run to their end",
            ),
        ),
    }
}

/// A batch the dashboard does not know, as a 404.
fn unknown_batch(what: &str, batch: u64) -> Response {
    refusal(
        StatusCode::NOT_FOUND,
        Refusal::new(
            what,
            format!("this dashboard knows no batch {batch}"),
            "the jobs page lists what ran",
        ),
    )
}

/// feat-batch-1: cancel every still-queued job of a batch.
pub(super) async fn cancel_batch(
    State(a): State<Actions>,
    UrlPath(batch): UrlPath<u64>,
) -> Response {
    if a.batch_view(batch).is_none() {
        return unknown_batch("cancel the batch", batch);
    }
    let cancelled = a.cancel_batch(batch, "a viewer");
    Json(serde_json::json!({ "batch": batch, "cancelled": cancelled })).into_response()
}

/// The batch panel's Stop: the running job ends on its own, the queued
/// ones never start.
pub(super) async fn stop_batch(State(a): State<Actions>, UrlPath(batch): UrlPath<u64>) -> Response {
    if a.batch_view(batch).is_none() {
        return unknown_batch("stop the batch", batch);
    }
    let stopped = a.stop_batch(batch, "a viewer");
    Json(serde_json::json!({ "batch": batch, "stopped": stopped })).into_response()
}
