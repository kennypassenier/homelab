// redesign-console (3.71.0, the approved Console demo): the host-log
// explorer Console and Activity's Host log share — sources turned on and
// off with a plain click, "only" one source, a minimum level, text, the
// side column's exact counts, and the filter's place in the address.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  counts,
  lineShown,
  logFilterFromParams,
  logFilterToParams,
  mergeLines,
  narrowed,
  sourceList,
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
  levels: new Set(["info", "warning", "critical"]),
  q: "",
});

test("redesign-console: sources turned off one by one, or one alone", () => {
  const srcs = sourceList(["notes", "gateway"], ["HOST", "gateway"]);
  assert.deepEqual(srcs, ["HOST", "gateway", "notes"], "the host first");
  // The facet rows hand over the sources that are off; several may be.
  assert.deepEqual(shown({ ...ALL(), off: new Set(["gateway"]) }), [1, 3]);
  assert.deepEqual(shown({ ...ALL(), off: new Set(["gateway", "HOST"]) }), [3]);
  // "only" one source.
  assert.deepEqual(shown({ ...ALL(), only: "notes" }), [3]);
});

test("redesign-console: level and text narrow the lines", () => {
  assert.deepEqual(shown(ALL()), [1, 2, 3, 4], "debug stays hidden");
  // feat-logs-1: any mix of the four levels, not a minimum.
  assert.deepEqual(shown({ ...ALL(), levels: new Set(["warning"]) }), [2]);
  assert.deepEqual(shown({ ...ALL(), levels: new Set(["critical"]) }), [3]);
  assert.deepEqual(
    shown({ ...ALL(), levels: new Set(["debug", "critical"]) }),
    [3, 5],
  );
  assert.deepEqual(shown({ ...ALL(), q: "RCLONE" }), [3]);
  assert.deepEqual(
    shown({ ...ALL(), q: "gateway" }),
    [2, 4],
    "the source matches too",
  );
  assert.equal(narrowed(ALL()), false);
  assert.equal(narrowed({ ...ALL(), q: "x" }), true);
  assert.equal(narrowed({ ...ALL(), levels: new Set(["info"]) }), true);
});

test("redesign-console: the side column's counts are exact", () => {
  const c = counts(LINES);
  assert.equal(c.bySource.get("gateway"), 2);
  assert.equal(c.bySource.get("HOST"), 1);
  assert.deepEqual(c.byLevel, { debug: 1, info: 2, warning: 1, critical: 1 });
});

test("redesign-console: the filter lives in the address, the old /log?source= included", () => {
  const old = logFilterFromParams(
    new URLSearchParams("source=notes&level=warn"),
  );
  assert.equal(old.only, "notes");
  assert.deepEqual([...old.levels].sort(), ["critical", "warning"]);
  assert.deepEqual(shown(old), [3]);
  const f = logFilterFromParams(
    new URLSearchParams("hide=gateway,HOST&q=r&level=bogus"),
    new Set(["debug", "info"]),
  );
  assert.deepEqual([...f.off].sort(), ["HOST", "gateway"]);
  assert.deepEqual(
    [...f.levels].sort(),
    ["debug", "info"],
    "the remembered levels",
  );
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
  assert.equal(
    counts(LINES).byLevel.info +
      counts(LINES).byLevel.warning +
      counts(LINES).byLevel.critical,
    4,
  );
});
