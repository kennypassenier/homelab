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
  fill,
  nowValue,
  peak,
  poweredOn,
  statusClasses,
  trafficFigures,
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
