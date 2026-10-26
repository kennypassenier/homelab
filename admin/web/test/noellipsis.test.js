// buttons-no-ellipsis-1 (Kenny, 2026-10-24): "Snooze... wordt Snooze". No
// button or menu label ends in an ellipsis; a label names the action and
// nothing more. What may still end in "…" is a wait ("Reading the notices…")
// or a field label that continues in the field ("No backup, because…").
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const JS = new URL("../js/", import.meta.url).pathname;

/** @param {string} dir @returns {string[]} */
function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : p.endsWith(".js") ? [p] : [];
  });
}

const ALLOWED = new Set([
  "Search or do anything…",
  "Management network open, because…",
  "No backup, because…",
  "Choose…",
]);

test("buttons-no-ellipsis-1: no capitalised label ends in an ellipsis unless it is a wait or a lead-in", () => {
  const bad = [];
  for (const f of files(JS)) {
    const src = readFileSync(f, "utf8");
    for (const m of src.matchAll(/(["'])([A-Z][^"'\n`]*?)(…|\.\.\.)\1/g)) {
      const text = `${m[2]}${m[3]}`;
      const first = m[2].split(/\s/)[0];
      if (first.endsWith("ing") || ALLOWED.has(text)) continue;
      bad.push(`${f.slice(JS.length)}: ${text}`);
    }
  }
  assert.deepEqual(bad, []);
});

import { stepStatText } from "../js/jobs.js";

test("run-view-min-max-1: a step says its median, fastest and slowest earlier run, and one run is one run", () => {
  assert.equal(
    stepStatText({ step: "pull", median_s: 5, min_s: 3, max_s: 9, runs: 4 }),
    "median 5 s, fastest 3 s, slowest 9 s",
  );
  assert.equal(
    stepStatText({ step: "pull", median_s: 5, min_s: 5, max_s: 5, runs: 1 }),
    "5 s in one earlier run",
  );
  assert.equal(stepStatText(undefined), "");
});
