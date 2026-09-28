// feat-overview-8: a table's search and filters live in the address.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  choice,
  replaceTableParams,
  setParams,
  tableFromParams,
  tableToParams,
} from "../js/urlstate.js";

test("a table's view goes into the query string and comes back the same", () => {
  const view = {
    query: "media",
    filters: {
      2: ["running", "degraded"],
      5: { from: "1", to: "8" },
      7: { to: "3" },
    },
  };
  const entries = tableToParams("fleet", view);
  assert.deepEqual(entries, [
    ["fleet.q", "media"],
    ["fleet.f2", "running"],
    ["fleet.f2", "degraded"],
    ["fleet.f5", "~1..8"],
    ["fleet.f7", "~..3"],
  ]);
  const search = replaceTableParams("?days=3", "fleet", entries);
  assert.deepEqual(tableFromParams("fleet", new URLSearchParams(search)), view);
  assert.ok(search.startsWith("?days=3&"));
});

test("replacing one table's keys keeps every other key", () => {
  const s = replaceTableParams(
    "?fleet.q=x&fleet.f2=a&logs.q=keep&app=sonarr",
    "fleet",
    [],
  );
  assert.equal(s, "?logs.q=keep&app=sonarr");
  assert.equal(replaceTableParams("?fleet.q=x", "fleet", []), "");
  // An empty filter writes nothing.
  assert.deepEqual(tableToParams("t", { query: "", filters: { 1: {} } }), []);
  // A key of another table with the same prefix start is not taken.
  assert.deepEqual(
    tableFromParams("t", new URLSearchParams("tx.q=1&t.fz=2&t.f-1=3")),
    { query: "", filters: {} },
  );
});

test("setParams and choice read and write single keys", () => {
  assert.equal(setParams("?a=1&b=2", { a: null, c: "3" }), "?b=2&c=3");
  assert.equal(setParams("?a=1", { a: "" }), "");
  const p = new URLSearchParams("days=3&x=9");
  assert.equal(choice(p, "days", ["1", "3", "7"], "7"), "3");
  assert.equal(choice(p, "x", ["1", "3", "7"], "7"), "7");
  assert.equal(choice(p, "none", ["1"], "1"), "1");
});
