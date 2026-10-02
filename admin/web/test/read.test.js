// Milestone read: the host page, the questions, the logs tab, the stack
// tabs, the timeline and the ticking age (feat-overview-2, feat-ops-2,
// feat-ops-4, feat-stacks-1, feat-ops-7, feat-overview-4).
import { test } from "node:test";
import assert from "node:assert/strict";
import { answerBody, askView, openAsks } from "../js/asks.js";
import { agoText } from "../js/format.js";
import {
  guestRows,
  hostBars,
  hostChecks,
  hostFacts,
  pct,
  versionText,
} from "../js/host.js";
import {
  JOURNAL,
  appChoices,
  levelTone,
  lineTime,
  logRows,
  logSettings,
  logsUrl,
} from "../js/logs.js";
import {
  aboutStack,
  stackChecks,
  stackEntries,
  stackIncidents,
} from "../js/stacktabs.js";
import { GUTTER, packRows, ticks, timelineModel } from "../js/timeline.js";

const UTC = { locale: "en-GB", timeZone: "UTC" };

test("agoText says how old a reading is, in human units", () => {
  assert.equal(agoText("read", 100, 112), "read 12 s ago");
  assert.equal(
    agoText("measured", 100, 100 + 3 * 3600 + 120),
    "measured 3 h 2 min ago",
  );
  assert.equal(agoText("read", 200, 100), "read 0 s ago");
  assert.equal(agoText("read", null, 100), "not read yet");
});

/** @type {import("../js/fleet.js").Fleet} */
const fleet = /** @type {any} */ ({
  measured_at: 1,
  host: {
    name: "pve",
    cpu_pct: 7,
    ram_used_mb: 16384,
    ram_total_mb: 32768,
    disk_pct: 31,
    ram_committed_mb: 40960,
    cores_total: 16,
    load1_x100: 250,
  },
  counts: { stacks: 2, online: 1, parked: 1 },
  stacks: [
    {
      name: "media",
      vmid: 106,
      online: true,
      enabled: true,
      apps_running: 1,
      apps_total: 1,
    },
    {
      name: "drill",
      vmid: 119,
      online: false,
      enabled: false,
      apps_running: 0,
      apps_total: 0,
    },
    {
      name: "kyu",
      vmid: 109,
      online: false,
      enabled: true,
      apps_running: 0,
      apps_total: 1,
    },
  ],
});

test("the host page's facts and bars read in the units a person reads", () => {
  const f = Object.fromEntries(
    hostFacts(fleet, { version: "3.62.1", build: "v3.62.1" }).map((x) => [
      x.label,
      x.value,
    ]),
  );
  assert.equal(f["Host daemon"], "3.62.1");
  assert.equal(f.Cores, "16");
  assert.equal(f["Load (1 min)"], "2.50");
  assert.equal(f["RAM promised to stacks"], "40.0 GB (125% of RAM)");
  assert.equal(f["Stacks online"], "1 of 2");
  assert.equal(f["Up for"], "not reported by the host");
  assert.deepEqual(hostBars(fleet), [
    { label: "CPU", pct: 7, value: "7%" },
    { label: "RAM", pct: 50, value: "16.0 GB of 32.0 GB" },
    { label: "Root disk", pct: 31, value: "31% used" },
  ]);
  assert.equal(
    versionText("3.62.1", "v3.62.1-4-gabc-dirty"),
    "3.62.1 (v3.62.1-4-gabc-dirty)",
  );
  assert.equal(versionText(null, null), "not known yet");
  assert.equal(pct(1, 0), null);
});

// fix-175: host CPU is unknown before the host's status loop has a second
// /proc/stat sample to diff against — never a fabricated "0%".
test("the CPU bar says 'not measured yet' instead of a fabricated 0%", () => {
  const f = { ...fleet, host: { ...fleet.host, cpu_pct: null } };
  assert.deepEqual(hostBars(f)[0], {
    label: "CPU",
    pct: 0,
    value: "not measured yet",
  });
});

test("the containers table names each guest's stack and flags a stopped one", () => {
  const rows = guestRows(
    [
      { vmid: 106, status: "running", lock: "", name: "106-app-media" },
      { vmid: 119, status: "stopped", lock: "", name: "119-drill" },
      { vmid: 109, status: "stopped", lock: "backup", name: "109-app-kyu" },
      { vmid: 998, status: "stopped", lock: "", name: "debian-12" },
    ],
    fleet,
  );
  assert.deepEqual(
    rows.map((r) => [r.vmid, r.status.tone, r.stack, r.lock]),
    [
      [106, "ok", "media", "—"],
      [119, "warn", "drill", "—"], // parked: stopped on purpose
      [109, "bad", "kyu", "backup"],
      [998, "warn", null, "—"],
    ],
  );
  assert.deepEqual(
    hostChecks({
      overall: "ok",
      checks: [
        { name: "host disk", health: "Ok", detail: "52% free" },
        { name: "stack media backup", health: "Ok", detail: "" },
      ],
    }).map((c) => c.name),
    ["host disk"],
  );
});

/** @type {import("../js/asks.js").Ask} */
const ask = {
  id: 3,
  boot: "b1",
  op: "deploy media",
  step: "native units",
  what: "restarted twice",
  if_allowed: "goes on",
  if_stopped: "stops",
  asked_at: 1000,
  deadline: 1120,
};

test("a question shows what each answer does and how long the host waits", () => {
  assert.equal(openAsks([ask], 1119).length, 1);
  assert.equal(openAsks([ask], 1120).length, 0);
  const v = askView(ask, 1020);
  assert.equal(v.title, 'deploy media is waiting at "native units"');
  assert.equal(v.left, "1 min 40 s left to answer");
  assert.equal(v.urgent, false);
  assert.equal(askView(ask, 1100).urgent, true);
  assert.equal(askView(ask, 1200).left, "no longer waiting");
  assert.deepEqual(answerBody(ask, false), {
    id: 3,
    boot: "b1",
    op: "deploy media",
    step: "native units",
    allow: false,
  });
  const older = { ...ask, id: 1, asked_at: 900, deadline: 1500 };
  assert.deepEqual(
    openAsks([ask, older], 1000).map((a) => a.id),
    [1, 3],
  );
});

test("the logs tab reads its settings from the address and asks the server", () => {
  const s = logSettings(
    new URLSearchParams("app=sonarr&since=86400&q=err&follow=1"),
  );
  assert.deepEqual(s, {
    since: "86400",
    app: "sonarr",
    q: "err",
    follow: true,
  });
  assert.deepEqual(logSettings(new URLSearchParams("since=5")), {
    since: "3600",
    app: "",
    q: "",
    follow: false,
  });
  assert.equal(
    logsUrl("media", s, 200),
    "/data/logs?stack=media&since=86400&limit=200&app=sonarr&q=err",
  );
  assert.deepEqual(
    appChoices(["sonarr", "bazarr"]).map((c) => c.value),
    ["", "bazarr", "sonarr", JOURNAL],
  );
  assert.equal(levelTone("error"), "bad");
  assert.equal(levelTone("warning"), "warn");
  assert.equal(levelTone("informational"), "ok");
  assert.equal(levelTone(""), "");
  assert.equal(lineTime(1790606585948, UTC), "28/09 14:43:05");
  const rows = logRows(
    [
      { ts_ms: 1000, source: "a", stream: "stdout", level: "", line: "old" },
      { ts_ms: 2000, source: "", stream: "", level: "error", line: "new" },
    ],
    UTC,
  );
  assert.deepEqual(
    rows.map((r) => [r.line, r.source, r.level, r.tone]),
    [
      ["new", "—", "error", "bad"],
      ["old", "a", "—", ""],
    ],
  );
});

test("a stack's tabs keep only what is about that stack", () => {
  assert.ok(aboutStack("deploy-kp-soft", "kp-soft"));
  assert.ok(aboutStack("deploy media", "media"));
  assert.ok(aboutStack("deploy-media", "media", "deploy"));
  assert.ok(!aboutStack("backup-media", "media", "deploy"));
  assert.ok(!aboutStack("deploy-kp-soft", "soft-x"));
  assert.ok(!aboutStack("self-update", "update"));
  assert.ok(!aboutStack(null, "media"));
  const entries = /** @type {import("../js/activity.js").Entry[]} */ ([
    {
      kind: "op",
      start: 1,
      end: 2,
      label: "deploy",
      subject: "deploy-media",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 1,
      end: 2,
      label: "self-update",
      subject: "self-update",
      ok: true,
      steps: [],
    },
    { kind: "phase", start: 1, end: 2, name: "media", count: 1 },
  ]);
  assert.equal(stackEntries(entries, "media").length, 1);
  assert.deepEqual(
    stackIncidents(
      ["1-deploy-media", "2-backup-kp-soft", "3-zfs-replicate"],
      "media",
    ),
    ["1-deploy-media"],
  );
  const checks = /** @type {import("../js/checks.js").Check[]} */ ([
    {
      id: "a",
      record: { stack: "media", app: "x", text: "?", registered_at: 1 },
    },
    {
      id: "b",
      record: { stack: "kyu", app: "y", text: "?", registered_at: 1 },
    },
  ]);
  assert.deepEqual(
    stackChecks(checks, "media").map((c) => c.id),
    ["a"],
  );
});

test("overlapping operations stack into rows, never more than the cap", () => {
  assert.deepEqual(
    packRows(
      [
        { a: 0, b: 10 },
        { a: 5, b: 8 },
        { a: 9, b: 12 },
        { a: 10, b: 11 },
        { a: 11, b: 20 },
      ],
      2,
    ),
    [0, 1, 1, 0, 0],
  );
  assert.deepEqual(
    packRows(
      [
        { a: 0, b: 9 },
        { a: 1, b: 9 },
        { a: 2, b: 9 },
      ],
      2,
    ),
    [0, 1, 1],
  );
});

test("the axis steps on whole local hours or days", () => {
  const t = ticks(0, 3 * 86400, 800, { ...UTC, offset: () => 0 });
  assert.deepEqual(t.map((x) => [x.t, x.major]).slice(0, 3), [
    [0, true],
    [43200, false],
    [86400, true],
  ]);
  assert.equal(t[0].label, "01/01");
  assert.equal(t[1].label, "12:00");
  // An offset moves the midnight marks to local midnight.
  const b = ticks(0, 86400, 100, { ...UTC, offset: () => 7200 });
  assert.equal(b[0].t, 43200 - 7200);
  assert.equal(b.find((x) => x.major)?.t, 86400 - 7200);
});

test("the timeline puts operations, nightly phases and incidents on one axis", () => {
  const from = 1_790_000_000;
  const to = from + 86400;
  const entries = /** @type {import("../js/activity.js").Entry[]} */ ([
    {
      kind: "op",
      start: from + 3600,
      end: from + 3700,
      label: "deploy",
      subject: "deploy-media",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: from + 3650,
      end: from + 3660,
      label: "deploy",
      subject: "deploy-kyu",
      ok: false,
      steps: [],
    },
    {
      kind: "op",
      start: from + 80000,
      end: 0,
      label: "backup",
      subject: "backup-kyu",
      ok: false,
      steps: [],
    },
    {
      kind: "op",
      start: from - 99999,
      end: from - 99000,
      label: "old",
      ok: true,
      steps: [],
    },
    {
      kind: "phase",
      start: from + 7200,
      end: from + 9000,
      name: "backup",
      count: 12,
    },
  ]);
  const m = timelineModel(
    {
      entries,
      incidents: [`${from + 3660}-deploy-kyu`, "junk", `${from - 5}-old`],
      from,
      to,
      width: 1000,
    },
    { ...UTC, offset: () => 0 },
  );
  assert.deepEqual(m.counts, { ops: 3, phases: 1, incidents: 1 });
  assert.deepEqual(
    m.lanes.map((l) => l.label),
    ["Operations", "Nightly round", "Incidents"],
  );
  const ops = m.marks.filter((k) => k.kind === "op");
  assert.deepEqual(
    ops.map((k) => k.tone),
    ["ok", "bad", "warn"],
  );
  // The two overlapping deploys sit in two rows.
  assert.notEqual(ops[0].y, ops[1].y);
  // A running operation reaches the window's end.
  assert.ok(Math.abs(ops[2].x + ops[2].w - (1000 - 12)) < 0.01);
  assert.ok(ops.every((k) => k.x >= GUTTER && k.w >= 3));
  assert.match(ops[0].title, /deploy-media · ok · .* · 1 min 40 s/);
  // An operation that ended the second it started is done, not running.
  const instant = timelineModel(
    {
      entries: [
        {
          kind: "op",
          start: from + 500,
          end: from + 500,
          label: "self-update",
          ok: true,
          steps: [],
        },
      ],
      incidents: [],
      from,
      to,
      width: 1000,
    },
    { ...UTC, offset: () => 0 },
  ).marks[0];
  assert.equal(instant.tone, "ok");
  assert.match(instant.title, /· 0 s$/);
  const inc = m.marks.find((k) => k.kind === "incident");
  assert.equal(inc?.label, "deploy-kyu");
  assert.ok(m.height > 0 && m.ticks.length > 2);
});
