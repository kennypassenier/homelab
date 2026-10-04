#!/usr/bin/env node
// redesign-final (B, coordinator 2026-10-04): a test, a demo fixture or a
// harness that reads the real clock depends on the time of day it runs at
// (two Activity cases went red after midnight UTC, when the demo's "last
// night" had not happened yet), and a date literal later than today in a
// test expires on its day (JobTracker's check-test-dates.mjs, a27d48d).
// This refuses both, in the dashboard's tests and harness and in the Rust
// tests and the demo host:
//   - JS: `Date.now()`, `new Date()` without arguments, `performance.timeOrigin`;
//   - Rust: `SystemTime::now()`, `Utc::now()`, `Local::now()`;
//   - a date literal (YYYY-MM-DD) after today.
// The only allowed source is the injected clock, in one helper per side:
// admin/web/test-e2e/clock.js (the harness and the browser), the unit tests'
// admin/web/test/support/clock.js, the Rust tests' core/tests/support/clock.rs
// and the dashboard's admin/src/shell/clock.rs. A file that takes its time
// from a fixed clock (it imports the helper) may name dates around it.
//
//   node scripts/check-test-clock.mjs     exit 1 and list each one found
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "../../..");

/** The clock helpers: the one place each side reads a clock. */
export const HELPERS = [
  "admin/web/test-e2e/clock.js",
  "admin/web/test/support/clock.js",
  "core/tests/support/clock.rs",
  "admin/src/shell/clock.rs",
];

const JS_DIRS = ["admin/web/test", "admin/web/test-e2e"];
const JS_FILES = ["scripts/invariants-run.sh"];
const RS_DIRS = ["core/tests", "admin/tests", "client/tests", "host/tests"];
const RS_FILES = ["admin/src/shell/demo.rs"];

const JS_CLOCK =
  /\bDate\.now\s*\(\s*\)|\bnew Date\s*\(\s*\)|\bperformance\.timeOrigin\b/;
const RS_CLOCK = /\b(?:SystemTime|Utc|Local)::now\s*\(\s*\)/;
const DATE = /(?<!\d)(20\d{2})-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])(?!\d)/g;

/** @param {string} dir @param {RegExp} ext @returns {string[]} */
const walk = (dir, ext) =>
  !existsSync(dir)
    ? []
    : readdirSync(dir).flatMap((n) => {
        const p = join(dir, n);
        if (statSync(p).isDirectory())
          return n === "node_modules" || n === "fixtures" ? [] : walk(p, ext);
        return ext.test(n) ? [p] : [];
      });

/**
 * Every real-clock read and future date in the files a test runs.
 * @param {{today?: string}} [opts] today as YYYY-MM-DD (the local day)
 * @returns {string[]}
 */
export function scan(opts = {}) {
  // The local calendar day: the checker itself is no test.
  const d = new Date(); // check-test-clock: the checker's own today
  const today =
    opts.today ??
    [d.getFullYear(), d.getMonth() + 1, d.getDate()]
      .map((n) => String(n).padStart(2, "0"))
      .join("-");
  const files = [
    ...JS_DIRS.flatMap((x) => walk(join(ROOT, x), /\.(m?js|json)$/)),
    ...JS_FILES.map((x) => join(ROOT, x)),
    ...RS_DIRS.flatMap((x) => walk(join(ROOT, x), /\.rs$/)),
    ...RS_FILES.map((x) => join(ROOT, x)),
  ].filter((f) => existsSync(f));
  /** @type {string[]} */
  const out = [];
  for (const f of files) {
    const rel = relative(ROOT, f);
    if (HELPERS.includes(rel)) continue;
    const src = readFileSync(f, "utf8");
    const rs = rel.endsWith(".rs");
    const fixed = rs
      ? /support::clock|mod support/.test(src) && /clock::/.test(src)
      : /(?:test|support)\/clock\.js|\.\/clock\.js|support\/clock\.js/.test(
          src,
        );
    src.split("\n").forEach((line, i) => {
      if ((rs ? RS_CLOCK : JS_CLOCK).test(line))
        out.push(`${rel}:${i + 1}: reads the real clock`);
      if (!fixed)
        for (const m of line.matchAll(DATE))
          if (m[0] > today)
            out.push(`${rel}:${i + 1}: ${m[0]} lies after today (${today})`);
    });
  }
  return out;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const found = scan();
  if (found.length) {
    console.error(
      `test clock: ${found.length} real-clock read(s) or future date(s) in tests, the harness or the demo host; take the time from the injected clock (${HELPERS.join(", ")}):\n  ${found.join("\n  ")}`,
    );
    process.exit(1);
  }
  console.log("test clock ok: no real clock and no future date in tests");
}
