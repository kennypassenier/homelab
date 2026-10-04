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


## 2026-10-02 · "The dashboard runs 3.70.0", said without measuring

**Status: open (owner not yet asked to ratify).**

1. **Wat ging er mis.** After the dashboard's install-native job, Claude
   told Kenny the dashboard ran 3.70.0 because the re-attached tab reported
   page `/`. Kenny's tab still ran the 3.69 page (the version banner with
   its update button was showing), and the next Live view steps, sent by a
   3.70 client with 3.70 routes, misbehaved on it. Kenny had to press Stop
   twice and explain the cause himself.
2. **Welke poort liet het door.** None existed: `homelab ui` never compares
   the driven tab's page version with the client's, and the claim rested on
   a page path that the 3.69 build (deployed from main on 2026-10-01)
   already had. The binary was in fact 3.70.0 (`homelab-admin --version` on
   CT 120); the loaded page was not.
3. **Waar zit dezelfde fout nog.** Any "it runs version X" read from a page
   path, a route or a title instead of `--version` or the served version.
4. **Hoe voorkomen we herhaling.** A version claim cites `--version` (or the
   served version) of the running binary AND of the driven tab; Live view
   refuses a step to a tab whose page version differs from the client's,
   with the reason (register row filed with the 3.70.1 fixes).
5–6. **Kost / handhaving.** Code-enforced for Live view once built; the
   citation rule is this entry until then.
7. **Hoe en wanneer meten we dat het werkt.** At the 3.70.1 rollout: drive
   one step against a tab left on the old page and see it refused with the
   reason.
8. **Fallback.** Kenny clicks the banner's update button; Claude asks for it
   instead of driving.
9. **Wanneer herzien we de maatregel.** At the measurement in field 7.

## 2026-10-02 · uptime destroyed after Kenny's "fix the reported problems first"

**Status: open (owner not yet asked to ratify).**

1. **Wat ging er mis.** Kenny wrote at 11:06:36 (local) "fix eerst de
   problemen die aangekaart zijn al". The destroy of uptime (CT 107) had
   been requested at 11:06:24, twelve seconds earlier, but its irreversible
   steps ran after his message: container stopped 11:08:49, destroyed
   11:08:52 (host journal). Claude did not read the message before those
   steps. Neither destroy (home at 11:05:40, uptime) had the restore proof
   the standing plan requires before an irreversible removal; the pre-destroy
   backup alone was taken as enough.
2. **Welke poort liet het door.** Nothing makes Claude read the thread
   between issuing a live step and its irreversible part, and the Destroy
   flow checks that a backup was written, not that it restores.
3. **Waar zit dezelfde fout nog.** Every long-running live step with an
   irreversible tail: destroy, CT rebuild, wipe of retired backups.
4. **Hoe voorkomen we herhaling.** (a) A Kenny message that arrives while a
   run is in progress is read before the next irreversible step, and a
   "stop / fix first" halts it (Stop in the job dialog). (b) A destroy runs
   only after the stack's latest backup was restored to a scratch location
   and its data checked; the proof is shown in the destroy's Go form.
5–6. **Kost / handhaving.** (a) is this entry; (b) is code-enforceable in
   the destroy plan as a restore-check step, filed with the 3.70.2 fixes.
7. **Hoe en wanneer meten we dat het werkt.** Measured after the fact for
   these two: restored on pve to a scratch directory at 11:15 and removed —
   uptime-kuma kuma.db integrity_check ok, 44 monitors, 1637192 heartbeats;
   homepage 10 files, services.yaml 170 lines. Next measurement: the first
   destroy after 3.70.2 shows the restore step in its plan.
8. **Fallback.** The backups above stay (Retired page) until Kenny wipes
   them.
9. **Wanneer herzien we de maatregel.** At the measurement in field 7.

## 2026-10-02 · Live view could not update the dashboard it exists to update

**Status: open (owner not yet asked to ratify).**

1. **Wat ging er mis.** fix-185 (this morning) made the dashboard refuse every Live view step from a client of another version, exempting only state, reload and done. At the 3.70.3 rollout the 3.70.1 tab refused the 3.70.3 client's steps, including the one that updates the dashboard; the update had to go through the CLI. Kenny: "een belachelijke fout die van mijlenver zichtbaar was".
2. **Welke poort liet het door.** fix-185's design asked "which steps are safe on a stale tab" but never "how does a stale tab become current through Live view", the one path every release takes; no test drove an older tab.
3. **Waar dezelfde fout nog zit.** Any version gate on a path that the version change itself must cross: the older-client refusal on the host (fix-105) has the same shape for `ui` steps (measured: the signed 3.70.1 client is refused by the 3.70.3 host).
4. **Hoe voorkomen we herhaling.** fix-199: one frozen `ui update-dashboard` step every version accepts; invariant row 14 with a screen test driving an older tab. A version gate is designed together with the path that crosses it.
5–6. **Kost / handhaving.** Code and screen test (invariants smoke in the full gate).
7. **Hoe en wanneer meten we dat het werkt.** At the 3.70.4 → next release rollout: the 3.70.4 tab updates itself through Live view without the CLI.
8. **Fallback.** `homelab install-native stacks/admin <tag>` from the CLI, announced in the thread.
9. **Wanneer herzien we de maatregel.** At the measurement in field 7.

---

## Correction · fix-238 — manual backups displaced the nightly of 2026-10-02

**Ratified 2026-10-03 (Kenny: Klopt).**

1. **Wat ging er mis.** The manual backups taken before the image updates and the Jellyfin 12.1 upgrade replaced the 2026-10-02 nightly snapshot on 7 stacks (jellyfin 78e2cae5, alertmanager, qbittorrent, actual, stirling, paperless, paperless-db, traefik, crowdsec), measured with `homelab snapshots`.
2. **Welke poort liet het door.** None: retention (one snapshot per day) was only ever tested with nightly snapshots, never with a manual one beside them.
3. **Waar dezelfde fout nog zit.** ZFS retention (an on-demand `zfs-replicate` on a day the nightly ran), fix-243. Searched with `grep -rn "forget_list(" core/src`: four restic call sites, three ZFS call sites.
4. **Hoe voorkomen we herhaling.** Two lanes: scheduled and on-demand snapshots are thinned separately (`retention::forget_list_by_lane`, through `ops::backup::retention_doomed`; ZFS `…-manual` names).
5. **Kost.** A few more on-demand snapshots kept (one per day, one per two weeks, …).
6. **Handhaving.** Code: invariant 38, `core/tests/retention_lanes_tests.rs`, `core/tests/fix_243_zfs_lanes_tests.rs`.
7. **Hoe en wanneer meten we dat het werkt.** After the first manual backup once 3.70.7 is live, that day's nightly is still listed by `homelab snapshots`. Until then Claude takes no manual backups (the release waits on the UI round, Kenny 2026-10-03).
8. **Fallback.** Claude stops taking manual backups until the cause is found and reports at once.
9. **Wanneer herzien we de maatregel.** At the next change to the retention settings.

---

## Correction · fix-livefix-1..4 — four 3.70.x fixes that did not hold live

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** The read-only sweep of host 3.70.7 (2026-10-03) found four rows that were "fixed" but not right on the real host: doctor and `homelab snapshots` disagreed about inbox's backup (fix-218); `status --json` had no root disk (fix-222); `homelab today` and `homelab checks` disagreed about four stacks' answers (fix-195/fix-65); `homelab check` reported Access drift from a stale capture.
2. **Welke poort liet het door.** Each fix was tested only against what its author imagined, never against the real shape: fix-218 fixed one of two readers of a stack's repositories and left the other with its own choice; fix-222's mock matched the substring `pv_name pve`, so a `pvs` call lvm2 refuses passed; fix-195 changed the report's rule and not the listing beside it; the edge capture was not renewed when the Access split changed the edge.
3. **Waar dezelfde fout nog zit.** Any second place that decides the same thing on its own: searched `natives.iter().map(|n| n.unit` (four places; two now share `stack_repo_owners`, the other two are the nightly and adopt, which decide it the same way) and every `MockExecutor` rule in the disk reading (now matched on the full command).
4. **Hoe voorkomen we herhaling.** One function per decision, called by every reader (`stack_repo_owners`, `manualchecks::open_again`); mocks for host commands match the whole command line and script the real refusal of a wrong one; an edge change made by hand is followed by a capture in the same sitting.
5. **Kost.** Small: four code changes, tests, one capture.
6. **Handhaving.** Tests `fix_218_repo_owners_tests.rs`, `fix_222_real_host_tests`, the fix-195 tests in `manualchecks_tests.rs` and `reports.test.js`, the renewed `edge_tests.rs` fixture.
7. **Hoe en wanneer meten we dat het werkt.** After the 3.71.0 rollout: `homelab snapshots stacks/inbox` lists the `inbox` repository with the snapshot doctor dates; `status --json` `disk_detail.root_disk_device` names a disk and `root_disk_total_gb` > 0; `homelab checks` shows the same open answers `homelab today` counts; `homelab check` keeps saying "edge: Cloudflare agrees".
8. **Fallback.** If one still disagrees live, the row stays "doing" and the reading is reported with the command output, not reasoned away.
9. **Wanneer herzien we de maatregel.** At the measurement in field 7.

---

## Correction · redesign-integrate-7 — a whole-screen run stalled 12 minutes on one case

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** On 2026-10-03 the 3.71.0 integration's whole-screen run (59 cases) stood still for 12 minutes on one case and named none; the coordinator stopped it at 18:20 (its browser alive 8 min 45 s with 4 s of CPU, the PC idle). The case was the Live view sweep "drive-reach: Live view finds and presses every declared control", which after the merges presses about twice as many controls and had no deadline per control.
2. **Welke poort liet het door.** None: the runner gave a case no deadline (`node --test` without `--test-timeout`), and only contexts made through `freshPage` had Playwright's per-step defaults; a case that opened its own context or waited inside `page.evaluate` could wait for ever.
3. **Waar dezelfde fout nog zit.** Every case that launched its own browser (all 153 in `invariants.e2e.js` and `secrets.e2e.js` called `chromium.launch()` directly) and every loop over a catalog (the sweep, the walk of every route).
4. **Hoe voorkomen we herhaling.** `scripts/invariants-run.sh` passes every `node --test` a per-test deadline (`INVARIANTS_TEST_TIMEOUT_MS`, 300 s by default); `admin/web/test-e2e/harness.js` is the one place a browser is launched and gives every context 20 s per step and per navigation; the sweep gives each control's try 45 s and names it, with a progress line per control when `INVARIANTS_PROGRESS` is set.
5. **Kost.** Small: one harness file, one flag, a mechanical rewrite of the launches.
6. **Handhaving.** Code: `admin/web/test/e2e_deadlines.test.js` ("redesign-integrate-7: every whole-screen case runs under a per-test deadline and per-step defaults", in `npm run check`, so at commit) refuses a run line without `--test-timeout`, a `chromium.launch(` or a Playwright import outside the harness, and a `timeout: 0`.
7. **Hoe en wanneer meten we dat het werkt.** At the next whole-screen run: no case runs past its deadline without failing by name; the 3.71.0 Go form's run reports every case's duration.
8. **Fallback.** If a case still hangs inside its deadline window often, the deadline drops to the longest measured case plus a margin and that case is split.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

---

## Correction · redesign-integrate-8 — five merges went green while 107 Live view controls could not be reached or did nothing

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** The 3.71.0 integration merged eight branches in five green merge commits (and three after); the first full Live view sweep then failed 107 of 276 declared controls. Most had no `reach` or `shows` (drawn in a view, a row, a panel or a state the sweep never drew); six were real Live view breakage: Update presses that open the Update flow were still declared as opening a dialog, a Map node (SVG) took the step and never answered, the hub's Compare now was never drawn, the server refused Deploy all changes' later-drawn controls, and text boxes declared as controls did nothing.
2. **Welke poort liet het door.** The commit-time catalog check (`.githooks/drivecatalog.sh` + `admin/web/test/drivecatalog.test.js`) ran on these merges and passed: it checks declarations and marks, never where a page draws a control. Reachability lived only in the whole-screen sweep, which no commit runs; the page branches were cut before the sweep existed. A clean `git merge` ran no hook at all (`pre-merge-commit` did not exist). The sweep itself then hid how much was wrong: its counts did not add up and a refused press in a panel failed the next 78 controls.
3. **Waar dezelfde fout nog zit.** Any control whose catalog entry is unchanged while its page changes how it draws it: the commit check keys on the entry, so such a change is caught only by the sweep at the release gate (the full fixed-order and shuffled runs before the Go).
4. **Hoe voorkomen we herhaling.** A passing full sweep stamps every control's catalog entry (`admin/web/test-e2e/sweep-stamp.json`, with the hash of its keys); the commit check refuses an entry no passing sweep pressed since it changed, and a stamp edited by hand. The sweep accounts for every control (passed, failed, conditional, not pressed; a run whose counts do not sum fails), resets the screen after every press and names a press that left something open, and passes in a shuffled order. `.githooks/pre-merge-commit` runs pre-commit.
5. **Kost.** A sweep run (about 3 to 6 minutes, in the e2e queue) before committing a change to a control's declaration; nothing for other commits.
6. **Handhaving.** Code: `redesign-integrate-8: every catalog control was pressed by a passing Live view sweep since its entry last changed` (drivecatalog.test.js, at every commit and merge touching admin/web/), `test/sweepkey.test.js`, `a_clean_merge_runs_the_same_commit_checks_as_a_commit` (register_hook_tests.rs), the sweep's own accounting and reset checks.
7. **Hoe en wanneer meten we dat het werkt.** At the next page change that adds or moves a control: the commit is refused until a sweep has pressed it; at the 3.71.0 gate: three shuffled full runs and the fixed-order stamp run pass.
8. **Fallback.** If a stamp is regenerated without a real sweep (its hash recomputed by hand), the release gate's own full sweep still fails the control.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

---

## Correction · redesign-final-gen-a — words broken per letter and text cut in drawers and dialogs

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** The final whole-dashboard review of redesign-371 (2026-10-04, C1 and C2) found Deploy all changes squeezed into a ~500 px drawer ("WILL DEPLO Y", tile numbers cut to "1…", "Compare again" over the step text) and the running pill's drawer cutting "step 3" and "no earlier run to go by".
2. **Welke poort liet het door.** Every page was reviewed at 1894 and 390 px, but a drawer's or dialog's content was laid out by the window's width (viewport media queries) while it sat in a 34 rem panel; no test measured text inside an open drawer, and a class name shared by two pages (`.ap-tile`, the Apps tile and the plan tile) went unnoticed.
3. **Waar dezelfde fout nog zit.** Any block that is drawn both on a page and in a panel (the job panel, the plan); the generic check lists seven more instances outside the review's C1–H5 (`AUDIT_KNOWN`, REGISTER redesign-final-10).
4. **Hoe voorkomen we herhaling.** Blocks that live in panels lay out by their own width (container queries: the plan, the job panel); one generic check (`redesign-final-gen-a`, `layoutAudit`) walks every page and every action dialog and drawer at both widths for words broken per letter, text cut by a clipping box and text spilling out of its block.
5. **Kost.** About 2 min per full sweep at the gate; a fix runs its own pages only (`INVARIANTS_AUDIT_ONLY`).
6. **Handhaving.** Code: INVARIANTS.md row 160; `redesign-final-gen-a` and `redesign-final-c1`, `redesign-final-c2` in `admin/web/test-e2e/invariants.e2e.js`.
7. **Hoe en wanneer meten we dat het werkt.** At the 3.71.0 gate's full whole-screen run: the check passes with only the listed known instances, each a correction item of its own.
8. **Fallback.** A false finding (a deliberate ellipsis) gets a `title` with the full text, never an exception in the check.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

## Correction · redesign-final-gen-b — a table row's chips printed over the next row

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** Firewall rule 18's stack chips wrapped into row 19 and printed over rule 19's address (final review C4).
2. **Welke poort liet het door.** The phone-width Firewall case (redesign-integrate-1) checked sideways scroll and labels, not row heights; at 1894 px nothing checked that a row holds its own content. The cause, `display: grid` on a `<td>`, does not grow its row.
3. **Waar dezelfde fout nog zit.** Any table cell styled as a grid or flex box directly; the generic check found no other instance.
4. **Hoe voorkomen we herhaling.** A cell keeps `display: table-cell`; its layout lives on a block inside it. The generic check `redesign-final-gen-b` compares every element's lowest content with its next sibling row's top on every page and dialog.
5. **Kost.** Part of the same sweep.
6. **Handhaving.** Code: INVARIANTS.md row 161; `redesign-final-gen-b`, `redesign-final-c4`.
7. **Hoe en wanneer meten we dat het werkt.** At the 3.71.0 gate's full run.
8. **Fallback.** A row that overlaps on purpose (none today) would sit in a positioned layer, which the check leaves out.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

## Correction · redesign-final-gen-c — numeric dates beside written-out ones

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** Dates read "Sat 3 Oct, 00:00" on most pages and "04/10/2026 00:00" on the Update flow, the Doctor, Notification rules, the Restore picker and the Map (final review X4).
2. **Welke poort liet het door.** `formatDateTime` (dd/mm/yyyy, Kenny's 2026-09-30 choice for the notification table) kept being reused by pages whose approved demos write the day out; no check compared formats across pages.
3. **Waar dezelfde fout nog zit.** Map and the Update flow (listed in `AUDIT_KNOWN` until Kenny decides X4); Notification rules and the Doctor view are gone with C3 and H5.
4. **Hoe voorkomen we herhaling.** The generic check `redesign-final-gen-c` refuses a numeric dd/mm/yyyy on any page or dialog; one format for the whole dashboard is Kenny's decision (X4).
5. **Kost.** Part of the same sweep.
6. **Handhaving.** Code: INVARIANTS.md row 162; `redesign-final-gen-c`.
7. **Hoe en wanneer meten we dat het werkt.** At the 3.71.0 gate's full run; once X4 is decided the two known entries go.
8. **Fallback.** A table column that needs a fixed width uses the written-out format with tabular figures.
9. **Wanneer herzien we de maatregel.** When Kenny decides X4.

## Correction · redesign-final-gen-d — a second page-sized title inside a page

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** Activity › Planned showed a second page-sized title "Schedules" under "Activity", and /host?section=doctor mounted the retired Doctor page (H1 "Doctor") inside Host (final review H5).
2. **Welke poort liet het door.** The Schedules page was hosted with `level: "h2"`, which changed the tag but kept the page-title size; the "one h1" checks looked at tags only.
3. **Waar dezelfde fout nog zit.** Any page module mounted as a view of another (`VIEW_MOUNTS`, main.js); the check found no other.
4. **Hoe voorkomen we herhaling.** A hosted page draws as a section (section-sized heading); a retired page's address opens the new page's own card instead of mounting the old page. The generic check `redesign-final-gen-d` refuses a second h1 or any heading of the h1's size.
5. **Kost.** Part of the same sweep.
6. **Handhaving.** Code: INVARIANTS.md row 163; `redesign-final-gen-d`, `redesign-final-h5`.
7. **Hoe en wanneer meten we dat het werkt.** At the 3.71.0 gate's full run.
8. **Fallback.** None needed: a page that must show two titles is two pages.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

## Correction · redesign-final-gen-e — a counter that contradicts its chips

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** Stacks read "Problems 0" while every card carried a red "does not build" or "will be destroyed" chip (final review M1).
2. **Welke poort liet het door.** The Problems filter counted down, degraded and missed-backup stacks only; the red drift flags were added later (redesign-stacks-7) without joining the count, and no test compared a counter with the chips on screen.
3. **Waar dezelfde fout nog zit.** Any counter computed apart from the marks it summarises; the check found Stacks only.
4. **Hoe voorkomen we herhaling.** A red chip on a stack is a problem (`stacksview.js`); the generic check `redesign-final-gen-e` refuses a problems counter reading 0 beside red chips on any page.
5. **Kost.** Part of the same sweep.
6. **Handhaving.** Code: INVARIANTS.md row 164; `redesign-final-gen-e` (whole-screen and unit).
7. **Hoe en wanneer meten we dat het werkt.** At the 3.71.0 gate's full run.
8. **Fallback.** A counter that deliberately counts less names what it counts in its label.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.

## Correction · redesign-final-11 — four whole-screen cases red on the integration branch

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** On redesign-371 (c6334fc9) four whole-screen cases failed although every page branch had passed its own: Deploy all changes' Compare and Esc case, the Doctor's loading case, Activity's view case and the Inbox's filter case. Two hid real faults: Esc closed the Deploy all changes sheet instead of resetting its filter, and Host's checks stopped for good on a first answer without checks.
2. **Welke poort liet het door.** Each branch ran only its own new cases; a merge ran the commit checks (catalog, gates) but no whole-screen case, and the kit's renames (`data-value` → `data-v`, `.sch-ph` gone) and the plan's move into a sheet broke cases of other pages.
3. **Waar dezelfde fout nog zit.** Any merge that changes a page another branch's cases open; until now caught only by the full run at the release gate.
4. **Hoe voorkomen we herhaling.** The gap is closed (redesign-final-12): a merge runs the whole-screen cases of the pages it changes (`.githooks/merge-cases.sh`, the mapping derived from the code by `admin/web/scripts/merge-cases.mjs`) and is refused on any failure, naming the cases.
5. **Kost.** About 20 s to 45 s of cases for a merge that touches one page (9 to 22 cases), more for a shared file (ui.js reaches 167 cases).
6. **Handhaving.** Code: `.githooks/pre-merge-commit` → `pre-commit` → `merge-cases.sh`; `admin/web/test/mergecases.test.js`; INVARIANTS.md row 167.
7. **Hoe en wanneer meten we dat het werkt.** Measured 2026-10-04 in a throwaway clone: a merge undoing the C4 fix was refused naming its case; a clean merge passed. Next: the next page branch merged into redesign-371.
8. **Fallback.** A merge that cannot wait for its cases is made with `--no-verify` as a conscious act; the release gate's full run still runs every case.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective, with the merges' measured hook durations.

## Correction · redesign-final-23 — a live update rebuilt a ticked row

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** On Stacks a live fleet push rebuilt a table row whose data had moved: the row lost its hover, and only the page's own tick set kept the tick; the case "Stacks keeps its ticks across Table/Cards and live updates" was red on the integration branch for it.
2. **Welke poort liet het door.** The page compared a row's whole content and replaced the node when anything in it changed; the one case that held it was page-specific, and the demo host pushed identical data every 5 s, so a rebuild only showed after a cursor move.
3. **Waar dezelfde fout nog zit.** Any page that draws selectable rows from live data. The generic check found ten such pages from the code (Stacks, Deploy all changes, the Update flow three ways, Host log, Console four ways); only Stacks had the fault.
4. **Hoe voorkomen we herhaling.** A row whose data moved keeps its node: its contents are renewed in place and the focus inside is given back (Stacks' `morph`). The generic check `redesign-final-extra-live-selection` visits every page the router knows plus their query links, ticks two rows, focuses one, waits two live pushes and refuses a lost tick, a lost focus or a rebuilt row; the demo host's memory now moves a little with every push, as a real container's does, so the check sees what a real host sends.
5. **Kost.** About 4 min of whole-screen time at the gate (most of it the crawl over ~60 addresses).
6. **Handhaving.** Code: `redesign-final-extra-live-selection` (invariants.e2e.js), `demo_usage` (admin/src/shell/demo.rs).
7. **Hoe en wanneer meten we dat het werkt.** Measured 2026-10-04: with Stacks' row fix reverted the check failed naming two rebuilt rows (242 s); with the fix it passed (242 s). Next: the 3.71.0 gate's full run.
8. **Fallback.** A page whose rows truly change identity (a different item) may rebuild that row; the check ticks rows whose identity stays.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective, with the gate's measured duration of the check.

## Correction · redesign-final-31 — a count that is not the rows it describes

**Draft — awaiting Kenny.**

1. **Wat ging er mis.** The Inbox's "Worth a look · 1" stood over two rows (final review M5), and Activity's History said "168 of 168 shown" while it drew 50 (it pages by 50).
2. **Welke poort liet het door.** Each page wrote its count from its own variable next to the rows it drew; nothing tied the number to the rows on screen, so a row added beside the counted ones (the doctor report) or a page limit slipped through.
3. **Waar dezelfde fout nog zit.** Any count drawn apart from its list; the check covers every count that names its list.
4. **Hoe voorkomen we herhaling.** A count names its list declaratively (`ui.js countOf`: `data-count-of`, `data-count-rows`), and the layout audit's class h holds every such count to the rows it describes, on every page and dialog at both widths.
5. **Kost.** Part of the layout audit's one sweep (about 3.5 min at the gate).
6. **Handhaving.** Code: `countOf` (ui.js), `layoutaudit.js` class h, `redesign-final-gen-h`.
7. **Hoe en wanneer meten we dat het werkt.** Measured 2026-10-04: with M5's fix reverted the check failed naming the Inbox's "· 1" over two rows; with it, 0. Next: the 3.71.0 gate's full run.
8. **Fallback.** A count that deliberately counts something else (a filter chip's matches) carries no `data-count-of` and says what it counts in its label.
9. **Wanneer herzien we de maatregel.** At the 3.71.0 retrospective.
