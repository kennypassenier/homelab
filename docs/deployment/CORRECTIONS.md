# Corrections — the Homelab Deployment Project

Every live-found fault that reaches a correction form (FORM_PROTOCOL §8,
nine fields, Klopt · Aanpassen · Schrappen) lands here once Kenny has
ratified it. A form queued but not yet put to him stays in
`QUEUED_MINI_ROUNDS.md` until it is; this file is the record of the ones
he has actually answered.

---

## Correction · O10 built a second time (F233, F239)

**Ratified 2026-10-01 (Kenny: Klopt on all nine fields).** Queued
2026-09-02 while Kenny was away (AFK rule: record and quarantine, present
on return); the presentation itself waited until today.

1. **Wat ging er mis.** `ops/streamguard.rs`, a `stream_guards` config
   surface, host.toml entries and a second copy of the Jellyfin token on
   disk were built as a complete second implementation of O10, which had
   been finished and armed since that morning (`core/src/ops/busy.rs`,
   wired at `core/src/ops/update.rs:210`, label
   `com.homelab.update.busy-check` on Jellyfin's compose, tests in
   `core/tests/busy_tests.rs`). It was released as v3.39.0 and rolled out
   to the host before it was caught.
2. **Welke poort liet het door.** None existed. The gate that should have
   caught it is the one before building anything: measure the code, do
   not read a plan sentence about it. `REALIZATION_PLAN.md` said M5's
   third item was "blocked on a working API key (F32)" — true when
   written, false hours earlier in the same session (F213).
3. **Waar zit dezelfde fout nog.** The literal "still says blocked" shape
   sat only in the M5 paragraph, but the class is everywhere: this
   register has hundreds of findings and the plan carries narrative
   paragraphs beside them that are not regenerated when the code changes.
4. **Hoe voorkomen we herhaling.** Already built: Phase 7's output
   document is generated from the tests rather than written (`homelab
   testplan`, F239). Before building a feature the plan calls blocked,
   grep the test plan for it first — a habit, not a hook.
5. **Wat kost het.** Almost nothing: one grep; the document regenerates
   in under a second and a test refuses a stale copy.
6. **Wie handhaaft het.** Code-enforced for the document
   (`the_committed_test_plan_matches_a_fresh_generation`); the habit of
   reading it is discipline-enforced only.
7. **Hoe en wanneer meten we dat het werkt.** At the next feature the
   plan describes as blocked or missing: the first action is a grep of
   `TEST_PLAN.md` for the feature's ID, the result recorded in the
   register before any code is written. **Measured since**: no second
   instance has recurred; the generated test plan has been the standing
   check at every later phase-7/8/9 round (fix-161, fix-165 and the rest
   of today's batch were all built against generated plans, not narrative
   paragraphs).
8. **Fallback als het niet werkt.** If a second duplicate is ever built,
   every "blocked"/"missing"/"not yet" claim in `REALIZATION_PLAN.md`
   gets a dated measurement line beside it or is deleted — narrative
   paragraphs stop being trusted at all.
9. **Wanneer herzien we de maatregel.** At the Phase-10 retrospective of
   this project, together with the other generated-document decisions
   (runbook, test plan, homepage services list).

Closed in `QUEUED_MINI_ROUNDS.md` the same day.

---

## Correction · two sessions nearly asked the same decision twice (B8)

**Ratified 2026-10-01 (Kenny: Klopt).** Queued 2026-09-05, deliberately
not opened as a form until now — opening a correction form while B8's own
form was unanswered would have committed the same mistake a second time.

↳ *B8 = the register row for kp-soft v0.3.0 being blocked on the latch
key.*

1. **Wat ging er mis.** This session measured the latch blockage, wrote
   it up as B8 and rendered one form with two options for Kenny. Reporting
   to the kp-soft session, it wrote out the decision including its option
   list ("Ik leg Kenny zo de keuze voor: hij draait de deploy zelf, of hij
   zet de sleutel hier terug"). The kp-soft session read that as a status
   message carrying a choice, built the same choice into a form of its
   own, and rendered it to Kenny — withdrawn on request.
2. **Welke poort liet het door.** None: a worked-out option list travelling
   between sessions reads as a specification whatever the sentence around
   it says. Ownership stated in a subordinate clause is not read as a
   boundary by a peer session with everything it needs to act.
3. **Waar zit dezelfde fout nog.** The same shape as F279 (a cross-session
   message displaced work in progress and nothing brought it back) — the
   second instance of a cross-session coordination fault.
4. **Hoe voorkomen we herhaling.** Both directions, adopted together with
   kp-soft's own sharpening (their commit `4f08d45`): *sending* — a peer
   gets the measurement and the register number; if the option list goes
   too, the message opens by naming the session that renders. *Receiving*
   — before rendering a form built on anything a peer sent, name the
   session that owns the decision; if a peer message carries options and
   does not say who renders, ask.
5. **Wat kost het.** One extra sentence per cross-session status message;
   cheaper than a withdrawn form.
6. **Wie handhaaft het.** Discipline-enforced on both sides (sending and
   receiving session); no code gate exists for prose between sessions.
7. **Hoe en wanneer meten we dat het werkt.** At the next cross-session
   handoff that carries a shaped decision: does the message name the
   rendering session, and does the receiving session ask before
   rendering if it does not. No repeat has been reported since.
8. **Fallback als het niet werkt.** A third instance of the same fault
   moves the measure out of discipline and into process: cross-session
   messages that carry a decision get a required field naming the
   rendering session, checked the way `[ID]` is checked on a commit.
9. **Wanneer herzien we de maatregel.** Kenny's call on whether this
   becomes a `~/Projects/dev-procedure/STANDING_RULES.md` rule (it looks
   like it holds for every project, but the shared procedure is not
   edited on a peer session's suggestion) — raised to him as its own item,
   folded into this ratification: **Kenny 2026-10-01: Klopt**, stays a
   project-local measure here, not promoted.

Closed in `QUEUED_MINI_ROUNDS.md` the same day.

---

## Correction · fix-161 — CT 118 (inbox) never got automatic security updates

**Ratified 2026-10-01 (Kenny: Klopt; inbox-repair: Deployen na de
release).** Full finding and fix in `REGISTER.md` under `fix-161`;
summarised here for the corrections record.

1. **Wat ging er mis.** CT 118 (inbox) never completed an
   `unattended-upgrades` run (86 packages upgradable). Measured read-only:
   `unattended-upgrades` and `sqlite3` not installed, `20auto-upgrades`
   absent.
2. **Welke poort liet het door.** The guard step ran `apt-get install`
   through `pct_sh` and never looked at the exit code; inbox's first
   deploy (2026-09-28 06:09) had stale package lists, the installs
   failed, and the deploy reported "security patching in place" anyway.
3. **Waar zit dezelfde fout nog.** Checked every managed container
   (`command -v logrotate sqlite3 unattended-upgrade` across all 15):
   only CT 118 lacked anything.
4. **Hoe voorkomen we herhaling.** `guards::ensure_package` now runs
   `apt-get update -qq && apt-get install` only when the tool is missing
   and fails the step with the package name and apt's stderr on a failed
   install; logrotate, sqlite3 and unattended-upgrades all go through it.
   Built and tested (`core/tests/fix_161_tests.rs`).
5–6. **Kost / handhaving.** Code-enforced (the guard itself); no ongoing
   cost beyond the one-time apt run per container.
7. **Hoe en wanneer meten we dat het werkt.** After the next release:
   inbox is deployed through `homelab` (Live view), then `command -v
   unattended-upgrade sqlite3` in CT 118 finds both and the fleet check no
   longer lists 118 after the first nightly run. **Not yet measured** —
   residual, tracked as `fix-161` in `REGISTER.md`.
8. **Fallback.** None needed yet; the guard fails closed if the install
   does not succeed, so a repeat deploy would simply fail loudly instead
   of claiming success.
9. **Wanneer herzien we de maatregel.** At the measurement in field 7,
   once inbox has actually been redeployed.

