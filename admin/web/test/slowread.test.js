// slow-reads (Kenny, 2026-09-29, form "Trage pagina's"): the last answer at
// once while one run reads again; a finished run announced on the live
// channel is fetched by id, only by pages that are not already waiting for
// it and only when it answered.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  fetchAnnounced,
  freshRead,
  keepRead,
  keptRead,
  runUrl,
  step,
} from "../js/slowread.js";

/**
 * A fake dashboard: answers per URL, in order.
 * @param {Record<string, any[]>} script
 */
function dashboard(script) {
  /** @type {string[]} */
  const asked = [];
  /** @param {string} url */
  const fetchJson = async (url) => {
    asked.push(url);
    const next = script[url]?.shift();
    if (next === undefined) throw new Error(`nothing scripted for ${url}`);
    return next;
  };
  return { asked, fetchJson };
}

test("slow-reads: the last answer is shown first, then the run's own", async () => {
  const d = dashboard({
    "/data/today": [
      {
        ok: true,
        body: { today: 1, read_at: 100, read_run: 1, refreshing: { run: 2 } },
      },
    ],
    "/data/today?run=2": [
      { ok: true, body: { running: true, run: 2 } },
      { ok: true, body: { today: 2, read_at: 190, read_run: 2 } },
    ],
  });
  /** @type {any[]} */
  const shown = [];
  const r = await freshRead(
    d.fetchJson,
    "/data/today",
    "today",
    undefined,
    (b) => shown.push(b),
  );
  assert.deepEqual(
    shown.map((b) => b.today),
    [1],
  );
  assert.ok(r.ok);
  assert.equal(r.ok && r.body.today, 2);
  assert.deepEqual(d.asked, [
    "/data/today",
    "/data/today?run=2",
    "/data/today?run=2",
  ]);
});

test("slow-reads: with nothing kept yet, the read waits as before", async () => {
  const d = dashboard({
    "/data/doctor": [{ ok: true, body: { running: true, run: 7 } }],
    "/data/doctor?run=7": [{ ok: true, body: { report: {}, read_run: 7 } }],
  });
  let last = 0;
  const r = await freshRead(
    d.fetchJson,
    "/data/doctor",
    "doctor",
    undefined,
    () => last++,
  );
  assert.equal(last, 0);
  assert.equal(r.ok && r.body.read_run, 7);
});

test("slow-reads: a failure ends the read and is returned", async () => {
  const d = dashboard({
    "/data/today": [{ ok: false, error: { what: "today" } }],
  });
  const r = await freshRead(d.fetchJson, "/data/today", "today");
  assert.equal(r.ok, false);
  assert.deepEqual(step(r), { kind: "done" });
});

test("slow-reads: an announced run is fetched only when it is new and answered", () => {
  const idle = { shown: 3, reading: false };
  assert.equal(
    fetchAnnounced({ read: "today", run: 4, ok: true }, "today", idle),
    true,
  );
  assert.equal(
    fetchAnnounced({ read: "today", run: 3, ok: true }, "today", idle),
    false,
  );
  assert.equal(
    fetchAnnounced({ read: "doctor", run: 4, ok: true }, "today", idle),
    false,
  );
  assert.equal(
    fetchAnnounced({ read: "today", run: 4, ok: false }, "today", idle),
    false,
    "a failed run does not replace another page's answer",
  );
  assert.equal(
    fetchAnnounced({ read: "today", run: 4, ok: true }, "today", {
      shown: 3,
      reading: true,
    }),
    false,
    "a page waiting for the run gets it itself",
  );
  assert.equal(fetchAnnounced(null, "today", idle), false);
});

test("slow-reads: urls and the tab's own memory", () => {
  assert.equal(runUrl("/data/today", 5), "/data/today?run=5");
  assert.equal(runUrl("/data/x?a=1", 5), "/data/x?a=1&run=5");
  assert.equal(keptRead("/data/nothing"), undefined);
  keepRead("/data/today", { today: 9 });
  assert.deepEqual(keptRead("/data/today"), { today: 9 });
});
