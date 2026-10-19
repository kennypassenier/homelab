// redesign-371-metrics: the Metrics page's pure half (js/metricsview.js) —
// which chart sits where, the Drives table, the events on every chart and
// the Traffic tab's figures.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  HOST_SECTIONS,
  STACK_SECTIONS,
  annotations,
  byTitle,
  changeText,
  count,
  drives,
  extremes,
  fill,
  healthPeriods,
  heatColumns,
  hostHeat,
  machinesOf,
  nowValue,
  peak,
  poweredOn,
  ribbonRuns,
  statusClasses,
  statusParts,
  trafficFigures,
  usualMachines,
} from "../js/metricsview.js";

/** @param {string} title @param {[string, [number, number][]][]} series */
const panel = (title, series) => ({
  panel: { title, desc: "", unit: "count" },
  series: series.map(([label, points]) => ({ label, points })),
});

test("redesign-371-metrics: the host's charts sit in two sections, Drives last and full width, every card described", () => {
  assert.deepEqual(
    HOST_SECTIONS.map((s) => s.title),
    ["Compute", "Storage and temperature"],
  );
  const cards = HOST_SECTIONS.flatMap((s) => s.cards);
  assert.equal(cards.at(-1)?.kind, "drives");
  assert.equal(cards.at(-1)?.span, 3);
  for (const c of [...cards, ...STACK_SECTIONS.flatMap((s) => s.cards)])
    assert.ok(c.desc.length > 20, `${c.title} has no one-sentence description`);
  // Each row of the 3-column grid is full: no empty track (invariant 31).
  for (const s of [...HOST_SECTIONS, ...STACK_SECTIONS]) {
    const spans = s.cards.reduce((a, c) => a + c.span, 0);
    assert.equal(spans % 3, 0, `${s.title} leaves a track empty`);
  }
  assert.equal(
    fill("Share of all {cores} cores in use.", { cores: 16 }),
    "Share of all 16 cores in use.",
  );
});

test("redesign-371-metrics: a card's figure is the first series' last reading, or the hottest", () => {
  const p = panel("t", [
    [
      "a",
      [
        [1, 40],
        [2, 46],
      ],
    ],
    [
      "b",
      [
        [1, 50],
        [2, 52],
      ],
    ],
  ]);
  assert.equal(nowValue(p, "first"), 46);
  assert.equal(nowValue(p, "max"), 52);
  assert.equal(nowValue(p, null), null);
  assert.deepEqual(
    peak([
      [1, 3],
      [2, 38],
      [3, 7],
    ]),
    { at: 2, value: 38 },
  );
  assert.equal(peak([]), null);
});

test("redesign-371-metrics: Drives names each drive's state and since when it is not ok", () => {
  const P = byTitle([
    panel("Drive health (SMART, 1 = ok)", [
      [
        "sda",
        [
          [1, 1],
          [2, 1],
          [3, 1],
        ],
      ],
      [
        "sdb",
        [
          [1, 1],
          [2, 0],
          [3, 0],
        ],
      ],
      [
        "sdc",
        [
          [1, 0],
          [2, 0],
        ],
      ],
    ]),
    panel("Drive pending sectors", [
      [
        "sdb",
        [
          [1, 7],
          [3, 12],
        ],
      ],
    ]),
    panel("Drive reallocated sectors", [["sdb", [[3, 3]]]]),
    panel("Drive temperature", [["sda", [[3, 34]]]]),
    panel("Drive power-on hours", [["sdb", [[3, 12000]]]]),
  ]);
  const d = drives(P);
  assert.deepEqual(
    d.map((x) => [x.name, x.ok, x.since]),
    [
      ["sda", true, null],
      ["sdb", false, 2],
      ["sdc", false, null],
    ],
  );
  assert.equal(d[1].pending, 12);
  assert.equal(d[1].realloc, 3);
  assert.equal(d[0].temp, 34);
  // The two lines in a drive's detail are the readings over the window.
  assert.deepEqual(d[1].pendPoints, [
    [1, 7],
    [3, 12],
  ]);
  assert.deepEqual(d[0].tempPoints, [[3, 34]]);
  assert.deepEqual(d[2].pendPoints, []);
  assert.equal(poweredOn(12000), "12,000 h · 1.4 y");
  assert.equal(poweredOn(null), "not reported");
});

test("redesign-371-metrics: what the host did becomes event markers, failures red, updates amber, per stack only its own", () => {
  /** @type {any[]} */
  const entries = [
    { kind: "phase", start: 100, end: 200, name: "backup", count: 6 },
    {
      kind: "op",
      start: 300,
      end: 395,
      label: "scheduled-backup",
      subject: "back up notes",
      ok: false,
      error: "repository locked",
      steps: [],
    },
    {
      kind: "op",
      start: 400,
      end: 540,
      label: "deploy",
      subject: "deploy films",
      by: "Kenny",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 600,
      end: 670,
      label: "update",
      subject: "update gateway",
      by: "Kenny",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 5,
      end: 6,
      label: "deploy",
      subject: "too early",
      ok: true,
      steps: [],
    },
  ];
  const a = annotations(entries, { from: 50, to: 1000 });
  assert.deepEqual(
    a.map((x) => [x.at, x.tone]),
    [
      [100, "info"],
      [300, "bad"],
      [400, "info"],
      [600, "warn"],
    ],
  );
  assert.match(a[1].label, /back up notes failed \(repository locked\)/);
  assert.equal(a[2].label, "deploy films (Kenny)");
  // The kind names what it was (kp's key words it); work that took a
  // minute or more is a window (until), a failure stays a line.
  assert.deepEqual(
    a.map((x) => [x.kind, x.until]),
    [
      ["operation", 200],
      ["failure", undefined],
      ["operation", 540],
      ["update", 670],
    ],
  );
  const quick = annotations(
    /** @type {any[]} */ ([
      {
        kind: "op",
        start: 100,
        end: 130,
        label: "deploy",
        subject: "deploy x",
        ok: true,
        steps: [],
      },
      {
        kind: "op",
        start: 200,
        end: 0,
        label: "update",
        subject: "update y",
        ok: false,
        steps: [],
      },
    ]),
    { from: 50, to: 1000 },
  );
  assert.deepEqual(
    quick.map((x) => [x.kind, x.until]),
    [
      ["operation", undefined],
      ["update", undefined],
    ],
    "30 s is a line, and a running operation has no end yet",
  );
  const films = annotations(entries, { from: 50, to: 1000, stack: "films" });
  assert.deepEqual(
    films.map((x) => x.at),
    [400],
  );
});

test("redesign-371-metrics: status codes fold into four classes, with the figures the tiles print", () => {
  const classes = statusClasses([
    {
      label: "200",
      points: [
        [1, 80],
        [2, 90],
      ],
    },
    {
      label: "304",
      points: [
        [1, 5],
        [2, 5],
      ],
    },
    {
      label: "404",
      points: [
        [1, 8],
        [2, 6],
      ],
    },
    {
      label: "500",
      points: [
        [1, 0],
        [2, 6],
      ],
    },
  ]);
  assert.deepEqual(
    classes.map((c) => [c.label, c.n]),
    [
      ["2xx", 170],
      ["3xx", 10],
      ["4xx", 14],
      ["5xx", 6],
    ],
  );
  assert.deepEqual(classes[0].points, [
    [1, 80],
    [2, 90],
  ]);
  const f = trafficFigures({
    classes,
    prevTotal: 160,
    errors: [
      { status: "500", host: "a", path: "/x", n: 6 },
      { status: "404", host: "demo.example.org", path: "/favicon.ico", n: 12 },
    ],
  });
  assert.equal(f.total, 200);
  assert.equal(f.shares["5xx"], 0.03);
  assert.equal(f.change, 25);
  assert.equal(f.spikeAt, 2);
  assert.equal(f.top4xx?.path, "/favicon.ico");
  assert.equal(changeText(25, "24h"), "+25% on the day before");
  assert.equal(changeText(-8, "7d"), "−8% on the week before");
  assert.equal(changeText(null, "24h"), "no earlier window to compare with");
  // Counters are exact, never 38.1k (Kenny, 2026-10-03).
  assert.equal(count(38112), "38,112");
});

test("kp11-metrics: the ribbon lists the same work in lanes, a failure as a failed bar, a running one without an end", () => {
  /** @type {any[]} */
  const entries = [
    { kind: "phase", start: 100, end: 2600, name: "backup", count: 6 },
    {
      kind: "op",
      start: 3000,
      end: 3100,
      label: "update",
      subject: "update gateway",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 4000,
      end: 4005,
      label: "scheduled-backup",
      subject: "back up notes",
      ok: false,
      error: "repository locked",
      steps: [],
    },
    {
      kind: "op",
      start: 5000,
      end: 0,
      label: "deploy",
      subject: "deploy films",
      ok: false,
      steps: [],
    },
  ];
  const runs = ribbonRuns(entries, { from: 0, to: 9000 });
  assert.deepEqual(
    runs.map((r) => [r.lane, r.state]),
    [
      ["rounds", "ok"],
      ["updates", "ok"],
      ["host", "failed"],
      ["host", "running"],
    ],
  );
  // Milliseconds (kp's unit); an instant gets two minutes to be pressed.
  assert.equal(runs[0].start, 100_000);
  assert.equal(runs[2].end, 4_120_000);
  assert.equal(runs[3].end, null);
  assert.match(runs[2].label, /failed \(repository locked\)/);
  assert.deepEqual(
    ribbonRuns(entries, { from: 0, to: 9000, stack: "films" }).map(
      (r) => r.lane,
    ),
    ["host"],
    "a stack's ribbon holds what named that stack",
  );
});

test("kp11-metrics: a key figure rests on its highest and lowest reading", () => {
  assert.equal(extremes([]), null);
  assert.deepEqual(
    extremes([
      [1, 4],
      [2, 9],
      [3, 2],
      [4, 5],
    ]),
    { high: { at: 2, value: 9 }, low: { at: 3, value: 2 } },
  );
});

test("kp11-metrics: the status mix is four parts with exact counts, 4xx warning and 5xx destructive", () => {
  assert.deepEqual(
    statusParts([
      { label: "2xx", n: 170 },
      { label: "3xx", n: 10 },
      { label: "4xx", n: 14 },
      { label: "5xx", n: 6 },
    ]),
    [
      { key: "2xx", label: "2xx", value: 170 },
      { key: "3xx", label: "3xx", value: 10 },
      { key: "4xx", label: "4xx", value: 14, tone: "warning" },
      { key: "5xx", label: "5xx", value: 6, tone: "destructive" },
    ],
  );
});

test("kp11-metrics: the heat grid cuts any window into about two dozen cells and sums the requests in each", () => {
  assert.deepEqual(heatColumns(3600), { bucket: 300, every: 3, unit: "clock" });
  assert.deepEqual(heatColumns(86400), {
    bucket: 3600,
    every: 3,
    unit: "clock",
  });
  assert.deepEqual(heatColumns(30 * 86400), {
    bucket: 86400,
    every: 5,
    unit: "day",
  });
  const to = 1_000_000;
  const from = to - 86400;
  const h = hostHeat(
    [
      {
        label: "a.example",
        points: [
          [to - 100, 7],
          [to - 200, 3],
          [to - 4000, 5],
          [from - 10, 99],
        ],
      },
    ],
    from,
    to,
  );
  assert.equal(h.columns.length, 24);
  assert.equal(h.columns[23].at, to - 3600);
  // The newest cell is whole: the two readings inside the last hour add up,
  // the one an hour and more back lands in the cell before, the one before
  // the window is left out.
  assert.equal(h.rows[0].counts[23], 10);
  assert.equal(h.rows[0].counts[22], 5);
  assert.equal(
    h.rows[0].counts.reduce((a, b) => a + b, 0),
    15,
  );
});

test("kp11-metrics: the picker lists the host and the three heaviest stacks first; the address holds at most three machines", () => {
  assert.deepEqual(
    usualMachines([
      { name: "a", ram_used_mb: 100 },
      { name: "b", ram_used_mb: 900 },
      { name: "c", ram_used_mb: 500 },
      { name: "d", ram_used_mb: 700 },
      { name: "e" },
    ]),
    ["host", "b", "d", "c"],
  );
  assert.deepEqual(machinesOf(null), ["host"]);
  assert.deepEqual(machinesOf(""), ["host"]);
  assert.deepEqual(machinesOf("kyu"), ["kyu"]);
  assert.deepEqual(machinesOf("host,kyu,kyu, almanac,inbox"), [
    "host",
    "kyu",
    "almanac",
  ]);
});

test("kp11-metrics: a drive's health strip cuts the window into cells: down when SMART failed, degraded when sectors are pending", () => {
  const day = 86400;
  const t0 = 1_760_000_000 - (1_760_000_000 % day);
  const d = {
    name: "sdb",
    healthPoints: /** @type {[number, number][]} */ ([
      [t0 + 100, 1],
      [t0 + day + 100, 1],
      [t0 + day + 200, 0],
      [t0 + 3 * day + 100, 1],
    ]),
    pendPoints: /** @type {[number, number][]} */ ([[t0 + 3 * day + 100, 4]]),
  };
  const p = healthPeriods(d);
  assert.deepEqual(
    p.map((x) => x.state),
    ["up", "down", "none", "degraded"],
  );
  assert.equal(p[1].incidents, 1);
  assert.equal(p[1].uptime, 0.5);
  assert.deepEqual(healthPeriods({ ...d, healthPoints: [] }), []);
  // A short window is cut in hours.
  const hourly = healthPeriods({
    name: "sda",
    healthPoints: [
      [t0, 1],
      [t0 + 10800, 1],
    ],
    pendPoints: [],
  });
  assert.equal(hourly.length, 4);
});
