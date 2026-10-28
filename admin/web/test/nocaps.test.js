// width-fill-check-1 (Kenny, 2026-10-25): inside the 80 % page column no part
// of the app is narrower than the column because of a max-inline-size cap in
// its own css. The caps that are left are on controls (a field, a chip group,
// a badge, a pill, a list item), named here with why; a new one fails this
// test until it is either removed or named.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const CSS = new URL("../css/", import.meta.url).pathname;

/** @param {string} dir @returns {string[]} */
function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory()
      ? files(join(dir, e.name))
      : e.name.endsWith(".css")
        ? [join(dir, e.name)]
        : [],
  );
}

/** The selector of each rule that still caps a width, with what it is. */
const CONTROLS = new Set([
  ".st-remote", // a remote's address field
  ".nr-facets", // a group of facet chips
  "#action-dialog [data-kp-step] > .kp-field > .kp-field__input", // a dialog field
  ".rs-order > li", // one step of the restore order
  ".follow--bar .follow-badge", // a badge
  ".nx-pill", // a pill
]);

test("width-fill-check-1: the app's css caps a width only on a control", () => {
  /** @type {string[]} */
  const found = [];
  for (const f of files(CSS)) {
    const src = readFileSync(f, "utf8");
    for (const m of src.matchAll(
      /([^{}]+)\{[^{}]*?max-(?:inline-size|width)\s*:\s*[0-9.]+(?:ch|rem|px)\b[^{}]*\}/g,
    )) {
      const sel = m[1].trim().split("\n").pop()?.trim() ?? "";
      if (sel.startsWith("@")) continue;
      if (!CONTROLS.has(sel)) found.push(`${f.slice(CSS.length)}: ${sel}`);
    }
  }
  assert.deepEqual(found, []);
});
