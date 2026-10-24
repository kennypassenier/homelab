// redesign-flows-1/2/3 (redesign 3.71.0, FLOWS.md §1.2 and §3.1; Kenny
// approved 2026-10-03): the Inbox's new sources as rows, the one Update
// flow's pure half, and the Help panel's tour.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  GROUPS,
  askId,
  askSpec,
  bulkRows,
  countWords,
  fixAllBatch,
  fixAllRule,
  groupOf,
  groupRows,
  kindOf,
  problemOf,
  setupRows,
  shownRows,
  staleApps,
  todayRows,
  updateRows,
  worthRows,
} from "../js/inboxrows.js";
import {
  firstChosen,
  flowTitle,
  itemsFor,
  lastDeployS,
  logVerdict,
  runRows,
  safetyNet,
  scopeOf,
  updateHref,
  updatesBody,
  whoNotices,
} from "../js/updateflow.js";
import { GLOSSARY, shouldTour, tourSteps } from "../js/helptour.js";

const STALE = {
  measured_at: 1000,
  images: [
    {
      where_: "admin/demo-agent",
      pinned: "v0.1.0",
      latest: "v0.2.0",
      upstream: "github.com/example/demo-agent",
      key: null,
    },
    {
      where_: "beta-demo/api",
      pinned: "v2.3.0",
      latest: "v3.0.0",
      upstream: "github.com/example/demo-api",
      key: "api/api",
    },
    {
      where_: "beta-demo/demo-web",
      pinned: "1.4.2",
      latest: "1.5.0",
      upstream: "github.com/example/demo-web",
      key: "web/web",
    },
    {
      where_: "kp-soft/demo-agent",
      pinned: "0.1.4",
      latest: "0.2.0",
      upstream: "github.com/example/demo-agent",
      key: "agent/agent",
    },
  ],
};

test("redesign-flows-2: the stale images are ONE Updates row naming every app the dashboard can move, majors marked", () => {
  const apps = staleApps(STALE);
  assert.deepEqual(
    apps.map((a) => `${a.stack}/${a.container}`),
    ["beta-demo/api", "beta-demo/demo-web", "kp-soft/demo-agent"],
    "a pin held outside the stack files is no row",
  );
  const rows = updateRows(STALE);
  assert.equal(rows.length, 1);
  const r = rows[0];
  // redesign-final-h1: every app with a newer version, the one that comes
  // with a homelab release included and said so (Stacks counts it too).
  assert.equal(r.title, "4 apps have a newer version");
  assert.match(
    r.why,
    /admin\/demo-agent v0\.1\.0 → v0\.2\.0 \(with a homelab release\)/,
  );
  assert.equal(kindOf(r), "update");
  assert.match(r.why, /beta-demo\/api v2\.3\.0 → v3\.0\.0 \(major\)/);
  assert.match(r.why, /kp-soft\/demo-agent 0\.1\.4 → 0\.2\.0(?! \(major\))/);
  assert.deepEqual(r.stacks, ["admin", "beta-demo", "kp-soft"]);
  assert.equal(r.href, "/inbox?update=all");
  assert.deepEqual(updateRows({ images: [] }), []);
  assert.deepEqual(updateRows(null), []);
  assert.equal(
    updateRows({ images: [STALE.images[1]] })[0].title,
    "1 app has a newer version",
  );
});

test("redesign-flows-2: the stacks without a sealed env are ONE Setup row", () => {
  const rows = setupRows({
    stacks: [
      { name: "gateway", env_sealed: false },
      { name: "notes", env_sealed: true },
      { name: "films", env_sealed: false },
      { name: "old" },
    ],
  });
  assert.equal(rows.length, 1);
  assert.equal(rows[0].title, "2 stacks have no sealed env on the host");
  assert.deepEqual(rows[0].stacks, ["gateway", "films"]);
  assert.equal(kindOf(rows[0]), "setup");
  assert.deepEqual(
    setupRows({ stacks: [{ name: "a", env_sealed: true }] }),
    [],
  );
  assert.deepEqual(setupRows(null), []);
});

test("redesign-flows-2: Today's items are rows with their remedy and fix; a doctor env item the Setup row names is left out", () => {
  const body = {
    measured_at: 5,
    today: {
      items: [
        {
          level: "Broken",
          source: "incident",
          what: "deploy-notes failed and nothing on its stack has succeeded since",
          remedy: "read the report",
        },
        {
          level: "Broken",
          source: "doctor",
          what: "stack gateway env: a secret file on the container has no copy in the host's vault",
          remedy: "redeploy gateway",
        },
        {
          level: "Attention",
          source: "check",
          what: "kp-soft: image drift",
          remedy: "deploy kp-soft",
          fix: { action: "deploy", stack: "kp-soft", label: "Deploy kp-soft" },
        },
      ],
    },
  };
  const rows = todayRows(body, ["gateway"]);
  assert.equal(rows.length, 2);
  assert.equal(kindOf(rows[0]), "fail");
  assert.equal(rows[0].severity, "bad");
  assert.equal(kindOf(rows[1]), "check");
  assert.equal(rows[1].fix?.action, "deploy");
  assert.equal(rows[1].why, "What to do: deploy kp-soft");
  assert.equal(
    todayRows(body, []).length,
    3,
    "no Setup row: the doctor item stays",
  );
});

test("redesign-flows-2: worth a look counts never-drilled repositories and a newer host, never in the counter", () => {
  const rows = worthRows(
    [
      {
        repos: [
          { drill: null },
          { drill: { last_attempt: 1, last_error: null } },
        ],
      },
      { repos: [{}] },
    ],
    { update_available: true, latest: "3.71.0", host: "3.70.7" },
  );
  assert.deepEqual(
    rows.map((r) => r.title),
    [
      "2 of 3 backup repositories were never restore-drilled",
      "Host 3.71.0 is available",
    ],
  );
  assert.ok(rows.every((r) => r.severity === "info"));
  assert.deepEqual(worthRows([], { update_available: false }), []);
});

test("redesign-flows-2: plain-click kinds filter the rows (none on: all); the card counts them exactly", () => {
  /** @type {any[]} */
  const rows = [
    { key: "a", severity: "bad", source: "asks", at: 1 },
    { key: "b", severity: "warn", kind: "update", source: "updates", at: 9 },
    { key: "c", severity: "warn", kind: "setup", source: "setup", at: 5 },
  ];
  assert.equal(shownRows(rows, new Set()).length, 3);
  assert.deepEqual(
    shownRows(rows, new Set(["update", "ask"])).map((r) => r.key),
    ["a", "b"],
  );
  assert.equal(countWords(3, 3), "3 items waiting");
  assert.equal(countWords(1, 1), "1 item waiting");
  assert.equal(countWords(1, 3), "1 of 3 items shown");
});

test("kp11: the rows stand under Urgent, Updates, Setup and Checks, in that order, an empty heading left out", () => {
  /** @type {any[]} */
  const rows = [
    { key: "c", severity: "warn", kind: "check", source: "checks", at: 5 },
    { key: "a", severity: "bad", source: "asks", at: 1 },
    { key: "f", severity: "warn", source: "notices", at: 3 },
    { key: "u", severity: "warn", kind: "update", source: "updates", at: 9 },
  ];
  assert.deepEqual([...GROUPS], ["Urgent", "Updates", "Setup", "Checks"]);
  assert.equal(groupOf(rows[1]), "Urgent");
  assert.equal(groupOf(rows[2]), "Urgent");
  assert.deepEqual(
    groupRows(rows).map((g) => [g.name, g.rows.map((r) => r.key)]),
    [
      ["Urgent", ["a", "f"]],
      ["Updates", ["u"]],
      ["Checks", ["c"]],
    ],
  );
});

test("kp11: a failure's fold reads what the notice carries and needs a remedy", () => {
  /** @type {any} */
  const row = {
    key: "notice:7",
    severity: "bad",
    problem: {
      what: "The upload was refused",
      why: "The key expired",
      remedy: "Renew it",
    },
  };
  assert.deepEqual(problemOf(row), {
    key: "notice:7",
    what: "The upload was refused",
    why: "The key expired",
    remedy: "Renew it",
    severity: "critical",
  });
  assert.equal(problemOf({ ...row, problem: { what: "x" } }), null);
  assert.equal(
    problemOf(/** @type {any} */ ({ key: "k", severity: "warn" })),
    null,
  );
});

test("kp11: a host question becomes a question card whose safe answer is to leave it stopped", () => {
  const ask = {
    id: 41,
    boot: "b1",
    op: "deploy",
    step: "record the new value",
    what: "The deploy needs a decision",
    if_allowed: "the deploy goes on",
    if_stopped: "",
    asked_at: 100,
    deadline: 160,
  };
  const spec = askSpec(ask);
  assert.equal(spec.id, askId(ask));
  assert.equal(spec.id, "b1-41");
  assert.equal(spec.question, "deploy is waiting at “record the new value”");
  assert.equal(spec.deadline, 160000);
  assert.equal(spec.asked, 100000);
  assert.equal(spec.defaultAnswer, "stop");
  assert.deepEqual(
    spec.answers?.map((a) => [a.value, a.consequence]),
    [
      ["allow", "The deploy goes on."],
      ["stop", "The operation stops here."],
    ],
  );
  assert.equal(askId({ ...ask, boot: null }), "boot-41");
});

test("kp11: the bulk card lists the first ten unread notices of the last day and counts all that match", () => {
  const now = 1_000_000;
  /** @type {any[]} */
  const notices = [
    ...Array.from({ length: 12 }, (_, i) => ({
      id: i + 1,
      at: now - 60 * (i + 1),
      read: false,
    })),
    { id: 90, at: now - 90_000, read: false },
    { id: 91, at: now - 60, read: true },
  ];
  const r = bulkRows(notices, now);
  assert.equal(r.rows.length, 10);
  assert.equal(r.rows[0].id, 1);
  assert.equal(r.matching, 12);
  assert.equal(r.ids.length, 12);
});

test("kp11: Fix all is on only for notices that share one fix without choices of its own", () => {
  const fix = (/** @type {string} */ stack, /** @type {any} */ args = {}) => ({
    action: "backup",
    stack,
    label: "Back up",
    args,
  });
  /** @type {Map<number, any>} */
  const byId = new Map([
    [1, { id: 1, fixes: [fix("a")] }],
    [2, { id: 2, fixes: [fix("b")] }],
    [3, { id: 3, fixes: [] }],
    [4, { id: 4, fixes: [{ ...fix("c"), action: "deploy" }] }],
    [5, { id: 5, fixes: [fix("d", { app: "x" })] }],
    [6, { id: 6, fixes: [fix("_host")] }],
  ]);
  assert.equal(fixAllRule([1, 2], byId), "");
  assert.deepEqual(fixAllBatch([1, 2], byId), {
    action: "backup",
    stacks: ["a", "b"],
  });
  assert.equal(fixAllRule([3], byId), "None of these can be fixed");
  assert.equal(fixAllRule([1, 3], byId), "1 of 2 can be fixed");
  assert.match(fixAllRule([1, 4], byId), /different fixes/);
  assert.match(fixAllRule([5], byId), /own choices/);
  assert.match(fixAllRule([6], byId), /own choices/);
});

test("redesign-flows-1: the Update flow's address and scope", () => {
  // redesign-flows-11: its own address since the review.
  assert.equal(updateHref(), "/update?all=1");
  assert.equal(updateHref("kp-soft"), "/update?stack=kp-soft");
  assert.equal(
    updateHref("beta-demo", "api/api"),
    "/update?stack=beta-demo&app=api%2Fapi",
  );
  assert.deepEqual(scopeOf("?all=1"), { all: true });
  assert.deepEqual(scopeOf("?stack=beta-demo&app=api%2Fapi"), {
    all: false,
    stack: "beta-demo",
    app: "api/api",
  });
  assert.equal(scopeOf("?kind=update"), null);
});

test("redesign-flows-1: all apps vs one stack (which also offers its moving-tag pull); what starts ticked", () => {
  const all = itemsFor(STALE, { all: true });
  assert.deepEqual(
    all.map((i) => i.id),
    [
      "pin:beta-demo:api/api",
      "pin:beta-demo:web/web",
      "pin:kp-soft:agent/agent",
      "release:admin:demo-agent",
    ],
  );
  assert.equal(all[0].major, true);
  assert.equal(
    all[0].notes,
    "https://github.com/example/demo-api/releases/tag/v3.0.0",
  );
  assert.equal(firstChosen(all, { all: true }).size, 3);
  assert.equal(flowTitle(all, { all: true }), "Update 3 apps");

  const one = itemsFor(STALE, { all: false, stack: "kp-soft" });
  assert.deepEqual(
    one.map((i) => i.kind),
    ["pin", "pull"],
  );
  assert.deepEqual(
    [...firstChosen(one, { all: false, stack: "kp-soft" })],
    ["pin:kp-soft:agent/agent"],
  );
  assert.equal(
    flowTitle(one, { all: false, stack: "kp-soft" }),
    "Update kp-soft",
  );

  // Nothing pinned is newer: the pull row is ticked.
  const none = itemsFor(STALE, { all: false, stack: "notes" });
  assert.deepEqual(
    [...firstChosen(none, { all: false, stack: "notes" })],
    ["pull:notes"],
  );

  // An opener that picked one app (the Map's row): only that one.
  const s = { all: false, stack: "beta-demo", app: "api/api" };
  const picked = itemsFor(STALE, /** @type {any} */ (s));
  assert.deepEqual(
    [...firstChosen(picked, /** @type {any} */ (s))],
    ["pin:beta-demo:api/api"],
  );
  assert.equal(
    flowTitle(picked, /** @type {any} */ (s)),
    "Update beta-demo/api",
  );
});

test("redesign-flows-1: the impact in words — downtime from the last deploy, who notices, the safety net", () => {
  const chosen = itemsFor(STALE, { all: true })
    .filter((i) => i.kind === "pin")
    .slice(0, 2);
  /** @type {any[]} */
  const jobs = [
    {
      job: 1,
      stack: "beta-demo",
      action: "deploy",
      state: "done",
      started_at: 100,
      finished_at: 160,
    },
    {
      job: 2,
      stack: "beta-demo",
      action: "deploy",
      state: "done",
      started_at: 200,
      finished_at: 225,
    },
    {
      job: 3,
      stack: "beta-demo",
      action: "backup",
      state: "done",
      started_at: 300,
      finished_at: 900,
    },
    {
      job: 4,
      stack: "other",
      action: "deploy",
      state: "done",
      started_at: 0,
      finished_at: 999,
    },
  ];
  assert.equal(lastDeployS(chosen, jobs), 25, "the newest deploy of the stack");
  assert.equal(lastDeployS(chosen, []), null);
  assert.deepEqual(
    whoNotices(chosen, [
      { stack: "beta-demo", depended_on_by: ["alpha-demo"] },
      { stack: "gateway", depended_on_by: ["x"] },
    ]),
    [
      { who: "alpha-demo", what: "beta-demo/api" },
      { who: "alpha-demo", what: "beta-demo/demo-web" },
    ],
  );
  // redesign-flows-6: the demo's own words, for pinned and moving-tag
  // apps alike (the job rolls a pinned app back by itself now).
  assert.equal(safetyNet(chosen).value, "Backup first, auto roll back");
  assert.equal(safetyNet(chosen).ctx, "if the app is not healthy within 2 min");
  const pull = itemsFor(STALE, { all: false, stack: "notes" });
  assert.equal(safetyNet(pull).value, "Backup first, auto roll back");
});

test("flow-stage-by-stage-1: the run's rows — every backup, every commit (pinned only), every deploy, two verifies", () => {
  const pins = itemsFor(STALE, { all: true }).filter((i) => i.kind === "pin");
  assert.deepEqual(
    runRows(pins).map((r) => [r.id, r.step]),
    [
      ["backup:beta-demo", 3],
      ["backup:kp-soft", 3],
      ["commit:beta-demo", 4],
      ["commit:kp-soft", 4],
      ["deploy:beta-demo", 4],
      ["deploy:kp-soft", 4],
      ["health", 5],
      ["logs", 5],
    ],
  );
  assert.equal(runRows(pins)[0].title, "Back up beta-demo");
  const pull = itemsFor(STALE, { all: false, stack: "notes" });
  assert.ok(!runRows(pull).some((r) => r.id.startsWith("commit:")));
});

test("redesign-flows-1: the log check passes on no more errors per minute than the hour before", () => {
  const t = 10_000;
  const at = (/** @type {number} */ s, /** @type {string} */ level) => ({
    ts_ms: s * 1000,
    level,
  });
  const quiet = logVerdict(
    [at(t - 100, "warn"), at(t + 10, "info")],
    t,
    t + 120,
  );
  assert.equal(quiet.ok, true);
  assert.equal(quiet.words, "0 errors since the restart, 0 in the hour before");
  const noisy = logVerdict(
    [at(t + 10, "error"), at(t + 20, "ERROR")],
    t,
    t + 120,
  );
  assert.equal(noisy.ok, false);
  assert.equal(noisy.after, 2);
  const usual = logVerdict(
    [
      ...Array.from({ length: 120 }, (_, i) => at(t - 3500 + i * 25, "error")),
      at(t + 30, "error"),
    ],
    t,
    t + 60,
  );
  assert.equal(usual.ok, true, "one error a minute after, two a minute before");
});

test("redesign-flows-3: the tour is six steps (search, then the areas), shown once, never to an automated browser", () => {
  const steps = tourSteps();
  assert.equal(steps.length, 6);
  assert.equal(steps[0].title, "One box for everything");
  assert.deepEqual(
    steps.slice(1).map((s) => s.title),
    ["Inbox", "Stacks", "Activity", "Backups", "System"],
  );
  assert.match(
    steps[4].phone,
    /data-kp-tab="more"/,
    "Backups is under More on a phone",
  );
  assert.equal(shouldTour({ search: "", toured: false, automated: false }), 0);
  assert.equal(
    shouldTour({ search: "", toured: true, automated: false }),
    null,
  );
  assert.equal(
    shouldTour({ search: "", toured: false, automated: true }),
    null,
  );
  assert.equal(
    shouldTour({ search: "?tour=3", toured: true, automated: true }),
    2,
  );
  assert.deepEqual(GLOSSARY.map(([w]) => w).slice(0, 8), [
    "Stack",
    "App",
    "Deploy",
    "Update",
    "Back up / snapshot",
    "Restore",
    "Secret",
    "Job",
  ]);
});

// redesign-openpoints-2: a hub app's Update… (an app on a moving tag) opens
// the flow with that one app picked: the pull row pulls only that app.
test("redesign-openpoints-2: one unpinned app picked: the pull row is that app's alone, picked alone, and its request names it", () => {
  const s = /** @type {any} */ ({ all: false, stack: "kp-soft", app: "cache" });
  const items = itemsFor(STALE, s);
  const pull = items.find((i) => i.kind === "pull");
  assert.equal(pull?.id, "pull:kp-soft", "the row's Live view id stays");
  assert.equal(pull?.container, "cache");
  assert.deepEqual([...firstChosen(items, s)], ["pull:kp-soft"]);
  assert.equal(flowTitle(items, s), "Update kp-soft/cache");
  assert.deepEqual(
    JSON.parse(updatesBody(pull ? [pull] : [], new Map()).updates),
    [{ stack: "kp-soft", kind: "pull", app: "cache" }],
  );
  // Without an app the pull row is still every app on a moving tag.
  const whole = itemsFor(STALE, { all: false, stack: "kp-soft" });
  assert.deepEqual(
    JSON.parse(
      updatesBody(
        whole.filter((i) => i.kind === "pull"),
        new Map(),
      ).updates,
    ),
    [{ stack: "kp-soft", kind: "pull" }],
  );
});
