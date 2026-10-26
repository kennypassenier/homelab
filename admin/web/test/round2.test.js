// The second fix round of 2026-10-24 (REGISTER: selfupdate-job-kept-1 and the
// pure parts of the others). Whole-screen pins live in test-e2e.
import { test } from "node:test";
import assert from "node:assert/strict";
import { install } from "./support/minidom.mjs";

install();
const { goneJobWords } = await import("../js/jobpanel.js");

test("selfupdate-job-kept-1: a job that restarted the dashboard says so and points at History, never at the 200-job limit", () => {
  const w = goneJobWords(7, { restarts_dashboard: true }, null);
  assert.match(w.title, /dashboard restarted while this job ran/);
  assert.match(w.text, /History/);
  assert.doesNotMatch(w.text, /200/);
});

test("selfupdate-job-kept-1: the host's own record of the end is said when it is known", () => {
  assert.match(
    goneJobWords(7, { restarts_dashboard: true }, { ok: true }).text,
    /finished well/,
  );
  assert.match(
    goneJobWords(7, { restarts_dashboard: true }, { ok: false, error: "boom" })
      .text,
    /failed: boom/,
  );
});

test("selfupdate-job-kept-1: any other missing job names the real reasons: the limit and the restart", () => {
  const w = goneJobWords(9, null, null);
  assert.match(w.title, /Job 9/);
  assert.match(w.text, /200 jobs/);
  assert.match(w.text, /restarts/);
});

// progress-order-1 (Kenny, 2026-10-24): the Update flow's step list is in the
// order the job runs, and never shows two rows running.
const { flowRows, runRows } = await import("../js/updateflow.js");

/** @param {string} stack */
const pin = (stack) => ({
  id: `${stack}/api`,
  kind: "pin",
  stack,
  key: "api/api",
  container: "api",
  app: "api",
  from: "1",
  to: "2",
});

test("progress-order-1: the page's rows are the job's rows in run order, the logs check last", () => {
  const chosen = /** @type {any[]} */ ([pin("gateway"), pin("downloader")]);
  const planned = runRows(chosen);
  assert.deepEqual(
    planned.map((r) => r.id),
    [
      "backup:gateway",
      "backup:downloader",
      "commit:gateway",
      "commit:downloader",
      "deploy:gateway",
      "deploy:downloader",
      "health",
      "logs",
    ],
  );
  // The host's job has the same rows but the logs check; the page appends it.
  const flow = {
    rows: planned
      .filter((r) => r.id !== "logs")
      .map((r) => ({ ...r, state: "wait" })),
  };
  assert.deepEqual(
    flowRows(flow, null).map((r) => r.id),
    planned.map((r) => r.id),
  );
});

test("progress-order-1: with the job on its commit, backups are done, one row runs and the rest wait", () => {
  const chosen = /** @type {any[]} */ ([pin("gateway"), pin("downloader")]);
  const states = ["ok", "ok", "run", "wait", "wait", "wait", "wait"];
  const rows = flowRows(
    {
      rows: runRows(chosen)
        .filter((r) => r.id !== "logs")
        .map((r, i) => ({ ...r, state: states[i] })),
    },
    null,
  );
  assert.equal(rows.filter((r) => r.state === "run").length, 1);
  assert.deepEqual(
    rows.map((r) => r.state),
    [...states, "wait"],
  );
  assert.equal(rows.find((r) => r.state === "run")?.id, "commit:gateway");
});

// checks-notice-link-1 and check-turn-off-1.
const { findingsView, FINDINGS_SECTION } =
  await import("../js/findingsview.js");
const { checkRefOf, keepHidden, offWords, switchedRows } =
  await import("../js/editforms.js");
const { route, redirectFor } = await import("../js/router.js");

test("checks-notice-link-1: the notice's old address lands on the findings section, not the plain Inbox", () => {
  for (const from of ["/health?block=checks", "/checks"]) {
    const r = route(from);
    assert.equal(
      redirectFor(r, from.split("?")[1] ? `?${from.split("?")[1]}` : "", {
        stacks: [],
      }),
      `/inbox?section=${FINDINGS_SECTION}`,
      from,
    );
  }
});

test("checks-notice-link-1: the view lists every broken and drift finding with its remedy and folds the noted ones apart", () => {
  const v = findingsView({
    passes: 40,
    findings: [
      { severity: "Drift", subject: "b/x", what: "w1", remedy: "r1" },
      { severity: "Noted", subject: "c/y", what: "w2", remedy: "r2" },
      { severity: "Broken", subject: "a/z", what: "w3", remedy: "r3" },
      { severity: "Drift", subject: "a/y", what: "w4", remedy: "r4" },
    ],
  });
  assert.deepEqual(
    v.broken.map((r) => r.subject),
    ["a/z"],
  );
  assert.deepEqual(
    v.drift.map((r) => r.subject),
    ["a/y", "b/x"],
  );
  assert.deepEqual(
    v.noted.map((r) => r.subject),
    ["c/y"],
  );
  assert.equal(v.summary, "1 broken · 2 drift · 1 noted");
  assert.ok([...v.broken, ...v.drift].every((r) => r.remedy));
});

test("check-turn-off-1: a probe finding names its check, and only that row is switched off", () => {
  assert.deepEqual(
    checkRefOf('media/jellyfin: films in the library: reads "3", healthy is 5'),
    { stack: "media", app: "jellyfin", name: "films in the library" },
  );
  assert.equal(checkRefOf("host: the clock is off"), null);
  const view = {
    checks: [{ name: "a" }],
    probes: [{ name: "p", id: "p1" }, { name: "q" }],
  };
  const off = switchedRows(view, "p", "retired app");
  assert.equal(off?.list, "probes");
  assert.deepEqual(
    off?.rows.map((r) => r.row),
    [{ name: "p", id: "p1", off: true }, { name: "q" }],
  );
  const on = switchedRows({ probes: [{ name: "p", off: "x" }] }, "p", null);
  assert.deepEqual(on?.rows[0].row, { name: "p" });
  assert.equal(switchedRows(view, "nope", "x"), null);
});

test("check-turn-off-1: an edited row keeps its reason and a list shows it as off", () => {
  assert.deepEqual(
    keepHidden({ name: "a", off: "why", id: "z" }, { name: "b" }),
    { name: "b", off: "why", id: "z" },
  );
  assert.equal(offWords({ off: "why" }), "Off: why");
  assert.equal(offWords({}), "On");
});
