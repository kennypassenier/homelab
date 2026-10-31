//! Watching the dashboard's own health endpoint, and deciding when an outage is expected or should be declared.

use crate::*;

/// A stack has just been parked by H8, which is the moment it stops being
/// protected: since fix-59 no automatic update (its nightly backup goes on;
/// onboot is left alone by the automatic park, gap-22; see REGISTER.md).
///
/// Decision notify-routing (2026-09-30): a notice in the dashboard's centre,
/// and pushed at once (push-edge, 2026-09-30): a parked stack loses its
/// nightly updates until someone enables it again.
/// The op name carries the stack: two stacks parking on the same night are
/// two notices, not one.
pub(crate) async fn notify_auto_disabled(
    state: &AppState,
    exec: &RealExecutor,
    stack: &str,
    why: &str,
) {
    let op = format!("stack-disabled-{}", stack);
    let ex = homelab_core::notify::explain_event(&op, stack, false, Some(why));
    publish_notice(
        state,
        exec,
        NoticeFacts {
            label: stack.to_string(),
            ok: false,
            deferred: false,
            since: unix_now(),
            urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Parked),
            incident: None,
            req: None,
            by: None,
            findings: Vec::new(),
            op,
            ex,
        },
    )
    .await;
}

/// replace-kuma: ask the dashboard's health address every minute; five
/// minutes without an answer is an urgent notice (pushed at once, since the
/// dashboard that would show it is the thing that is gone), and its return
/// another.
/// Decision "deploys are known outages" (Kenny, 2026-09-30): true while a
/// planned, single-stack operation (deploy, install-native, update-native,
/// adopt, patch, ...) is running on the one stack `watch_url` is declared
/// to answer for, or the host itself is updating or restarting — the
/// watched target is expected to be briefly unreachable, so the host's own
/// watch of it must not start (or advance) the down timer then.
///
/// fix-184 (2026-10-02): pulled out of [`dashboard_outage_expected`] so it
/// takes the holder and the declared stack as plain values — no app name is
/// hardcoded here; which stack counts is purely `watch_stack` (host.toml),
/// the same declaration `watch_url` itself lives beside.
pub(crate) fn outage_expected_for(
    holder: Option<&homelab_core::oplock::Holder>,
    watch_stack: Option<&str>,
) -> bool {
    holder.is_some_and(|h| {
        (watch_stack.is_some() && h.stack.as_deref() == watch_stack)
            || matches!(h.what.as_str(), "self-update" | "restart-host")
    })
}

pub(crate) fn dashboard_outage_expected(state: &AppState) -> bool {
    let Ok(b) = state.busy.lock() else {
        return false;
    };
    outage_expected_for(b.as_ref(), state.config.watch_stack.as_deref())
}

/// fix-184: the down notice fires once failure has lasted `down_after_s`
/// without a break — never on the first failed probe. Before this was
/// pulled out as its own named check, the same rule lived inline at the
/// `watch_dashboard` call site (now line 8233-8235 below); this makes it
/// one thing the test below can call directly instead of driving the whole
/// loop with a fake clock.
pub(crate) fn watch_should_declare_down(elapsed_failing_s: u64, down_after_s: u64) -> bool {
    elapsed_failing_s >= down_after_s
}

pub(crate) async fn watch_dashboard(state: AppState, url: String) {
    let mut t = tokio::time::interval(Duration::from_secs(state.config.watch_interval_s));
    let mut failing_since: Option<u64> = None;
    let mut told = false;
    // fix-184: whether the down notice that `told` remembers was actually
    // pushed out (urgent, and not swallowed by the 20 h damper) — the
    // recovery notice is urgent in turn only when this is true, never on
    // its own (see `watch_facts`).
    let mut down_pushed = false;
    loop {
        t.tick().await;
        if dashboard_outage_expected(&state) {
            // A known outage: skip this round entirely so the timer restarts
            // from zero once the deploy, update or restart has ended, same
            // as a tile the dashboard's own watch treats as "deploying".
            failing_since = None;
            told = false;
            down_pushed = false;
            continue;
        }
        let out = RealExecutor
            .run(&homelab_core::executor::Cmd::new(
                "curl",
                &["-sf", "-m", "10", "-o", "/dev/null", &url],
                20,
            ))
            .await;
        let ok = out.as_ref().map(|o| o.success()).unwrap_or(false);
        let now = unix_now();
        if ok {
            failing_since = None;
            if std::mem::take(&mut told) {
                publish_notice(
                    &state,
                    &RealExecutor,
                    watch_facts(true, now, &url, down_pushed),
                )
                .await;
            }
            down_pushed = false;
            continue;
        }
        let since = *failing_since.get_or_insert(now);
        if !told
            && watch_should_declare_down(now.saturating_sub(since), state.config.watch_down_after_s)
        {
            told = true;
            down_pushed = publish_notice(
                &state,
                &RealExecutor,
                watch_facts(false, since, &url, false),
            )
            .await;
        }
    }
}

/// `down_was_pushed` matters only for `ok: true` (a recovery): the dashboard
/// centre always gets the notice, but it is urgent — reaches
/// `notify_webhook` — only when the outage it closes was itself pushed out;
/// an outage that never left the centre (too short, or damped) does not get
/// a loop-closing push either (fix-184).
pub(crate) fn watch_facts(ok: bool, since: u64, url: &str, down_was_pushed: bool) -> NoticeFacts {
    NoticeFacts {
        op: "watch-dashboard".into(),
        label: "watch".into(),
        ok,
        deferred: false,
        since,
        ex: homelab_core::notify::Explained {
            title: if ok {
                "The dashboard answers again".into()
            } else {
                "The dashboard does not answer".into()
            },
            stack: None,
            what: format!(
                "{} {}",
                url,
                if ok {
                    "answers"
                } else {
                    "gave no answer for five minutes"
                }
            ),
            consequence: if ok {
                "Nothing to do.".into()
            } else {
                "Nothing watches the services every minute while it is gone.".into()
            },
            remedy: if ok {
                "Nothing to do.".into()
            } else {
                "Look at the dashboard's container and its unit from the host.".into()
            },
            page: homelab_core::notify::page::HOST.into(),
        },
        // fix-184: going down is always urgent ("a service does not
        // answer"); coming back is urgent only when the outage itself was
        // actually pushed — otherwise this recovery closes nothing anyone
        // outside the centre ever saw go down.
        urgency: if ok && !down_was_pushed {
            homelab_core::notify::Urgency {
                urgent: false,
                why: "not urgent: the outage was never pushed",
            }
        } else {
            homelab_core::notify::urgency(&homelab_core::notify::Event::Alert {
                alertname: homelab_core::notify::SERVICE_DOWN_ALERTS[0],
            })
        },
        incident: None,
        req: None,
        by: None,
        findings: Vec::new(),
    }
}
