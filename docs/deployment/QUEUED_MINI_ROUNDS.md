# Queued mini-rounds — the deployment project

Rounds that are owed to Kenny but have not been put to him yet. The
procedure's rule (PROCEDURE.md, Phase 2): when a mandatory discussion item is
added to the procedure, every project not yet at Phase 10 inherits it in its
own queue **on the day it is added** — because "it applies from now on"
quietly means "nobody owns it".

Nothing here is built before he answers. Each one becomes a form.

---

## Q1 · Data & config location (Phase 2 mandatory item 4, standing rule 28)

**Added to the procedure 2026-08-31** (`~/Projects/dev-procedure`, commit
224d155), and Kenny named THIS project as the reason. Relayed by the
dev-procedure session the same evening.

**What the item asks.** Where does the software keep its state on disk — data,
configuration, secrets, logs, caches — and can Kenny *choose* that location
rather than inherit what the author hardcoded? It separates the kinds of state
first (durable data, config, secrets, and regenerable caches, which belong
OUTSIDE the rest so a backup carries no ballast), then puts the realistic
options side by side. The hard rule: an opinionated default is fine, an
immovable location is not. One documented knob — env var, CLI flag or config
key — moves the whole tree, and every path derives from that root.

**Why it is not theoretical here.** This project already has two roots and
they were chosen for good reasons, but neither is configurable today:

| Root | Holds | Configurable? |
|---|---|---|
| `/appdata/<stack>/<app>-config` | what the applications write; bind-mounted into containers; one restic repo per app | the per-stack paths are declared in each manifest, but the `/appdata` prefix is a hardcoded validation rule |
| `/var/lib/homelab/` | the orchestrator's own state: `state.json`, the secrets vault, TLS material, the intent git repo, incidents, journal | `HOMELAB_STATE_DIR` exists, so this one already has its knob |

So the honest position going in: one root has a knob, the other does not, and
the `/appdata` prefix is enforced by `manifest.rs` rather than configured.

**And the evening's own evidence, which the item predicts exactly.** Two
findings from 2026-08-31 are this item in miniature:

- **F125** — almanac writes to `/opt/almanac/data` by absolute path and latch
  keys its project link on an absolute path. Moving the working directory was
  not enough; the migration failed live and was reverted. That is precisely
  "an immovable location".
- **kyu**, by contrast, already has `KYU_DATA_DIR` and would have moved
  without complaint. Same house, same author, opposite answers — which is the
  argument for making it a standing question rather than a per-app accident.

**What the form will have to decide**, when Kenny gets to it:
1. Does the `/appdata` prefix become configurable, or stay an enforced
   convention with its reasons written down?
2. Do the native services get a required "one knob" contract — and if so, is
   that a request to those projects (almanac, http-switchboard) or a
   bind-mount at the path each app insists on?
3. Does the caches-outside-the-root rule change anything here — e.g. Jellyfin's
   transcode cache, which today lives in a docker volume and is deliberately
   NOT backed up.

**Status:** queued, not built, not started. Waiting on Kenny.

---

## Correctieformulier · ik heb O10 een tweede keer gebouwd (F233, F239)

Queued 2026-09-02 while Kenny was away, per the AFK rule: the fault is
recorded and the area quarantined, the form is presented on his return.
FORM_PROTOCOL §8 says every live-found fault ends in a correction form with
nine fields filled in. These are the nine; Kenny answers each with
Klopt · Aanpassen · Schrappen.

1. **Wat ging er mis.** I built `ops/streamguard.rs`, a `stream_guards`
   config surface, host.toml entries and a second copy of the Jellyfin token
   on disk — a complete second implementation of O10, which had been finished
   and armed since that morning (`core/src/ops/busy.rs`, wired at
   `core/src/ops/update.rs:210`, label `com.homelab.update.busy-check` on
   Jellyfin's compose, tests in `core/tests/busy_tests.rs`). It was released
   as v3.39.0 and rolled out to the host before it was caught.

2. **Welke poort liet het door.** None existed. The gate that should have
   caught it is the one before building anything: *measure the code, do not
   read a plan sentence about it.* `REALIZATION_PLAN.md` said M5's third item
   was "blocked on a working API key (F32)". That was true when written and
   F213 had made it false hours earlier — in this same session.

3. **Waar zit dezelfde fout nog.** Measured after the fact: the same "still
   says blocked" shape sat in the M5 paragraph only. But the CLASS is
   everywhere — this register has 239 findings and the plan has narrative
   paragraphs beside them, and a paragraph is not regenerated when the code
   changes. The generated `TEST_PLAN.md` is now the counterweight, and it is
   what caught this one.

4. **Hoe voorkomen we herhaling.** The measure is already built: Phase 7's
   output document is generated from the tests rather than written
   (`homelab testplan`, F239). Before building a feature the plan calls
   blocked, grep the test plan for it first. Proposed as a habit, not a hook —
   a hook that ran on every build would be noise.

5. **Wat kost het.** Almost nothing: one grep. The document regenerates in
   under a second and a test refuses a stale copy.

6. **Wie handhaaft het.** Code-enforced for the document
   (`the_committed_test_plan_matches_a_fresh_generation`); discipline-enforced
   for the habit of reading it. Standing rule 24 says to mark the difference,
   and this is the honest split: nothing can force somebody to look.

7. **Hoe en wanneer meten we dat het werkt.** At the next feature the plan
   describes as blocked or missing: the first action is a grep of
   `TEST_PLAN.md` for the feature's ID, and the result goes in the register
   before any code is written. Not a date — that moment.

8. **Fallback als het niet werkt.** If a second duplicate is ever built,
   the plan's narrative paragraphs stop being trusted at all: every
   "blocked"/"missing"/"not yet" claim in `REALIZATION_PLAN.md` gets a dated
   measurement line beside it or is deleted.

9. **Wanneer herzien we de maatregel.** At the Phase-10 retro of this
   project, together with the other generated-document decisions (runbook,
   test plan, homepage services list).

**Status:** queued, not presented. The revert is already done and pushed.

---

## F285 · a throwaway stack wrote into a live backup repository

**Status: PRESENTED AND RATIFIED 2026-09-04.** Kenny answered all nine fields
with *Klopt*. Recorded here only because field 7's measurement has not
happened yet, and standing rule 29 keeps the loop open until it has.

The fault, the gate, the measure and the cost are in `REGISTER.md` under F285.
What is outstanding is exactly one thing:

7. **How and when we measure it works.** At the NEXT drill on a throwaway
   container. That drill deliberately opens with an app name a live stack
   already owns, and the backup must refuse before a single restic command
   runs — read off the transcript, not off the error message alone. Only then
   is the name changed and the drill continued.

8. **Fallback if that measurement fails.** The likely cause would be that
   `state.json` did not know the other stack — a stack never deployed is not
   in it. The check then moves to the CLIENT, which sees every stack file on
   disk. That is a weaker place (the client can be bypassed by talking to the
   host directly), so it only moves there if the cheap place demonstrably
   falls short.

9. **When we review it.** The first time it refuses a backup that was wanted.
   A fail-closed rule that never gets in the way has no price; one that does
   has put itself up for discussion at that moment.

**CLOSED 2026-09-04.** The drill ran (F293). The drill stack declared the app
`jobtracker`, which kp-soft owns, and the backup refused at the `owner conflict`
step with **zero restic commands executed** — read off the transcript, as field 7
required. The measurement that kept this entry open has happened.

It also produced a new finding the ratified measure did not anticipate: the
guard is symmetric, so the drill blocked kp-soft's own backup until the drill
stack was destroyed (F292, task T82).

---

## QUEUED · Correction form — two sessions nearly put the same decision to Kenny twice

**Not opened as a form yet, deliberately.** The fault is "a second form on top
of an open form"; opening a correction form while B8's form is unanswered
would commit it again. This entry is the queue, per FORM_PROTOCOL §8 field 7:
the loop stays open until the form has actually been put and answered.

↳ *B8 = the register row for kp-soft v0.3.0 being blocked on the latch key.*

**What happened (2026-09-05).** This session measured the latch blockage, wrote
it up as B8 and rendered ONE form with two options for Kenny. It then reported
to the kp-soft session and, in that report, wrote out the decision including
its option list: *"Ik leg Kenny zo de keuze voor: hij draait de deploy zelf, of
hij zet de sleutel hier terug."* That session read it as a status message
carrying a choice, built the same choice into a form of its own, and rendered
it to Kenny. They withdrew it on request and queued their own process
correction (kp-soft queue item 30).

**Where the fault sits on THIS side.** The message handed over a fully shaped
decision — both options, the recommendation, the reasoning — which is exactly
the material a session builds a form from, with the ownership stated in a
subordinate clause. A peer reading it had everything needed to act and one
easily-missed word saying not to.

**The measure to propose — rewritten once, and the rewrite is the point.**

The first version read: *when telling a peer that a decision is going to Kenny,
say who owns the form in its own sentence, and send the measurement rather than
the option list.* That is written against the sentence this fault happened to
be in, which is exactly what FORM_PROTOCOL §8 field 3 warns about — a measure
shaped like the place you found it meets you again somewhere else.

The property underneath: **a worked-out option list travelling between sessions
reads as a specification, whatever the sentence around it says.** Ownership in
a subordinate clause is not what makes it dangerous; the shaped list is. So the
measure has to hold in the case that will actually bite next — the one where
nobody says anything about ownership at all. Both directions:

- *Sending.* A peer gets the measurement and the register number. If the option
  list goes too, the message opens by naming the session that renders.
- *Receiving.* Before rendering a form built on anything a peer sent, name the
  session that owns the decision. If a peer message carries options and does not
  say who renders, ask — a sibling session answers in minutes, which is
  FORM_PROTOCOL §5.6a's own reasoning applied to ownership instead of behaviour.

The kp-soft session found the property (their commit `4f08d45`) after this
entry was first written; the sharpening is theirs and the receiving half is
adopted here on its merits, not because a peer proposed it.

**What is NOT ours to decide.** Whether this becomes a rule in
`~/Projects/dev-procedure/STANDING_RULES.md` is Kenny's call. It looks like it
holds for every project — this is the second cross-session coordination fault
after F279 — but the shared procedure is not edited on a peer session's
suggestion, and the §8 landing rule puts a lesson there only when it holds
everywhere. That question goes in the correction form as its own item.

↳ *F279 = a cross-session message displaced work in progress and nothing
brought it back.*

