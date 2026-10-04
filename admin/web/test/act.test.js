// Milestone act: the action forms as data, the running-job panel, batches,
// roll back, schedules and the notification centre, as pure view models.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionForm,
  batchActions,
  batchBody,
  batchConfirmErrors,
  batchForm,
  buildArgs,
  checkValues,
  fieldChoices,
  formFields,
  initialValues,
  nameTyped,
  previewArgs,
  schedulableActions,
  scheduleArgFields,
  stackActionGroups,
} from "../js/actionforms.js";
import {
  actionCommands,
  allCommands,
  registerCommands,
} from "../js/commands.js";
import {
  addLog,
  applyOutcome,
  applyProgress,
  batchView,
  jobFacts,
  jobPanel,
  jobRows,
  jobsAwaitingOutcome,
  logLine,
  outcome,
  outcomeSubject,
  percent,
  remaining,
  stepText,
  upsertJob,
} from "../js/jobs.js";
import {
  addNotice,
  bell,
  digestText,
  fixLabel,
  levelBadge,
  noticeRows,
  pushText,
  settingsBody,
  snoozeState,
  stackMuteRows,
  toastOf,
} from "../js/notices.js";
import { findingRows, todayView } from "../js/parity.js";
import { rollbackView } from "../js/rollback.js";
import {
  scheduleRows,
  toggledBody,
  whenFromValues,
  whenText,
  whenValues,
} from "../js/schedules.js";

/** @param {string} action @param {Partial<import("../js/actionforms.js").CatalogEntry>} [o] */
const entry = (action, o = {}) =>
  /** @type {import("../js/actionforms.js").CatalogEntry} */ ({
    action,
    target: "stack",
    label: action,
    what: `does ${action}`,
    scope: "operate",
    needs: "nothing",
    args: [],
    confirm: false,
    refused_for_self: false,
    destructive: false,
    ...o,
  });

/** The server's catalog, as far as the forms read it. */
const catalog = {
  host_target: "_host",
  self_stack: "admin",
  actions: [
    entry("deploy", {
      label: "Deploy",
      args: ["force"],
      batch_args: ["skip_backup"],
      needs: "spec",
    }),
    entry("deploy-commit", { args: ["commit", "force"] }),
    entry("backup", { label: "Back up" }),
    entry("restore", {
      label: "Restore",
      args: ["confirm", "app", "snapshot", "skip_safety_copy"],
      confirm: true,
    }),
    entry("update", { label: "Update", args: ["app"] }),
    entry("enable"),
    entry("rollback-native", { args: ["unit"] }),
    entry("destroy", {
      label: "Destroy",
      args: ["confirm", "skip_backup"],
      confirm: true,
      refused_for_self: true,
      scope: "all",
      destructive: true,
    }),
    entry("wipe", {
      args: ["confirm"],
      refused_for_self: true,
      scope: "all",
      destructive: true,
    }),
    entry("patch", { target: "host", label: "Patch the fleet" }),
  ],
};
const find = (/** @type {string} */ a) =>
  /** @type {import("../js/actionforms.js").CatalogEntry} */ (
    catalog.actions.find((x) => x.action === a)
  );
const ctx = { stack: "media", selfStack: "admin", hostTarget: "_host" };

test("a form is its steps: options first, the review with force and the typed name last", () => {
  const f = actionForm(find("restore"), ctx);
  assert.deepEqual(
    f.steps.map((s) => [s.id, s.fields.map((x) => x.name)]),
    [
      ["options", ["app", "snapshot", "skip_safety_copy"]],
      ["review", ["confirm"]],
    ],
  );
  assert.equal(f.confirmName, "media");
  assert.equal(f.runPath, "/data/actions/media/restore");
  assert.equal(f.previewPath, "/data/actions/media/restore/preview");
  // Every field has a stable id a replayed step can find.
  assert.deepEqual(
    formFields(f).map((x) => x.id),
    ["act-app", "act-snapshot", "act-skip-safety-copy", "act-confirm"],
  );
  const d = actionForm(find("deploy"), ctx);
  assert.deepEqual(
    d.steps.map((s) => s.id),
    ["review"],
  );
  assert.equal(d.steps[0].fields[0].when, "guard");
  const p = actionForm(find("patch"), ctx);
  assert.equal(p.stack, "_host");
  assert.equal(p.title, "Patch the fleet · the whole host");
  assert.equal(actionForm(find("destroy"), ctx).destructive, true);
});

test("arch-self: destroy and wipe are refused on the dashboard's own stack", () => {
  const f = actionForm(find("destroy"), { ...ctx, stack: "admin" });
  assert.match(String(f.refused), /own stack/);
  assert.equal(
    actionForm(find("deploy"), { ...ctx, stack: "admin" }).refused,
    null,
  );
  const groups = stackActionGroups(catalog, "admin");
  const retire = groups.find((g) => g.group === "Retire");
  assert.deepEqual(
    retire?.actions.map((a) => [a.entry.action, a.refused != null]),
    [
      ["destroy", true],
      ["wipe", true],
    ],
  );
  // Host-wide actions never show among a stack's buttons.
  assert.ok(
    !groups.flatMap((g) => g.actions).some((a) => a.entry.target === "host"),
  );
});

// fix-229 (the 3.70.6 visual pass): kp-soft's page offered Adopt, Install a
// release and Roll back binary, whose dialogs then had no unit to choose.
test("fix_229_a_docker_stack_never_offers_the_native_actions", () => {
  const offered = (/** @type {boolean | null} */ native) =>
    stackActionGroups(catalog, "kp-soft", native)
      .flatMap((g) => g.actions)
      .map((a) => a.entry.action);
  assert.ok(!offered(false).includes("rollback-native"), "docker stack");
  assert.ok(offered(false).includes("deploy"));
  // A native stack, and a host too old to say, keep them.
  assert.ok(offered(true).includes("rollback-native"));
  assert.ok(offered(null).includes("rollback-native"));
});

test("the body carries only what is set; the preview sends a typed name only once it is right", () => {
  const f = actionForm(find("restore"), ctx);
  const v = initialValues(f, { app: "sonarr", nonsense: "x" });
  assert.equal(v.app, "sonarr");
  assert.ok(!("nonsense" in v));
  assert.deepEqual(buildArgs(f, v), { app: "sonarr" });
  // cli-yes: the server fills the name in for the preview; the line
  // carries --yes only once the name typed here is right.
  assert.deepEqual(previewArgs(f, v), { app: "sonarr" });
  assert.equal(nameTyped(f, v), false);
  assert.deepEqual(previewArgs(f, { ...v, confirm: "medi" }), {
    app: "sonarr",
  });
  assert.equal(nameTyped(f, { ...v, confirm: "media" }), true);
  v.skip_safety_copy = true;
  v.snapshot = "  ab12cd  ";
  v.confirm = "media";
  assert.deepEqual(buildArgs(f, v), {
    app: "sonarr",
    snapshot: "ab12cd",
    skip_safety_copy: true,
    confirm: "media",
  });
  // A wipe only lists unless the name was typed right.
  const w = actionForm(find("wipe"), ctx);
  assert.deepEqual(previewArgs(w, { confirm: "med" }), {});
  assert.deepEqual(previewArgs(w, { confirm: "media" }), { confirm: "media" });
});

test("the checks name the field and say what to do", () => {
  const f = actionForm(find("restore"), ctx);
  const v = initialValues(f);
  assert.deepEqual(checkValues(f, v, "options"), {});
  assert.match(checkValues(f, v, "review").confirm, /Type media/);
  v.confirm = "medi";
  assert.match(checkValues(f, v).confirm, /not media/);
  v.confirm = "media";
  v.snapshot = "no spaces allowed";
  assert.deepEqual(Object.keys(checkValues(f, v)), ["snapshot"]);
  const rb = actionForm(find("rollback-native"), ctx);
  assert.match(
    checkValues(rb, initialValues(rb)).unit,
    /Choose the native unit/,
  );
  const c = fieldChoices(rb.steps[0].fields[0], { units: ["kyu", "almanac"] });
  assert.deepEqual(
    c.map((x) => x.value),
    ["", "kyu", "almanac"],
  );
  const u = actionForm(find("update"), ctx);
  const apps = fieldChoices(u.steps[0].fields[0], { apps: ["sonarr"] });
  assert.deepEqual(apps, [
    { value: "", label: "Every app" },
    { value: "sonarr", label: "sonarr" },
  ]);
});

// dashboard-latest: the "release tag" dropdown of update-host and
// install-native — a server-built list ("latest" first, an unsigned
// release disabled), never a free-text tag.
test("the release tag is a dropdown the server builds, latest by default", () => {
  const uh = actionForm(
    entry("update-host", { target: "host", args: ["tag"] }),
    ctx,
  );
  const tagField = uh.steps
    .flatMap((s) => s.fields)
    .find((f) => f.name === "tag");
  assert.equal(tagField?.kind, "choice");
  assert.equal(tagField?.source, "releases");
  // The dashboard's server sends the choices ready-made; fieldChoices just
  // passes them through, with no synthetic empty/"Choose…" row prepended.
  const releases = [
    { value: "latest", label: "latest (now v3.63.0)", disabled: false },
    { value: "v3.63.0", label: "v3.63.0", disabled: false },
    { value: "v3.62.2", label: "v3.62.2 (unsigned)", disabled: true },
  ];
  assert.deepEqual(
    fieldChoices(/** @type {*} */ (tagField), { releases }),
    releases,
  );
  assert.deepEqual(fieldChoices(/** @type {*} */ (tagField), {}), []);
  // A fresh form starts on "latest", not empty — the dropdown's top row.
  const v = initialValues(uh);
  assert.equal(v.tag, "latest");
  // The body sends "latest" through unchanged; the server resolves it to
  // the concrete tag for the preview and the job (never shown here).
  assert.deepEqual(buildArgs(uh, v), { tag: "latest" });
});

test("a batch asks each typed name and leaves out what differs per stack", () => {
  assert.ok(!batchActions(catalog).some((a) => a.action === "rollback-native"));
  assert.ok(!batchActions(catalog).some((a) => a.target === "host"));
  const f = batchForm(find("destroy"), ["media", "books"], "admin");
  assert.deepEqual(
    f.shared.map((x) => x.name),
    ["skip_backup"],
  );
  assert.deepEqual(
    f.confirms.map((c) => c.stack),
    ["media", "books"],
  );
  assert.equal(f.submit, "Destroy 2 stacks");
  const v = {
    "confirm:media": "media",
    "confirm:books": "book",
    skip_backup: true,
  };
  assert.deepEqual(batchConfirmErrors(f, v), ["books"]);
  assert.deepEqual(batchBody(f, v), {
    action: "destroy",
    stacks: ["media", "books"],
    args: { skip_backup: true },
    confirms: { media: "media", books: "book" },
  });
  assert.match(
    String(batchForm(find("destroy"), ["admin"], "admin").refused),
    /own stack/,
  );
  // Kenny, 2026-10-04: the batch Deploy's Skip the backups, off by
  // default; a single Deploy's form has nothing to skip.
  const d = batchForm(find("deploy"), ["media", "books"], "admin");
  const skip = d.shared.find((x) => x.name === "skip_backup");
  assert.ok(skip, "the batch Deploy asks Skip the backups");
  assert.equal(skip.label, "Skip the backups (deploy without one)");
  assert.deepEqual(batchBody(d, { skip_backup: false }).args, {});
  assert.deepEqual(batchBody(d, { skip_backup: true }).args, {
    skip_backup: true,
  });
  assert.ok(
    !formFields(
      actionForm(find("deploy"), { stack: "media", selfStack: "admin" }),
    ).some((f) => f.name === "skip_backup"),
    "a single Deploy has nothing to skip",
  );
  const u = batchForm(find("update"), ["media", "admin"], "admin");
  assert.deepEqual(u.shared, []);
  assert.equal(u.restartsDashboard, true);
  assert.deepEqual(batchBody(u, {}), {
    action: "update",
    stacks: ["media", "admin"],
    args: {},
  });
});

test("every action is a palette command, the open stack's first", () => {
  /** @type {string[]} */
  const opened = [];
  const off = registerCommands(
    "actions-test",
    actionCommands(
      () => catalog,
      () => "media",
      (s, a) => opened.push(`${s}/${a}`),
    ),
  );
  const fleet = /** @type {any} */ ({
    stacks: [{ name: "admin" }, { name: "media" }],
  });
  const all = allCommands({ fleet, themes: [], theme: null }).filter((c) =>
    c.id.startsWith("action:"),
  );
  off();
  // feat-shell-2: every action is in "Do"; the open stack's come first.
  assert.ok(all.every((c) => c.group === "Do"));
  assert.equal(all[0].stack, "media");
  assert.ok(all.some((c) => c.id === "action:_host:patch"));
  assert.ok(!all.some((c) => c.id === "action:admin:destroy"));
  assert.ok(all.some((c) => c.id === "action:admin:deploy"));
  // A `*-native` action is left out for a stack without native services.
  assert.ok(!all.some((c) => c.id === "action:media:rollback-native"));
  assert.match(
    /** @type {string} */ (
      all.find((c) => c.id === "action:media:update")?.words
    ),
    /upgrade/,
  );
  all.find((c) => c.id === "action:media:restore")?.run?.();
  assert.deepEqual(opened, ["media/restore"]);
  assert.deepEqual(
    actionCommands(
      () => null,
      () => null,
      () => {},
    )({ fleet, themes: [], theme: null }),
    [],
  );
});

/** @returns {import("../js/jobs.js").Job} */
const job = (/** @type {Partial<import("../js/jobs.js").Job>} */ o = {}) => ({
  job: 7,
  origin: { from: "manual" },
  stack: "media",
  action: "deploy",
  args: {},
  state: "running",
  queued_at: 1000,
  started_at: 1010,
  finished_at: null,
  reqs: [41],
  message: null,
  cli: "homelab deploy media",
  progress: null,
  restarts_dashboard: false,
  ...o,
});
/** @returns {import("../js/jobs.js").Progress} */
const prog = (
  /** @type {Partial<import("../js/jobs.js").Progress>} */ o = {},
) => ({
  op: "deploy-media",
  step: "pull images",
  n: 3,
  m: 35,
  finished: false,
  changed: false,
  expected_step_s: 20,
  expected_total_s: 300,
  expected_remaining_s: 200,
  elapsed_s: 90,
  runs: 5,
  ...o,
});

test("the job panel reads step 3/35 with the time run and the time left", () => {
  const j = job({ progress: prog() });
  const v = jobPanel(j, {
    label: (a) => (a === "deploy" ? "Deploy" : a),
    progressAt: 1100,
    now: 1130,
  });
  assert.equal(v.title, "Deploy · media");
  assert.equal(v.step, "step 3/35");
  assert.equal(v.stepName, "pull images");
  assert.equal(v.elapsed, "2 min"); // 1130 - 1010
  assert.equal(v.remaining, "about 2 min 50 s left"); // 200 - 30
  assert.equal(v.basis, "expected from 5 earlier runs");
  // fix-189 (Kenny, 2026-10-02): a step total is the bar's one truth once
  // the host sends one; time history (which alone would say 41% here,
  // 120 / (120 + 170)) now only feeds "Expected remaining" above.
  assert.equal(v.percent, 6); // (3 - 1) / 35
  assert.equal(v.finished, false);
  assert.equal(v.badge.label, "running");
});

// fix-189: 263/310 with an earlier, shorter run's time estimate expired
// (no time left to go by) read 99% — the bar pinned at "almost done" on a
// run barely a third through its own steps. Steps-only once there is a
// total: 85%, and it never goes backwards or claims 100 before `done`.
test("fix_189_the_bar_is_steps_only_once_it_has_a_total_never_pinned_at_99", () => {
  const mid = job({ progress: prog({ n: 263, m: 310 }) });
  assert.equal(percent(mid, 1100, 1130), 85); // (263 - 1) / 310
  assert.notEqual(percent(mid, 1100, 1130), 99);

  // Monotonic: as the host marks more steps, the bar never drops back.
  let last = -1;
  for (const n of [1, 50, 120, 200, 263, 309, 310]) {
    const p = /** @type {number} */ (
      percent(job({ progress: prog({ n, m: 310 }) }), 1100, 1130)
    );
    assert.ok(p >= last, `${p} should not be behind ${last} at step ${n}`);
    last = p;
  }

  // Done is 100, from the job's own state, not the step math.
  assert.equal(
    percent(
      job({ state: "done", progress: prog({ n: 310, m: 310 }) }),
      1100,
      1130,
    ),
    100,
  );
});

test("no earlier run: the step without a count and an honest remaining", () => {
  const j = job({
    progress: prog({
      m: null,
      expected_remaining_s: null,
      expected_total_s: null,
      runs: 0,
    }),
  });
  assert.equal(stepText(j), "step 3");
  assert.equal(remaining(j, 1000, 1100).text, "no earlier run to go by");
  assert.equal(percent(j, 1000, 1100), null);
  const late = job({ progress: prog({ expected_remaining_s: 10 }) });
  assert.deepEqual(remaining(late, 1000, 1100), {
    text: "taking longer than earlier runs",
    late: true,
    s: 0,
  });
  assert.equal(
    stepText(job({ state: "queued", started_at: null })),
    "waiting in the queue",
  );
  const noM = job({ progress: prog({ expected_remaining_s: null }) });
  assert.equal(percent(noM, 1000, 1100), 6); // (3 - 1) / 35
});

// Kenny, 2026-09-29: a deploy's bar stayed empty and filled only at its
// last step. The bar never trails the steps already finished, and a job
// with no count to go by is busy (indeterminate), never 0%.
test("the bar advances with each step and is busy without a count", () => {
  const slow = job({
    progress: prog({ n: 20, m: 31, expected_remaining_s: 5000 }),
  });
  // Time says 2% (20 s run of 5020 s); 19 of 31 steps are behind it.
  assert.equal(percent(slow, 1030, 1030), 61);
  const unknown = job({
    progress: prog({ n: 31, m: null, expected_remaining_s: null, runs: 0 }),
  });
  assert.equal(percent(unknown, 1030, 1030), null);
  assert.equal(stepText(unknown), "step 31");
  assert.equal(
    jobPanel(unknown, { label: (a) => a, progressAt: 1030, now: 1030 }).percent,
    null,
  );
});

test("the end of a job says what came of it", () => {
  const done = job({ state: "done", finished_at: 1210, progress: prog() });
  const v = jobPanel(done, { label: (a) => a, progressAt: null, now: 5000 });
  assert.equal(v.elapsed, "3 min 20 s");
  assert.equal(v.percent, 100);
  assert.equal(v.remaining, "");
  assert.equal(v.outcome?.tone, "success");
  assert.equal(
    outcome(job({ state: "failed", message: "pull failed" }))?.text,
    "pull failed",
  );
  assert.match(
    String(outcome(job({ state: "unknown" }))?.text),
    /Activity page/,
  );
  assert.equal(outcome(job({ state: "deferred" }))?.tone, "info");
  assert.equal(outcome(job()), null);
});

// feat-jobpanel-1 (Kenny, 2026-10-02): the panel's facts grid never shows
// an empty cell — every state (queued, running, done, failed, paused-ish
// via "unknown") gives State, Step, Origin, Running for and Expected
// remaining a real value, so the grid's cells never collapse or reflow as
// a job moves between states.
test("feat_jobpanel_1_every_fact_cell_is_non_empty_in_every_state", () => {
  const ctx = {
    label: (/** @type {string} */ a) => a,
    progressAt: 1100,
    now: 1130,
  };
  const states = [
    job({ state: "queued", started_at: null, progress: null }),
    job({ state: "running", progress: prog() }),
    job({ state: "running", progress: null }),
    job({ state: "done", finished_at: 1210, progress: prog() }),
    job({ state: "failed", finished_at: 1210, progress: prog() }),
    job({ state: "unknown", progress: null }),
  ];
  for (const j of states) {
    const cells = jobFacts(jobPanel(j, ctx));
    assert.equal(cells.length, 5);
    for (const c of cells)
      assert.ok(
        c.value && c.value.length > 0,
        `${j.state}'s "${c.label}" cell must not be empty`,
      );
  }
  // The State cell carries the badge to draw, not just its label text.
  const stateCell = jobFacts(jobPanel(job({ state: "failed" }), ctx)).find(
    (c) => c.key === "state",
  );
  assert.equal(stateCell?.badge?.label, "failed");
});

test("jobs merge newest first, progress survives a later job view", () => {
  let jobs = upsertJob([], job({ job: 1 }));
  jobs = upsertJob(jobs, job({ job: 3, state: "queued" }));
  jobs = applyProgress(jobs, { job: 3, progress: prog() });
  assert.equal(jobs[0].job, 3);
  assert.equal(jobs[0].state, "running");
  jobs = upsertJob(jobs, job({ job: 3, state: "running", progress: null }));
  assert.equal(jobs[0].progress?.n, 3);
  const rows = jobRows(jobs, (a) => a.toUpperCase(), 1070);
  assert.deepEqual(
    rows.map((r) => [r.job, r.action, r.tookText, r.origin]),
    [
      [3, "DEPLOY", "1 min", "by hand"],
      [1, "DEPLOY", "1 min", "by hand"],
    ],
  );
  const logs = new Map();
  for (let i = 0; i < 5; i++)
    addLog(
      logs,
      { job: 3, req: 1, level: "info", source: "host", msg: `l${i}`, ts: 1 },
      3,
    );
  assert.deepEqual(
    logs.get(3).map((/** @type {any} */ l) => l.msg),
    ["l2", "l3", "l4"],
  );
  const l = logLine(
    { job: 3, req: 1, level: "WARN", source: "", msg: "x", ts: 0 },
    { locale: "en-GB", timeZone: "UTC" },
  );
  assert.deepEqual(
    [l.time, l.source, l.severity],
    ["00:00:00", "host", "warning"],
  );
});

// arch-self: install-native-admin restarts the dashboard mid-job; the tab
// that started it must not be left saying "running" forever once the host
// finished it.
test("a job that restarted the dashboard is read back from the host's history", () => {
  const restarting = job({
    job: 9,
    action: "install-native",
    stack: "admin",
    state: "running",
    started_at: 1000,
    restarts_dashboard: true,
    progress: prog({ n: 12, m: 22 }),
  });
  const other = job({ job: 10, restarts_dashboard: false, state: "running" });
  const doneAlready = job({
    job: 11,
    state: "done",
    restarts_dashboard: true,
  });
  const jobs = [restarting, other, doneAlready];
  assert.equal(outcomeSubject(restarting), "install-native-admin");
  // Only the still-running job that restarts the dashboard needs asking
  // about; a normal job and one already finished do not.
  assert.deepEqual(
    jobsAwaitingOutcome(jobs).map((j) => j.job),
    [9],
  );

  const found = applyOutcome(jobs, 9, {
    found: true,
    ok: true,
    steps: 22,
    end: 1500,
  });
  const patched = found.find((j) => j.job === 9);
  assert.equal(patched?.state, "done");
  assert.equal(patched?.finished_at, 1500);
  assert.equal(patched && stepText(patched), "step 22/22");
  assert.match(String(patched?.message), /host's jobs history/);
  // A job unrelated to the poll, and one that was never awaiting one, are
  // untouched.
  assert.deepEqual(
    found.find((j) => j.job === 10),
    other,
  );
  assert.deepEqual(
    found.find((j) => j.job === 11),
    doneAlready,
  );

  const failed = applyOutcome(jobs, 9, {
    found: true,
    ok: false,
    steps: 22,
    end: 1500,
  });
  assert.equal(failed.find((j) => j.job === 9)?.state, "failed");

  // Nothing in the host's history within the poll window: shown as
  // "unknown", pointing at where the real outcome lives.
  const unknown = applyOutcome(jobs, 9, {
    found: false,
    ok: false,
    steps: 0,
    end: 0,
  });
  const u = unknown.find((j) => j.job === 9);
  assert.equal(u?.state, "unknown");
  assert.match(String(u?.message), /jobs history on the host/);
  assert.deepEqual(jobsAwaitingOutcome(unknown), []);
});

test("a batch counts its jobs", () => {
  const v = batchView({
    batch: 2,
    jobs: [
      { job: 1, stack: "a", state: "done", message: null },
      { job: 2, stack: "b", state: "failed", message: "no" },
      { job: 3, stack: "c", state: "running", message: null },
    ],
    done: 2,
    ok: 1,
    failed: 1,
    deferred: 0,
  });
  assert.equal(v.text, "2 of 3 finished · 1 done · 1 failed");
  assert.equal(v.percent, 67);
  assert.equal(v.finished, false);
  assert.equal(v.rows[1].badge.tone, "bad");
});

test("roll back says what the host deployed, and what it cannot tell", () => {
  const v = rollbackView({
    stack: "media",
    applied_source: "git",
    applied_commit: "0123456789abcdef",
    working_copy: false,
    commits: [
      { commit: "0123456789abcdef", at: 1, subject: "s", applied: true },
    ],
    native_units: ["kyu"],
    missing: ["the host does not list x"],
  });
  assert.equal(v.applied, "The host last deployed commit 0123456789 (git).");
  assert.match(String(v.noWorkingCopy), /no working copy/);
  assert.equal(v.commits[0].applied, "deployed now");
  assert.deepEqual(v.missing, ["the host does not list x"]);
});

test("a schedule's when in words and back from the form", () => {
  assert.equal(whenText({ every: "day", at: "03:00" }), "Every day at 03:00");
  assert.equal(
    whenText({ every: "week", days: [3, 0], at: "04:30" }, "Europe/Brussels"),
    "On Mon, Thu at 04:30 (Europe/Brussels)",
  );
  assert.equal(
    whenText({ every: "week", days: [0, 1, 2, 3, 4], at: "01:00" }),
    "On weekdays at 01:00",
  );
  assert.equal(
    whenText({ every: "once", date: "2026-10-01", at: "02:00" }, undefined, {
      locale: "en-GB",
    }),
    "Once on 01/10/2026 at 02:00",
  );
  assert.deepEqual(
    whenFromValues({ every: "week", at: "03:00", days: [4, 1, 4], date: "" }),
    {
      ok: true,
      when: { every: "week", days: [1, 4], at: "03:00" },
    },
  );
  assert.equal(
    whenFromValues({ every: "week", at: "03:00", days: [], date: "" }).ok,
    false,
  );
  assert.equal(
    whenFromValues({ every: "day", at: "24:00", days: [], date: "" }).ok,
    false,
  );
  assert.equal(
    whenFromValues({ every: "once", at: "03:00", days: [], date: "" }).ok,
    false,
  );
  assert.deepEqual(whenValues(null), {
    every: "day",
    at: "03:00",
    days: [],
    date: "",
  });
  const s = {
    id: "s1",
    stack: "media",
    action: "backup",
    args: {},
    when: /** @type {const} */ ({ every: "day", at: "03:00" }),
    enabled: true,
    note: "nightly",
    created_at: 1,
    handled_until: 1,
  };
  const rows = scheduleRows(
    [
      {
        schedule: s,
        next_run: 1790000000,
        next_run_local: null,
        last_job: null,
      },
    ],
    () => "Back up",
    { locale: "en-GB", timeZone: "UTC" },
  );
  assert.equal(rows[0].nextText, "21/09/2026 14:13");
  assert.equal(rows[0].last, "never");
  assert.deepEqual(toggledBody(s, false), {
    stack: "media",
    action: "backup",
    args: {},
    when: { every: "day", at: "03:00" },
    enabled: false,
    note: "nightly",
  });
  const sched = schedulableActions(catalog).map((a) => a.action);
  assert.ok(sched.includes("backup") && sched.includes("patch"));
  assert.ok(
    !sched.includes("restore") &&
      !sched.includes("wipe") &&
      !sched.includes("rollback-native"),
  );
  assert.deepEqual(scheduleArgFields(find("deploy"), "media"), []);
  assert.deepEqual(
    scheduleArgFields(find("update"), "media").map((f) => f.id),
    ["sched-app"],
  );
});

test("the bell, the snooze and the per-stack switches", () => {
  assert.deepEqual(bell(0), { count: "", label: "Notifications, none unread" });
  assert.equal(bell(3).count, "3");
  // feat-shell-4 (Kenny, 2026-10-03): counters are exact, never "9+".
  assert.equal(bell(12).count, "12");
  assert.equal(bell(130).count, "130");
  const settings = {
    push: true,
    muted_stacks: ["books"],
    snooze_until: 1600,
    digest_at: "09:00",
  };
  assert.deepEqual(snoozeState(settings, 2000), {
    on: false,
    text: "Not snoozed",
  });
  const s = snoozeState(settings, 1000, { locale: "en-GB", timeZone: "UTC" });
  assert.equal(s.on, true);
  assert.match(s.text, /^Snoozed until .*00:26 \(10 min from now\)$/);
  const snap = {
    notices: [],
    unread: 0,
    unread_by_stack: { media: 2 },
    settings,
    snoozed: false,
  };
  assert.deepEqual(stackMuteRows(["media"], snap), [
    { stack: "books", unread: 0, muted: true },
    { stack: "media", unread: 2, muted: false },
  ]);
  assert.deepEqual(settingsBody(settings, { push: false }), {
    push: false,
    muted_stacks: ["books"],
    digest_at: "09:00",
    snooze_until: 1600,
  });
  // Decision daily-digest: empty is no digest.
  assert.equal(settingsBody(settings, { digest_at: null }).digest_at, null);
  assert.ok(
    !("snooze_until" in settingsBody({ ...settings, snooze_until: null }, {})),
  );
});

test("notices: rows, a new one on top, its toast and its push", () => {
  /** @type {import("../js/notices.js").Notice} */
  const n = {
    id: 5,
    at: 100,
    kind: "action_failed",
    stack: "media",
    title: "Deploy media failed",
    body: "pull failed",
    job: 7,
    read: false,
    push: { state: "skipped", why: "snoozed" },
  };
  const rows = noticeRows([n]);
  assert.deepEqual(
    [rows[0].kind.label, rows[0].push, rows[0].read],
    ["action failed", "not sent: snoozed", "unread"],
  );
  assert.equal(pushText({ state: "sent" }), "sent");
  assert.deepEqual(toastOf(n), {
    text: "Deploy media failed: pull failed",
    tone: "error",
  });
  const snap = addNotice(
    {
      notices: [],
      unread: 0,
      unread_by_stack: {},
      settings: { push: true, muted_stacks: [], digest_at: "09:00" },
      snoozed: false,
    },
    { notice: n, unread: 1 },
  );
  assert.equal(snap?.unread, 1);
  assert.equal(snap?.unread_by_stack.media, 1);
  assert.equal(addNotice(null, { notice: n, unread: 1 }), null);
});

test("notify-detail: level, what to do, the page and the Fix button", () => {
  /** @type {import("../js/notices.js").Notice} */
  const n = {
    id: 9,
    at: 100,
    kind: "host_event",
    stack: "media",
    title: "deploy media failed",
    body: "compose: pull failed",
    read: false,
    push: { state: "by_sender", who: "the host" },
    level: "critical",
    since: 90,
    consequence: "It may run the old version.",
    remedy: "Run it again: `homelab deploy media`.",
    link: "/stacks/media",
    fixes: [{ action: "deploy", stack: "media", label: "Deploy" }],
  };
  const [r] = noticeRows([n]);
  assert.deepEqual(r.level, { label: "urgent", tone: "bad" });
  assert.equal(r.kind.label, "host");
  assert.equal(r.since, 90);
  assert.equal(r.link, "/stacks/media");
  assert.equal(r.push, "pushed by the host");
  assert.equal(fixLabel(r.fixes[0]), "Fix: Deploy");
  assert.equal(toastOf(n).tone, "error");
  // A notice from before levels existed reads as its kind says.
  assert.equal(
    levelBadge({ ...n, level: undefined, kind: "action_failed" }).label,
    "warning",
  );
  assert.equal(
    pushText({ state: "by_sender", who: "Alertmanager" }),
    "pushed by Alertmanager",
  );
  assert.equal(digestText(null), "No digest sent yet.");
  assert.match(
    digestText(
      { day: "2026-09-30", at: 0, count: 2, push: { state: "sent" } },
      { locale: "en-GB", timeZone: "UTC" },
    ),
    /2 thing\(s\) waited, pushed\.$/,
  );
});

test("Today and the fleet check carry each row's Fix", () => {
  const fix = { action: "backup", stack: "media", label: "Back up" };
  const v = todayView({
    today: {
      items: [
        { level: "Attention", source: "check", what: "w", remedy: "r" },
        {
          level: "Broken",
          source: "check",
          what: "x",
          remedy: "homelab backup media",
          fix,
        },
      ],
      unread: [],
    },
    verdict: "2 things need you",
    needs_you: true,
    stack_files: 1,
  });
  assert.deepEqual(v.items[0].fix, fix, "broken first, with its fix");
  assert.equal(v.items[1].fix, null);
  const rows = findingRows([
    { severity: "Drift", subject: "a", what: "w", remedy: "r" },
    {
      severity: "Broken",
      subject: "b",
      what: "w",
      remedy: "homelab backup media",
      fix,
    },
  ]);
  assert.deepEqual(rows[0].fix, fix);
  assert.equal(rows[1].fix, null);
});
