//! J1: the AFTER half. Same commands, same container, minutes later.

use crate::checks::{self, Reading, Verdict};
use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::runner::StepOutcome;

use super::run::{Run, log_info, log_warn};

/// The readings taken after the deploy, and which app and command each came
/// from (fix-27), so a reading of an app this deploy just restarted can be
/// taken again.
struct Taken {
    all: Vec<Reading>,
    origin: Vec<(String, String)>,
}

impl Run<'_, '_> {
    pub(super) async fn service_checks(&self) -> Result<StepOutcome, CoreError> {
        let (_, spec, _, _) = self.parts();
        if spec.checks.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut taken = self.take_readings().await;
        let (mut verdicts, blind) = checks::judge_all(&taken.all);
        self.settle_restarted(&mut taken, &mut verdicts).await;
        self.log_verdicts(&taken.all, &verdicts);
        // Reported even when everything passed: a green result is exactly
        // when nobody asks what was not checked.
        for b in &blind {
            log_info(self.ctx, format!("[check] does not prove: {}", b));
        }
        let bad = checks::regressions(&verdicts);
        if bad.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        self.ask_operator(&bad).await
    }

    async fn take_readings(&self) -> Taken {
        let (_, spec, m, exec) = self.parts();
        let mut taken = Taken {
            all: Vec::new(),
            origin: Vec::new(),
        };
        for (app, sc) in &spec.checks {
            let before = self.baseline.get(app);
            for c in sc.checks.iter().filter(|c| c.off.is_none()) {
                taken.origin.push((app.clone(), c.command.clone()));
                let after = pct_sh(exec, m.vmid, &c.command, 120)
                    .await
                    .map(|o| o.stdout.trim().to_string())
                    .unwrap_or_default();
                let b = before
                    .and_then(|rs| rs.iter().find(|(n, _)| n == &c.name))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                taken.all.push(Reading {
                    name: format!("{} · {}", app, c.name),
                    before: b,
                    after,
                    expect: c.expect,
                    blind_spot: c.blind_spot.clone(),
                });
            }
        }
        taken
    }

    /// fix-27: an app this deploy restarted or recreated may still be
    /// warming up. On 2026-09-26 Loki answered "Ingester not ready:
    /// waiting for 15s after being ready" to the first reading after its
    /// restart, and the deploy asked Kenny about a change that fixed
    /// itself fifteen seconds later. Such a reading is taken again, every
    /// 5 s for up to 60 s, before it counts as a change. An app the deploy
    /// did not touch is judged on its first reading, as before.
    async fn settle_restarted(&self, taken: &mut Taken, verdicts: &mut [Verdict]) {
        let touched: std::collections::BTreeSet<&String> = self
            .pushed
            .needs_restart
            .iter()
            .chain(self.pushed.recreated.iter())
            .collect();
        let rows = taken.all.iter_mut().zip(&taken.origin).zip(verdicts);
        for ((reading, (app, command)), verdict) in rows {
            if matches!(verdict, Verdict::Regressed(_)) && touched.contains(app) {
                self.retake(reading, command, verdict).await;
            }
        }
    }

    /// Takes one reading again, up to 12 times, until it no longer regresses.
    async fn retake(&self, reading: &mut Reading, command: &str, verdict: &mut Verdict) {
        let (ctx, _, m, exec) = self.parts();
        for _ in 0..12 {
            let again = pct_sh(exec, m.vmid, &format!("sleep 5; {}", command), 120)
                .await
                .map(|o| o.stdout.trim().to_string())
                .unwrap_or_default();
            reading.after = again;
            *verdict = checks::judge(reading);
            if !matches!(verdict, Verdict::Regressed(_)) {
                log_info(
                    ctx,
                    format!(
                        "[check] {} settled after its restart :: {}",
                        reading.name, reading.after
                    ),
                );
                break;
            }
        }
    }

    fn log_verdicts(&self, all: &[Reading], verdicts: &[Verdict]) {
        let ctx = self.ctx;
        for (r, v) in all.iter().zip(verdicts.iter()) {
            match v {
                Verdict::Ok => log_info(
                    ctx,
                    format!("[check] {} :: {} → {}", r.name, r.before, r.after),
                ),
                Verdict::Unreadable(why) => log_warn(ctx, format!("[check] {}", why)),
                Verdict::Regressed(_) => {}
            }
        }
    }

    /// T69 (Kenny, form H1). A reading that went down is not always
    /// damage. The case that raised this: routes 29 → 28 after a route
    /// was removed on purpose — where failing the deploy and bundling an
    /// incident is the wrong answer, and nobody learns anything from it.
    ///
    /// So the step stops and asks, instead of deciding for the operator.
    /// If nobody is watching — the nightly round at 04:00 — the answer is
    /// `Unattended` and the deploy fails exactly as it did before. Silence
    /// never reads as permission.
    async fn ask_operator(&self, bad: &[String]) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, _) = self.parts();
        let answer = ctx
            .asker
            .ask(&crate::ask::Question {
                op: format!("deploy-{}", m.stack_name),
                step: "service checks".into(),
                what: bad.join("; "),
                // fix-107: in the language of the client that shows it.
                if_allowed: "the deploy goes on and records the new value as the normal \
                             one"
                .into(),
                if_stopped: "the deploy fails here and bundles an incident".into(),
            })
            .await;
        match answer {
            crate::ask::Answer::Allow => {
                log_info(
                    ctx,
                    format!(
                        "[check] {} :: allowed by the operator — the new reading is now the \
                         baseline",
                        bad.join("; ")
                    ),
                );
                Ok(StepOutcome::Changed)
            }
            other => Err(Self::refused(&m.stack_name, bad, other)),
        }
    }

    /// The operator said stop, or nobody was there to say anything.
    fn refused(stack: &str, bad: &[String], answer: crate::ask::Answer) -> CoreError {
        let why = match answer {
            crate::ask::Answer::Stop => " :: stopped by the operator".to_string(),
            crate::ask::Answer::Unattended(why) => format!(" :: {}", why),
            crate::ask::Answer::Allow => String::new(),
        };
        CoreError::Command {
            rendered: format!("service checks {}", stack),
            detail: format!("{}{}", bad.join("; "), why),
        }
    }
}
