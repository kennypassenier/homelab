// root-disk-decimals-1 (Kenny, 2026-10-22): "Who fills the root disk" read
// "81.39999999999999 GB / 96 GB". Every size and figure a person reads is
// rounded to at most 2 decimals; this pins the formatters that do it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { figure, gbText, humanDuration } from "../js/format.js";
import { gb, humanMb } from "../js/fleet.js";
import { humanBytes } from "../js/backupsview.js";
import { ramText } from "../js/presetsview.js";

test("root-disk-decimals-1: figure rounds to at most two decimals and drops trailing zeros", () => {
  assert.equal(figure(81.39999999999999), "81.4");
  assert.equal(figure(96), "96");
  assert.equal(figure(100), "100");
  assert.equal(figure(0.1 + 0.2), "0.3");
  assert.equal(figure(1 / 3), "0.33");
  assert.equal(figure(2 / 3), "0.67");
  assert.equal(figure(12.5), "12.5");
  assert.equal(figure(1000.004), "1000");
  assert.equal(figure(-0.001), "0");
  assert.equal(figure(0), "0");
  assert.equal(figure(3.14159, 3), "3.142");
  assert.equal(figure(null), "—");
  assert.equal(figure(NaN), "—");
  assert.equal(figure(Infinity), "—");
  assert.equal(gbText(81.39999999999999), "81.4 GB");
  assert.equal(gbText(96), "96 GB");
  assert.equal(gbText(undefined), "—");
});

test("root-disk-decimals-1: no formatter lets a float's tail reach the screen, whatever sum it was handed", () => {
  const tail = /\d\.\d{3,}/;
  // Sums the way a bar makes them: parts of a disk added up in floating point.
  const raw = [
    32.5 + 48.9,
    0.1 + 0.2,
    96 - 14.6,
    (3 / 7) * 1024,
    1e21 / 3,
    81.39999999999999,
  ];
  for (const n of raw) {
    for (const [name, text] of /** @type {[string, string][]} */ ([
      ["figure", figure(n)],
      ["gbText", gbText(n)],
      ["gb", gb(n)],
      ["humanMb", humanMb(n)],
      ["humanBytes", String(humanBytes(n) ?? "")],
      ["ramText", ramText(n)],
      ["humanDuration", humanDuration(n)],
    ]))
      assert.doesNotMatch(text, tail, `${name}(${n}) = ${text}`);
  }
});

test("root-disk-decimals-1: the bars hand kp's stack bar a rounding formatter, never a bare interpolation of a sum", () => {
  const dir = new URL("../js/", import.meta.url);
  /** @type {string[]} */
  const bare = [];
  const walk = (/** @type {URL} */ d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      if (e.isDirectory()) walk(new URL(`${e.name}/`, d));
      else if (e.name.endsWith(".js")) {
        const text = readFileSync(new URL(e.name, d), "utf8");
        // `format: (v) => \`${v} GB\`` hands the bar's own float sums to the page unrounded.
        if (/format:\s*\(\w+\)\s*=>\s*`\$\{\w+\}\s*(GB|MB|TB|GiB)/.test(text))
          bare.push(e.name);
      }
    }
  };
  walk(dir);
  assert.deepEqual(bare, [], "a bar formats its figures without rounding");
});
