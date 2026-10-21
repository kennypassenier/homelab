// fix-179 (1): a stack whose backup read failed must never vanish from the
// Backups page or pass for "keeps no backup". The per-stack failed list that
// perstack.js once held now lives in backupsview.js (stackWhy, repoGroups,
// kpis); this pins it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { at } from "./support/clock.js";

process.env.TZ = "Europe/Brussels";

const { kpis, nightsNow, repoGroups, stackWhy } =
  await import("../js/backupsview.js");

/** @type {import("../js/backupsview.js").StackRead} */
const failed = { status: "failed", reason: "ssh timed out" };
/** @type {import("../js/backupsview.js").StackRead} */
const none = {
  status: "ok",
  noBackup: true,
  times: [],
  native: false,
  repos: [],
};

test("fix-179: a failed stack stays listed with its reason and is not read as 'keeps no data'", () => {
  /** @type {import("../js/backupsview.js").TableView} */
  const view = {
    stacks: new Set(),
    q: "",
    undrilled: false,
    sort: [],
    collapsed: new Set(),
    open: null,
  };
  const groups = repoGroups(
    { b: none, a: failed },
    view,
    at("2026-10-02T12:00:00+02:00"),
  );
  assert.deepEqual(
    groups.map((g) => [g.stack, g.read.status]),
    [
      ["a", "failed"],
      ["b", "ok"],
    ],
  );
  assert.equal(stackWhy(failed), "could not read it: ssh timed out");
  assert.notEqual(stackWhy(failed), stackWhy(none));
});

test("fix-179: failed reads are settled but never counted as expected backups", () => {
  const now = at("2026-10-02T12:00:00+02:00");
  const k = kpis({ a: failed }, now, null);
  assert.equal(k.settled, true);
  assert.equal(k.lastNight.expected, 0);
  assert.equal(k.lastNight.night, nightsNow(now).last);
});
