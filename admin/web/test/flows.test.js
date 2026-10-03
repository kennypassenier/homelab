// redesign-flows-1/2/3 (redesign 3.71.0, FLOWS.md §1.2 and §3.1; Kenny
// approved 2026-10-03): the Inbox's new sources as rows, the one Update
// flow's pure half, and the Help panel's tour.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  checkRowsOf,
  headline,
  kindOf,
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
  assert.equal(r.title, "3 apps have a newer version");
  assert.equal(kindOf(r), "update");
  assert.match(r.why, /beta-demo\/api v2\.3\.0 → v3\.0\.0 \(major\)/);
  assert.match(r.why, /kp-soft\/demo-agent 0\.1\.4 → 0\.2\.0(?! \(major\))/);
  assert.deepEqual(r.stacks, ["beta-demo", "kp-soft"]);
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

test("redesign-flows-2: a manual check is a row while open or failing, never once it passes or is accepted", () => {
  const rows = checkRowsOf({
    now: 100,
    checks: [
      {
        id: "ups",
        record: {
          stack: "gateway",
          app: "ups",
          text: "Does the UPS self-test pass?",
          ok: null,
          registered_at: 10,
        },
      },
      {
        id: "fan",
        record: {
          stack: "nas",
          app: "fan",
          text: "Fan quiet?",
          ok: false,
          answered_at: 50,
        },
      },
      { id: "ok", record: { stack: "nas", app: "x", text: "Fine?", ok: true } },
      {
        id: "acc",
        record: {
          stack: "nas",
          app: "y",
          text: "Later?",
          ok: false,
          accepted_until: 500,
        },
      },
    ],
  });
  assert.deepEqual(
    rows.map((r) => [r.check, r.severity, r.title]),
    [
      ["ups", "warn", "Check: Does the UPS self-test pass?"],
      ["fan", "bad", "A check fails: Fan quiet?"],
    ],
  );
  assert.ok(rows.every((r) => kindOf(r) === "check"));
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

test("redesign-flows-2: plain-click kinds filter the rows (none on: all); the heading counts the urgent ones", () => {
  /** @type {any[]} */
  const rows = [
    { key: "a", severity: "bad", source: "asks", at: 1 },
    { key: "b", severity: "warn", kind: "update", source: "updates", at: 9 },
    { key: "c", severity: "warn", kind: "setup", source: "setup", at: 5 },
  ];
  assert.equal(shownRows(rows, new Set(), "worst").length, 3);
  assert.deepEqual(
    shownRows(rows, new Set(["update", "ask"]), "worst").map((r) => r.key),
    ["a", "b"],
  );
  assert.deepEqual(
    shownRows(rows, new Set(), "newest").map((r) => r.key),
    ["b", "c", "a"],
  );
  assert.equal(headline(rows), "3 things need you · 1 is urgent");
  assert.equal(headline(rows.slice(1, 2)), "1 thing needs you");
});

test("redesign-flows-1: the Update flow's address and scope", () => {
  assert.equal(updateHref(), "/inbox?update=all");
  assert.equal(updateHref("kp-soft"), "/inbox?update=kp-soft");
  assert.equal(
    updateHref("beta-demo", "api/api"),
    "/inbox?update=beta-demo&app=api%2Fapi",
  );
  assert.deepEqual(scopeOf("?update=all"), { all: true });
  assert.deepEqual(scopeOf("?update=beta-demo&app=api%2Fapi"), {
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
  const chosen = itemsFor(STALE, { all: true }).slice(0, 2);
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
  assert.equal(safetyNet(chosen).value, "Backup first, health checked");
  const pull = itemsFor(STALE, { all: false, stack: "notes" });
  assert.equal(safetyNet(pull).value, "Backup first, auto roll back");
});

test("redesign-flows-1: the run's rows — back up, commit only for pinned apps, deploy, two verifies", () => {
  const pins = itemsFor(STALE, { all: true });
  assert.deepEqual(
    runRows(pins).map((r) => [r.id, r.step]),
    [
      ["backup", 3],
      ["commit", 4],
      ["deploy", 4],
      ["health", 5],
      ["logs", 5],
    ],
  );
  assert.equal(runRows(pins)[0].title, "Back up beta-demo, kp-soft");
  const pull = itemsFor(STALE, { all: false, stack: "notes" });
  assert.ok(!runRows(pull).some((r) => r.id === "commit"));
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
    /data-area="more"/,
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
