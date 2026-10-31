//! What the deploy leaves for the operator: the questions no measurement
//! can settle, and the closing line.

use crate::ops::manualchecks::{self, Question};
use crate::runner::Runner;
use crate::sink::Level;
use crate::state::StateStore;

use super::run::Run;

impl Run<'_, '_> {
    /// The list no measurement can settle. manual-checks-as-info-1: it is
    /// information on the stack's page, no longer a notification.
    pub(super) async fn report(&self, runner: &Runner<'_>) {
        let (_, spec, m, _) = self.parts();
        let questions = manualchecks::questions_of(&spec.checks);
        self.register(&questions).await;
        Self::log_questions(runner, &m.stack_name, &questions);
        runner.log(
            Level::Info,
            format!(
                "[sync] Sync complete — {} {} · {} app(s) verified",
                if self.created {
                    "provisioned"
                } else {
                    "updated"
                },
                m.hostname,
                m.apps.len()
            ),
        );
    }

    /// G17: registering them is the half that was missing. Printing a
    /// question at the end of a transcript is not asking anybody anything.
    /// Registered even when the list is empty: that is how a stack that
    /// dropped its last question loses it (declarative, step-22).
    async fn register(&self, questions: &[Question]) {
        let (ctx, spec, m, _) = self.parts();
        let store = StateStore::new(ctx.exec, &ctx.state_dir);
        let Ok(mut st) = store.load().await else {
            return;
        };
        let before = st.manual_checks.len();
        manualchecks::register(&mut st, &m.stack_name, questions, ctx.now_unix);
        // checks-automate: the stack's probes, as this deploy declares them.
        let probes_before = st.probes.clone();
        crate::ops::probes::register(&mut st, &m.stack_name, m.vmid, &spec.checks);
        let busy_before = st.busy_checks.clone();
        crate::ops::busy::register(&mut st, &m.stack_name, &spec.checks);
        let settings_before = st.app_settings.clone();
        crate::ops::settings::register(&mut st, &m.stack_name, &spec.checks);
        if !questions.is_empty()
            || st.manual_checks.len() != before
            || st.probes != probes_before
            || st.busy_checks != busy_before
            || st.app_settings != settings_before
        {
            let _ = store.save(st).await;
        }
    }

    fn log_questions(runner: &Runner<'_>, stack: &str, questions: &[Question]) {
        if questions.is_empty() {
            return;
        }
        let lines: Vec<String> = questions
            .iter()
            .map(|q| {
                format!(
                    "{}  {}",
                    manualchecks::id_for(stack, &q.app, &q.text),
                    q.text
                )
            })
            .collect();
        runner.log(
            Level::Info,
            format!(
                "[check] {} thing(s) to look at if something goes wrong, listed on the \
                 stack's page (`homelab checks` lists them):\n  - {}",
                lines.len(),
                lines.join("\n  - ")
            ),
        );
    }
}
