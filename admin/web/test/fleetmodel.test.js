// feat-kp11demo-12 (the approved stacks demo): the Stacks page's pure half on
// kp-themes 11 — each stack as an entity card, the groups of the table, the
// hungriest ranking and the nights grid.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  cpuDay,
  entityOf,
  hungryRows,
  needsAttention,
  nightsGrid,
  readsOf,
  stateGroup,
} from "../js/fleetmodel.js";

/**
 * @param {string} name
 * @param {Record<string, any>} [o]
 * @returns {any}
 */
const row = (name, o = {}) => ({
  name,
  vmid: 100,
  state: { label: "running", tone: "ok" },
  appsUp: 2,
  appsTotal: 2,
  restarts: 0,
  ramUsed: 512,
  ramMax: 1024,
  ramPct: 50,
  cpuPct: 4,
  problem: false,
  flags: [],
  action: { kind: "backup" },
  backup: { state: "ok", at: 1, text: "", tone: "ok" },
  ...o,
});
const gb = (/** @type {number | null | undefined} */ mb) =>
  mb == null ? "—" : (mb / 1024).toFixed(1);

test("feat-kp11demo-12: a parked stack is never in Needs attention, and its card says why Deploy is off", () => {
  const parked = row("wiki", {
    state: { label: "parked", tone: "warn" },
    problem: true,
  });
  assert.equal(needsAttention(parked), false);
  assert.equal(stateGroup(parked), "parked");
  const card = entityOf(parked, gb);
  assert.match(card.actions?.[0].disabled ?? "", /unpark it first/);
  assert.equal(card.figures?.[2].value, "off");
});

test("feat-kp11demo-12: a card keeps its RAM meter with the 80 % mark and its tone", () => {
  const hot = entityOf(row("loki", { ramPct: 91, problem: true }), gb);
  assert.equal(hot.meters?.[0].mark, 0.8);
  assert.equal(hot.meters?.[0].tone, "destructive");
  assert.equal(hot.state?.label, "needs attention");
  assert.equal(stateGroup(row("loki", { problem: true })), "attention");
});

test("feat-kp11demo-12: the hungriest stacks are ranked worst first, parked ones left out, one stack on its own when picked", () => {
  const rows = [
    row("a", { ramPct: 40 }),
    row("b", { ramPct: 91 }),
    row("c", { state: { label: "parked", tone: "warn" }, ramPct: 99 }),
  ];
  const all = hungryRows(rows, "ram", new Map(), null, gb);
  assert.deepEqual(
    all.map((r) => r.key),
    ["b", "a"],
  );
  assert.equal(all[0].figure, "91 %");
  assert.deepEqual(
    hungryRows(rows, "ram", new Map(), "a", gb).map((r) => r.key),
    ["a"],
  );
  const cap = new Map([["a", { cpu: 10, mem: null, disk: 77 }]]);
  assert.equal(hungryRows(rows, "disk", cap, null, gb)[0].key, "a");
});

test("feat-kp11demo-12: the nights grid has one column per night, a paused row for a parked stack and a summary of misses", () => {
  const now = Date.UTC(2026, 9, 3, 12) / 1000;
  const rows = [
    row("a"),
    row("w", { state: { label: "parked", tone: "warn" } }),
  ];
  const night = (/** @type {number} */ d) => now - d * 86400 - 6 * 3600;
  const reads = readsOf(
    { stacks: { a: [night(1), night(2), night(4)], w: [] }, no_backup: [] },
    rows,
  );
  const g = nightsGrid(
    rows,
    reads,
    now,
    5,
    { every: 2, narrow: false },
    (k) => k,
  );
  assert.equal(g.columns.length, 5);
  assert.equal(
    g.rows[1].cells.every((c) => c.tone === "paused"),
    true,
  );
  assert.equal(
    g.rows[0].cells.some((c) => c.tone === "bad"),
    true,
  );
  assert.equal(g.summary.cells.length, 5);
});

test("kpadopt-2: a card's trend row is the stack's 24 h CPU, thinned to 48 readings, and absent while unread", () => {
  const points = Array.from({ length: 288 }, (_, i) => [i * 300, i % 10]);
  const body = { stacks: { loki: points } };
  assert.equal(cpuDay(body, "loki").length, 48);
  assert.deepEqual(cpuDay(body, "other"), []);
  assert.deepEqual(cpuDay({ stacks: {} }, "loki"), []);
  const r = row("loki", { cpuPct: 12.34 });
  assert.equal(entityOf(r, (mb) => String(mb)).trend, undefined);
  const card = entityOf(r, (mb) => String(mb), [1, 2, 3]);
  assert.deepEqual(card.trend, [1, 2, 3]);
  assert.equal(card.trendLabel, "CPU 24 h");
  assert.equal(card.trendText, "12.3 %");
});
