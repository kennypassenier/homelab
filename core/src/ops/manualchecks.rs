//! G17 · the checks only a person can answer.
//!
//! `checks.yml` gives each service a `manual:` list — whether a film looks
//! right on the television, whether the front page shows the services you
//! expect. Kenny asked for them explicitly ("ik ga het vergeten met mijn
//! adhd", form I2). Until now the deploy printed them at the end of its
//! transcript and stored nothing: 94 questions across 28 files, and no answer
//! anywhere to "has anybody ever looked". One of them is the check that would
//! have caught the empty homepage months before it was found by accident.
//!
//! The fix is deliberately not a file somebody keeps up to date. The deploy
//! already knows which questions it printed, so the deploy registers them; a
//! person answers one with a single command; and anything unanswered is a
//! finding in the nightly round that already reaches him. Nothing to
//! maintain, and no way for the list to drift away from the stack files.

use std::collections::BTreeMap;

use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::{HostState, ManualCheckRecord};

/// A check is the same check as long as its wording is. Change the question
/// and the old answer is about something else, so it becomes a new id and
/// falls back to unanswered — which is the honest outcome.
///
/// FNV-1a rather than a crypto hash: this identifies a line in a config file,
/// it does not authenticate anything, and eight hex characters is short
/// enough for Kenny to type without a copy-paste.
pub fn id_for(stack: &str, app: &str, text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in stack
        .as_bytes()
        .iter()
        .chain(b"/")
        .chain(app.as_bytes())
        .chain(b"/")
        .chain(text.as_bytes())
    {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", (h >> 32) as u32)
}

/// One question as a deploy registers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub app: String,
    pub text: String,
    pub once: bool,
    pub url: Option<String>,
    /// fix-182: the check's own `id:` from `checks.yml`, when it has one.
    /// `None` falls back to `id_for(stack, app, text)`, same as before this
    /// field existed.
    pub id: Option<String>,
    /// fix-182: previous hash-ids (`checks.yml`'s `replaces:`) whose answer
    /// should carry over to this question's id on registration.
    pub replaces: Vec<String>,
}

/// Fold the questions a deploy just printed into the state.
///
/// Idempotent, and it never touches an answer: re-deploying a stack ten times
/// does not make an answered check unanswered. Questions that disappeared
/// from the stack files are dropped, but only for the stack being deployed —
/// a deploy of `media` says nothing about `paperwork`'s checks.
pub fn register(state: &mut HostState, stack: &str, questions: &[Question], now: u64) {
    let mut seen: Vec<String> = Vec::new();
    for q in questions {
        // fix-182: an explicit `id:` IS the identity; only a check that
        // leaves it out falls back to the old hash. This is what makes a
        // wording fix (a typo, a translation) no longer reset the answer:
        // the text can change freely once an id names the check.
        let id =
            q.id.clone()
                .unwrap_or_else(|| id_for(stack, &q.app, &q.text));

        // fix-178/fix-182: a check given an id for the first time, with
        // `replaces:` naming the hash(es) its text used to live under,
        // carries that answer across instead of starting unanswered. Only
        // tried when nothing already sits under the new id — a second
        // deploy after the migration must not keep re-reading the (now
        // vacated) old record. The old record is dropped on the spot: it
        // would otherwise survive the `retain` below under its own stack and
        // app, duplicating the question that just moved under a new id.
        if !q.replaces.is_empty() && !state.manual_checks.contains_key(&id) {
            for old_id in &q.replaces {
                if let Some(old) = state.manual_checks.remove(old_id) {
                    state.manual_checks.insert(
                        id.clone(),
                        ManualCheckRecord {
                            stack: stack.to_string(),
                            app: q.app.clone(),
                            text: q.text.clone(),
                            registered_at: now,
                            answered_at: old.answered_at,
                            ok: old.ok,
                            note: old.note,
                            answered_hash: old.answered_hash,
                            answered_digests: old.answered_digests,
                            accepted_until: old.accepted_until,
                            once: q.once,
                            url: q.url.clone(),
                            recur_days: old.recur_days,
                        },
                    );
                    break;
                }
            }
        }

        seen.push(id.clone());
        let r = state
            .manual_checks
            .entry(id)
            .or_insert_with(|| ManualCheckRecord {
                stack: stack.to_string(),
                app: q.app.clone(),
                text: q.text.clone(),
                registered_at: now,
                answered_at: None,
                ok: None,
                note: String::new(),
                answered_hash: None,
                answered_digests: None,
                accepted_until: None,
                once: false,
                url: None,
                recur_days: None,
            });
        // Not the answer: whether it is asked once and where the app is can
        // change with the stack file, and follow it.
        r.once = q.once;
        r.url = q.url.clone();
    }
    // Gone from the stack file = gone as a question. Keeping it would grow a
    // list nobody can shrink, which is the hand-maintained file this exists
    // to avoid.
    state
        .manual_checks
        .retain(|id, r| r.stack != stack || seen.contains(id));
}

/// Record a person's answer. Returns false when the id is unknown, so the
/// caller can say so instead of silently doing nothing.
///
/// fix-65: the stack's `applied_hash` is kept with the answer, so only a
/// deploy that changed the stack's files asks the question again.
pub fn answer(state: &mut HostState, id: &str, ok: bool, note: &str, now: u64) -> bool {
    let Some((stack, app)) = state
        .manual_checks
        .get(id)
        .map(|r| (r.stack.clone(), r.app.clone()))
    else {
        return false;
    };
    let hash = state.stacks.get(&stack).map(|s| s.applied_hash.clone());
    // fix-195: record exactly the digests this answer is about — the app's
    // own files, the stack's rootfs, and the app's env — so a change
    // elsewhere in the stack (another app, the check's own wording, the
    // manifest's own serialization) can never reopen it. Only when the
    // stack already carries `component_digests` (fix-192); an older host, or
    // a stack never (re)applied since, has nothing to record and falls back
    // to `answered_hash` below.
    let digests = state
        .stacks
        .get(&stack)
        .filter(|s| s.component_digests.has_digests())
        .map(|s| crate::manifest::relevant_digests(&s.component_digests, &app));
    match state.manual_checks.get_mut(id) {
        Some(r) => {
            r.answered_at = Some(now);
            r.ok = Some(ok);
            r.note = note.to_string();
            r.answered_hash = hash;
            r.answered_digests = digests;
            r.accepted_until = None;
            true
        }
        None => false,
    }
}

/// password-chain-bus-factor: the stack name a STANDING check (one not
/// registered by any deploy) is filed under — never a real stack, so it can
/// never collide with one (app-knowledge's guard only names real stacks and
/// apps, and this is neither).
pub const STANDING_STACK: &str = "standing";

/// The one standing check this project has (owner decision "Alleen de
/// 90-dagen-controle", 2026-10-01): has anyone actually confirmed, inside
/// the last `PASSWORD_CHAIN_RECUR_DAYS`, that the restic password chain
/// still opens the backups — the password itself, its only independent copy
/// in Bitwarden, and (D5) the mirror remote credential. Nothing here can
/// measure that; it can only ask, and ask again.
pub const PASSWORD_CHAIN_APP: &str = "backups";
pub const PASSWORD_CHAIN_TEXT: &str = "has the restic password chain (the password itself, \
    its copy in Bitwarden, and the mirror remote credential) been opened and confirmed working \
    in the last 90 days";
pub const PASSWORD_CHAIN_RECUR_DAYS: u64 = 90;

/// Make sure a standing check's record exists, without touching an existing
/// answer — the same idempotence `register` gives a deploy's questions, so
/// calling this every nightly tick is free once the check has been created.
pub fn ensure_standing(state: &mut HostState, app: &str, text: &str, recur_days: u64, now: u64) {
    let id = id_for(STANDING_STACK, app, text);
    state
        .manual_checks
        .entry(id)
        .or_insert_with(|| ManualCheckRecord {
            stack: STANDING_STACK.to_string(),
            app: app.to_string(),
            text: text.to_string(),
            registered_at: now,
            answered_at: None,
            ok: None,
            note: String::new(),
            answered_hash: None,
            answered_digests: None,
            accepted_until: None,
            once: false,
            url: None,
            recur_days: Some(recur_days),
        });
}

/// Every standing check this project asks. Called once per nightly tick;
/// `ensure_standing` is a no-op once each already exists.
pub fn ensure_standing_checks(state: &mut HostState, now: u64) {
    ensure_standing(
        state,
        PASSWORD_CHAIN_APP,
        PASSWORD_CHAIN_TEXT,
        PASSWORD_CHAIN_RECUR_DAYS,
        now,
    );
}

/// fix-65: record a deliberate "not ok" that is accepted until `until`,
/// with the reason. An accepted answer is Noted until its date and Broken
/// again after, so the acceptance cannot quietly become permanent. Story:
/// `docs/deployment/REGISTER.md`.
pub fn accept(state: &mut HostState, id: &str, until: u64, reason: &str, now: u64) -> bool {
    if !answer(state, id, false, reason, now) {
        return false;
    }
    if let Some(r) = state.manual_checks.get_mut(id) {
        r.accepted_until = Some(until);
    }
    true
}

/// What the state says about the questions nobody can measure.
///
/// Three outcomes, and the difference between them matters:
/// - answered "not ok" and left that way → **Broken**, one finding each:
///   a person looked and said it was wrong, which is the strongest signal
///   this project gets, and it names the question and the note.
/// - never answered, or answered before the stack's last deploy → **Drift**,
///   aggregated per stack: the deploy may have changed exactly the thing the
///   question asks about.
/// - answered ok → silent until a deploy changes the stack's files; a
///   `once` question answered ok stays answered even then. There is no age
///   limit (checks-interval, Kenny, 2026-09-30: only a new version that
///   really changes the stack asks again; the 90-day window is gone).
///
/// The aggregation is not tidiness. There are 94 of these across the fleet,
/// and a nightly report with 94 lines in it is a report nobody reads — which
/// is the exact failure this gap exists to fix. One line per stack with a
/// count and two examples is something Kenny can act on; the full list is one
/// command away.
pub fn evaluate_manual(state: &HostState, now: u64) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    // stack -> (unanswered//stale-by-deploy, stale-by-age)
    let mut pending: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // fix-65: how many of a stack's open checks were answered before and
    // reopened because its files changed, so the finding can say why.
    let mut reopened: BTreeMap<String, usize> = BTreeMap::new();

    for r in state.manual_checks.values() {
        let stack = state.stacks.get(&r.stack);
        let deployed_at = stack.map(|s| s.applied_at).unwrap_or(0);
        // fix-65: an answer is about the files it was given against. Any
        // deploy used to reopen it, and `applied_at` moves on every deploy,
        // so answers given in the morning were open again by the evening.
        //
        // fix-195: when the answer carries `answered_digests` AND the stack
        // still has `component_digests` to compare against, that replaces
        // the whole-stack `answered_hash` rule — it only reopens when the
        // check's own app files, the rootfs, or the app's env actually
        // changed, not when something the question was never about did
        // (another app, the check's own wording, the manifest's own
        // serialization). An older answer, or one made before the stack
        // ever carried `component_digests`, keeps the old rule.
        let files_changed = |at: u64| {
            if let (Some(rd), Some(s)) = (&r.answered_digests, stack)
                && s.component_digests.has_digests()
            {
                return crate::manifest::relevant_digests(&s.component_digests, &r.app) != *rd;
            }
            match &r.answered_hash {
                Some(h) => stack.map(|s| &s.applied_hash != h).unwrap_or(false),
                None => at < deployed_at,
            }
        };
        match (r.ok, r.answered_at) {
            (Some(false), _) if r.accepted_until.is_some_and(|u| now < u) => out.push(Finding {
                severity: Severity::Noted,
                subject: format!("{}/{}", r.stack, r.app),
                what: format!(
                    "\"{}\" is not ok, accepted until {}: {}",
                    r.text,
                    crate::state::ymd(r.accepted_until.unwrap_or(0)),
                    r.note
                ),
                remedy: "nothing until that date; then it is a finding again unless it is \
                         answered anew"
                    .into(),
            }),
            (Some(false), _) => out.push(Finding {
                severity: Severity::Broken,
                subject: format!("{}/{}", r.stack, r.app),
                what: format!(
                    "somebody looked and said no: \"{}\"{}",
                    r.text,
                    if r.note.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", r.note)
                    }
                ),
                remedy: "fix it, then answer the check again — it stays a finding until \
                         somebody says it is right"
                    .into(),
            }),
            (_, None) => pending
                .entry(r.stack.clone())
                .or_default()
                .push(labelled(r)),
            (Some(true), Some(_)) if r.once => {}
            // password-chain-bus-factor: a standing check is due again
            // `recur_days` after it was last answered, whatever the answer
            // was — on a clock, since no deploy ever reopens it.
            (_, Some(at))
                if r.recur_days
                    .is_some_and(|d| now.saturating_sub(at) >= d * 86400) =>
            {
                pending
                    .entry(r.stack.clone())
                    .or_default()
                    .push(labelled(r))
            }
            (_, Some(at)) if files_changed(at) => {
                if r.answered_hash.is_some() {
                    *reopened.entry(r.stack.clone()).or_default() += 1;
                }
                pending
                    .entry(r.stack.clone())
                    .or_default()
                    .push(labelled(r))
            }
            _ => {}
        }
    }

    for (stack, mut texts) in pending {
        texts.sort();
        out.push(Finding {
            severity: Severity::Drift,
            subject: stack.clone(),
            what: format!(
                "{} check(s) only a person can answer are open, e.g. {}{}",
                texts.len(),
                examples(&texts),
                match reopened.get(&stack) {
                    Some(n) => format!(
                        " ({} of them answered before, reopened because the stack's files \
                         changed since)",
                        n
                    ),
                    None => String::new(),
                }
            ),
            remedy: "`homelab checks` lists them with their ids; \
                     `homelab checks answer <id> ok|nok` records one"
                .into(),
        });
    }
    out.sort_by(|a, b| a.subject.cmp(&b.subject).then(a.what.cmp(&b.what)));
    out
}

/// The question as a finding quotes it, with the application's address when
/// it has one (checks-link), so the notification opens the right app.
fn labelled(r: &ManualCheckRecord) -> String {
    match &r.url {
        Some(u) => format!("{} ({})", r.text, u),
        None => r.text.clone(),
    }
}

/// Two examples and a tail, so a one-line finding still says something
/// concrete about what is open.
fn examples(texts: &[String]) -> String {
    let head: Vec<String> = texts.iter().take(2).map(|t| format!("\"{}\"", t)).collect();
    if texts.len() > 2 {
        format!("{} and {} more", head.join(", "), texts.len() - 2)
    } else {
        head.join(", ")
    }
}

/// The list a person answers from, newest question first within a stack.
pub fn listing(state: &HostState) -> Vec<(String, ManualCheckRecord)> {
    let mut v: Vec<(String, ManualCheckRecord)> = state
        .manual_checks
        .iter()
        .map(|(k, r)| (k.clone(), r.clone()))
        .collect();
    v.sort_by(|a, b| {
        a.1.stack
            .cmp(&b.1.stack)
            .then(a.1.app.cmp(&b.1.app))
            .then(a.1.text.cmp(&b.1.text))
    });
    v
}

/// Render the listing for a terminal. Kept here rather than in the client so
/// the shape is testable without a terminal.
pub fn render_listing(rows: &[(String, ManualCheckRecord)], now: u64) -> String {
    if rows.is_empty() {
        return "no manual checks are registered — deploy a stack that has them".into();
    }
    let mut s = String::new();
    let mut stack = String::new();
    let (mut open, mut done) = (0usize, 0usize);
    for (id, r) in rows {
        if r.stack != stack {
            stack = r.stack.clone();
            s.push_str(&format!("\n{}\n", stack));
        }
        let status = match (r.ok, r.answered_at) {
            (Some(true), Some(at)) => format!("ok, {}d ago", now.saturating_sub(at) / 86400),
            // fix-65
            (Some(false), _) if r.accepted_until.is_some_and(|u| now < u) => format!(
                "accepted to {}",
                crate::state::ymd(r.accepted_until.unwrap_or(0))
            ),
            (Some(false), _) => "NOT OK".into(),
            _ => "unanswered".into(),
        };
        if matches!(r.ok, Some(true)) {
            done += 1;
        } else {
            open += 1;
        }
        s.push_str(&format!("  {}  [{:>11}]  {}\n", id, status, r.text));
        if let Some(u) = &r.url {
            s.push_str(&format!("                         {}\n", u));
        }
    }
    s.push_str(&format!(
        "\n{} answered ok, {} open. Answer one with:\n  homelab checks answer <id> ok|nok [note]\n  \
         homelab checks answer <id> accept <days> <reason>   (a deliberate nok, noted until then)\n",
        done, open
    ));
    s
}

/// The questions a deploy printed, flattened out of the checks map.
pub fn questions_of(checks: &BTreeMap<String, crate::checks::ServiceChecks>) -> Vec<Question> {
    let mut v: Vec<Question> = Vec::new();
    for (app, sc) in checks {
        for m in &sc.manual {
            v.push(Question {
                app: app.clone(),
                text: m.text().to_string(),
                once: m.once(),
                url: sc.url.clone(),
                id: m.id().map(str::to_string),
                replaces: m.replaces().to_vec(),
            });
        }
    }
    v
}
