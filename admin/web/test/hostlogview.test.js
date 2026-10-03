// redesign-console (3.71.0, the approved Console demo): the host-log
// explorer Console and Activity's Host log share — sources turned on and
// off with a plain click, "only" one source, a minimum level, text, the
// side column's exact counts, and the filter's place in the address.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  counts,
  countText,
  keyAction,
  lineShown,
  logFilterFromParams,
  logFilterToParams,
  mergeLines,
  narrowed,
  sourceList,
  toggleSource,
} from "../js/hostlogview.js";

/** @returns {import("../js/parity.js").HostLine} */
const line = (
  /** @type {number} */ seq,
  /** @type {string} */ source,
  /** @type {string} */ level,
  /** @type {string} */ msg,
) => ({
  seq,
  ts: 1_790_000_000 + seq,
  level,
  source,
  msg,
  req: null,
  by: null,
});

const LINES = [
  line(1, "HOST", "info", "nightly round: 6 stacks"),
  line(2, "gateway", "warn", "health check slow"),
  line(3, "notes", "error", "rclone timeout"),
  line(4, "gateway", "info", "restic backup done"),
  line(5, "notes", "debug", "rclone retry"),
];
/** @param {import("../js/hostlogview.js").LogFilter} f */
const shown = (f) => LINES.filter((l) => lineShown(l, f)).map((l) => l.seq);
const ALL = () => ({
  off: new Set(),
  only: null,
  level: /** @type {"" | "warn" | "error"} */ (""),
  q: "",
});

test("redesign-console: a plain click turns one source on or off, several may be off", () => {
  const srcs = sourceList(["notes", "gateway"], ["HOST", "gateway"]);
  assert.deepEqual(srcs, ["HOST", "gateway", "notes"], "the host first");
  let f = toggleSource(ALL(), "gateway", srcs);
  assert.deepEqual(shown(f), [1, 3]);
  f = toggleSource(f, "HOST", srcs);
  assert.deepEqual(shown(f), [3]);
  f = toggleSource(f, "gateway", srcs);
  assert.deepEqual(shown(f), [2, 3, 4]);
  // "only" one source, then a click on another adds it back beside it.
  const only = { ...ALL(), only: "notes" };
  assert.deepEqual(shown(only), [3]);
  assert.deepEqual(shown(toggleSource(only, "HOST", srcs)), [1, 3]);
});

test("redesign-console: level and text narrow the lines", () => {
  assert.deepEqual(shown(ALL()), [1, 2, 3, 4], "debug stays hidden");
  assert.deepEqual(shown({ ...ALL(), level: "warn" }), [2, 3]);
  assert.deepEqual(shown({ ...ALL(), level: "error" }), [3]);
  assert.deepEqual(shown({ ...ALL(), q: "RCLONE" }), [3]);
  assert.deepEqual(
    shown({ ...ALL(), q: "gateway" }),
    [2, 4],
    "the source matches too",
  );
  assert.equal(narrowed(ALL()), false);
  assert.equal(narrowed({ ...ALL(), q: "x" }), true);
});

test("redesign-console: the side column's counts are exact", () => {
  const c = counts(LINES);
  assert.equal(c.bySource.get("gateway"), 2);
  assert.equal(c.bySource.get("HOST"), 1);
  assert.deepEqual(c.byLevel, { "": 4, warn: 2, error: 1 });
  assert.equal(countText(2, 5), "2 of 5 lines");
  assert.equal(countText(1, 1), "1 of 1 line");
});

test("redesign-console: the filter lives in the address, the old /log?source= included", () => {
  const old = logFilterFromParams(
    new URLSearchParams("source=notes&level=warn"),
  );
  assert.equal(old.only, "notes");
  assert.equal(old.level, "warn");
  assert.deepEqual(shown(old), [3]);
  const f = logFilterFromParams(
    new URLSearchParams("hide=gateway,HOST&q=r&level=bogus"),
  );
  assert.deepEqual([...f.off].sort(), ["HOST", "gateway"]);
  assert.equal(f.level, "");
  assert.deepEqual(logFilterToParams(f), {
    source: null,
    hide: "HOST,gateway",
    level: null,
    q: "r",
  });
});

// Senior review (redesign-371-activity, finding 1): a live line that
// arrives before the snapshot raised the last seq, and the snapshot's
// older lines were all dropped ("1 of 1 lines" during the nightly round).
test("redesign-console: the snapshot and the live lines merge by number, deduped, sorted and capped", () => {
  const snap = [line(1, "HOST", "info", "a"), line(2, "HOST", "info", "b")];
  const early = [line(3, "HOST", "info", "c"), line(2, "HOST", "info", "b")];
  assert.deepEqual(
    mergeLines(snap, early, 2000).map((l) => l.seq),
    [1, 2, 3],
    "a live line before the snapshot keeps every snapshot line",
  );
  assert.deepEqual(
    mergeLines([line(5, "HOST", "info", "e")], snap, 2000).map((l) => l.seq),
    [1, 2, 5],
    "sorted by number",
  );
  const many = Array.from({ length: 2500 }, (_, i) =>
    line(i + 1, "HOST", "info", String(i)),
  );
  const kept = mergeLines(many, [], 2000);
  assert.equal(kept.length, 2000, "capped at the ring's size");
  assert.equal(kept[0].seq, 501, "the newest lines stay");
});

// Senior review finding 14: "Info and up" hides debug lines.
test("redesign-console: Info and up hides debug lines", () => {
  assert.deepEqual(shown(ALL()), [1, 2, 3, 4]);
  assert.equal(counts(LINES).byLevel[""], 4);
});

// Senior review finding 12: the page keys ignore a focused button or link
// only (the unanchored regex also matched MAIN, LABEL and ASIDE).
test("redesign-console: Space and End act unless a control has focus", () => {
  assert.equal(keyAction(" ", "MAIN", null), "pause");
  assert.equal(keyAction(" ", "LABEL", null), "pause");
  assert.equal(keyAction("End", "DIV", null), "tail");
  assert.equal(keyAction(" ", "BUTTON", null), null);
  assert.equal(keyAction(" ", "A", null), null);
  assert.equal(keyAction(" ", "DIV", "button"), null);
  assert.equal(keyAction("/", "DIV", null), "search");
  assert.equal(keyAction("/", "INPUT", null), null);
  assert.equal(keyAction(" ", "TEXTAREA", null), null);
  assert.equal(keyAction("x", "DIV", null), null);
});
