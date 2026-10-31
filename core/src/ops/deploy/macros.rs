//! Step wrappers that end the deploy with a `Failure`.

/// Shorthand: run a step of `Run`, leave the phase on failure (A3).
///
/// S2: before bailing, leave a record that this stack was half-deployed. The
/// deploy's own `record state` step is the last one, so until now a failure
/// anywhere before it wrote nothing at all — see `mark_incomplete`.
macro_rules! step {
    ($runner:expr, $run:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(outcome) => outcome,
            Err(e) => return Err($run.failed($name, e).await),
        }
    };
}

/// Like `step!`, but the step must prove its own work (S2).
macro_rules! stepv {
    ($runner:expr, $run:expr, $name:expr, $body:expr, $verify:expr) => {
        match $runner
            .step_verified($name, || async { $body }, || async { $verify })
            .await
        {
            Ok(outcome) => outcome,
            Err(e) => return Err($run.failed($name, e).await),
        }
    };
}
