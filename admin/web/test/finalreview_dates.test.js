// redesign-final X4 (3.71.0's final review, 2026-10-04): one date format on
// every page — Kenny's rule (2026-10-02, REGISTER fix-216): dd/mm/yyyy
// HH:MM, 24-hour, Europe/Brussels — from one place, js/format.js
// (formatDateTime, formatDay, formatClock). This guard refuses a date or
// clock formatter anywhere else, so a page cannot drift back to its own
// (the locale's order, a weekday or a month's name: "Sat 3 Oct",
// "30 Sep 12:14").
// A formatter that only builds a data key (YYYY-MM-DD, a zone's parts for
// arithmetic) and is never shown says so on the line above: `date-key:`;
// a calendar grid's or a weekday toggle's names say `calendar-words:`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("../js/", import.meta.url).pathname;

/** @param {string} dir @returns {string[]} */
const files = (dir) =>
  readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : f.endsWith(".js") ? [p] : [];
  });

const BANNED = [
  [/new Intl\.DateTimeFormat\(/, "its own Intl.DateTimeFormat"],
  [/\.toLocale(Date|Time)String\(/, "toLocaleDateString/TimeString"],
  [/\.getDay\(\)/, "a hand-built weekday"],
  [/\.get(UTC)?Hours\(\)/, "a hand-built clock"],
  [/["']Jan["'],?\s*["']?Feb|Jan Feb Mar/, "its own month names"],
  [
    /["']Sun["'],?\s*["']?Mon|Sun Mon Tue|["']Mon["'],\s*["']Tue/,
    "its own day names",
  ],
  [/\$\{[^}]*day[^}]*\}\/\$\{[^}]*month/i, "a numeric dd/mm date"],
];

test("redesign-final X4: no page formats a date or a clock itself; js/format.js is the one place", () => {
  /** @type {string[]} */
  const bad = [];
  for (const p of files(ROOT)) {
    if (p.endsWith("/format.js")) continue;
    const lines = readFileSync(p, "utf8").split("\n");
    lines.forEach((line, i) => {
      if (/^\s*(\/\/|\*)/.test(line)) return;
      for (const [re, what] of BANNED)
        if (/** @type {RegExp} */ (re).test(line)) {
          const above = lines.slice(Math.max(0, i - 2), i).join("\n");
          if (/date-key:|calendar-words:/.test(above)) continue;
          bad.push(`${p.slice(ROOT.length)}:${i + 1}: ${what}`);
        }
    });
  }
  assert.deepEqual(bad, [], "use formatDateTime / formatDay / formatClock");
});
