// What the dashboard's data tables say while they load, when they hold
// nothing and when nothing matches (dom.js puts these words in kp's slots),
// and when an open tab is older than the dashboard serving it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { busyWords, emptyWords } from "../js/tablestate.js";
import { outdatedPage } from "../js/parity.js";

test("a loading table counts, names the usual time and the rows it keeps", () => {
  const first = busyWords({ words: "Asking the host…", seconds: 42.7 });
  assert.equal(first.text, "Asking the host…");
  assert.equal(first.counter, "42 s so far");
  const slow = busyWords({
    words: "Asking the host…",
    seconds: 95,
    expect: 93,
  });
  assert.equal(
    slow.text,
    "Asking the host… It usually takes about 1 min 33 s.",
  );
  assert.equal(slow.counter, "1 min 35 s so far");
  const again = busyWords({
    words: "Reading again…",
    seconds: 3,
    shownFrom: "05:40",
  });
  assert.match(again.text, /Showing the rows from 05:40\.$/);
  assert.equal(busyWords({ words: "x", seconds: -2 }).counter, "0 s so far");
});

test("an empty table says nothing is there, apart from nothing matching", () => {
  const none = emptyWords({ total: 0, query: "", filters: {} }, "No jobs yet.");
  assert.deepEqual(none, { title: "No jobs yet.", body: "", clear: false });
  const search = emptyWords(
    { total: 12, query: "boiler", filters: {} },
    "No jobs yet.",
  );
  assert.equal(search.title, 'No row matches the search "boiler".');
  assert.match(
    search.body,
    /holds 12 rows; clear the search and filters to see them/,
  );
  assert.equal(search.clear, true);
  const both = emptyWords(
    { total: 1, query: "x", filters: { 2: ["broken"], 3: ["a"] } },
    "n",
  );
  assert.equal(both.title, 'No row matches the search "x" and 2 filters.');
  assert.match(both.body, /holds 1 row; .* see it\./);
  assert.equal(
    emptyWords({ total: 3, query: "", filters: { 1: ["a"] } }, "n").title,
    "No row matches a filter.",
  );
});

test("a tab older than the dashboard that serves it says so", () => {
  assert.equal(outdatedPage("3.63.1", "3.63.1"), null);
  assert.equal(outdatedPage(null, "3.63.1"), null);
  assert.equal(outdatedPage("3.63.1", undefined), null);
  assert.equal(
    outdatedPage("3.63.0", "3.63.1"),
    "The dashboard was updated to 3.63.1; this page still runs 3.63.0.",
  );
});
