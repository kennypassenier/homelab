// redesign-final round 2 (3.71.0's final review, coordinator 2026-10-04):
// the pure halves of M1–M9, the Low items and X1–X7, each pinned so it
// cannot drift back.
import { test } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import { existsSync, readFileSync } from "node:fs";

const web = (/** @type {string} */ p) => new URL(`../${p}`, import.meta.url);

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
