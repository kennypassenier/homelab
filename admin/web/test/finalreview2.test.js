// redesign-final round 2 (3.71.0's final review, coordinator 2026-10-04):
// the pure halves of M1–M9, the Low items and X1–X7, each pinned so it
// cannot drift back.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import { existsSync, readFileSync } from "node:fs";
import { appliesTo } from "../js/commands.js";
import { fieldAsClick, resolve } from "../js/drivable.js";
import "../js/pages/notifications.js";
import "../js/pages/restore.js";

const web = (/** @type {string} */ p) => new URL(`../${p}`, import.meta.url);

test("redesign-final M3: the palette offers Park only for a stack that is not parked and Unpark only for a parked one", () => {
  const running = { name: "a", enabled: true };
  const parked = { name: "b", enabled: false };
  assert.equal(appliesTo("disable", running), true);
  assert.equal(appliesTo("enable", running), false);
  assert.equal(appliesTo("disable", parked), false);
  assert.equal(appliesTo("enable", parked), true);
  assert.equal(appliesTo("backup", parked), true);
  assert.equal(appliesTo("restore-native", running), false);
});

test("redesign-final: a removed select's old field id still answers ui pick / ui type, by pressing the control it became", () => {
  for (const [old, now, value] of [
    ["notify-snooze-minutes", "snooze-for", "240"],
    ["bk-restore-stack", "restore-stack", "kp-soft"],
    ["bk-restore-app", "restore-app", "jobtracker"],
    ["bk-restore-snapshot", "restore-night", "abc123"],
  ]) {
    assert.deepEqual(
      fieldAsClick(old, value),
      { control: now, row: value },
      `ui pick ${old} ${value}`,
    );
    assert.equal(resolve(old)?.control.id, now);
  }
  // A control's own id, or a name nothing had, is not a field alias.
  assert.equal(fieldAsClick("snooze-for", "60"), null);
  assert.equal(fieldAsClick("no-such-field", "x"), null);
});

test("redesign-final: the generic layout audit excuses nothing (AUDIT_KNOWN is gone)", () => {
  const e2e = readFileSync(web("test-e2e/invariants.e2e.js"), "utf8");
  assert.doesNotMatch(e2e, /const AUDIT_KNOWN\b/);
  assert.match(e2e, /const found = \(await auditSweep\(\)\)\[cls\];/);
});

test("redesign-final: every module in js/ is reached from main.js; one no page imports is deleted (pages/doctor.js was)", () => {
  const { readdirSync, statSync } = /** @type {typeof import("node:fs")} */ (
    fs
  );
  const root = new URL("../js/", import.meta.url).pathname;
  /** @param {string} d @returns {string[]} */
  const walk = (d) =>
    readdirSync(d).flatMap((f) =>
      statSync(`${d}/${f}`).isDirectory()
        ? walk(`${d}/${f}`)
        : f.endsWith(".js")
          ? [`${d}/${f}`]
          : [],
    );
  const re =
    /(?:^|\n)\s*(?:import|export)\s[^;]*?from\s+["'](\.{1,2}\/[^"']+)["']|(?:^|\n)\s*import\s+["'](\.{1,2}\/[^"']+)["']|import\(\s*["'](\.{1,2}\/[^"']+)["']\s*\)/g;
  const seen = new Set();
  const todo = [`${root}main.js`];
  while (todo.length) {
    const f = /** @type {string} */ (todo.pop());
    if (seen.has(f)) continue;
    seen.add(f);
    for (const m of readFileSync(f, "utf8").matchAll(re)) {
      const rel = m[1] ?? m[2] ?? m[3];
      todo.push(new URL(rel, `file://${f}`).pathname);
    }
  }
  const dead = walk(root.replace(/\/$/, ""))
    .map((f) => f.replace(/\/\//g, "/"))
    .filter((f) => !seen.has(f))
    .map((f) => f.slice(root.length));
  assert.deepEqual(dead, [], "modules nothing imports: delete them");
  assert.equal(existsSync(web("js/pages/doctor.js")), false);
});
