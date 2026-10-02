// Kenny, 2026-10-02: a click anywhere in an expandable row toggles it,
// never a click on a control of the row's own.
import { test } from "node:test";
import assert from "node:assert/strict";
import { kpHandlesRowClicks, rowToggleFor } from "../js/rowtoggle.js";

/**
 * A minimal element: `closest` answers by the selectors it is "inside".
 * @param {string[]} inside
 * @param {object | null} [toggle]
 * @returns {Element}
 */
function el(inside, toggle = null) {
  return /** @type {Element} */ (
    /** @type {unknown} */ ({
      closest: (/** @type {string} */ sel) => {
        if (sel.includes("tbody tr"))
          return inside.includes("row")
            ? { querySelector: () => toggle }
            : null;
        return inside.includes("control") ? {} : null;
      },
    })
  );
}

test("rowtoggle: a click on a plain cell of an expandable row presses its toggle", () => {
  const toggle = { id: "t" };
  assert.equal(rowToggleFor(el(["row"], toggle)), toggle);
});

test("rowtoggle: a click on a button, link or field in the row is left alone", () => {
  assert.equal(rowToggleFor(el(["row", "control"], { id: "t" })), null);
});

test("rowtoggle: a click outside an expandable table's rows does nothing", () => {
  assert.equal(rowToggleFor(el([])), null);
  assert.equal(rowToggleFor(null), null);
});

test("rowtoggle: stands down once kp-themes does it itself (8.1.1 and later)", () => {
  assert.equal(kpHandlesRowClicks('"8.1.0"'), false);
  assert.equal(kpHandlesRowClicks(""), false);
  assert.equal(kpHandlesRowClicks('"8.1.1"'), true);
  assert.equal(kpHandlesRowClicks("8.2.0"), true);
  assert.equal(kpHandlesRowClicks("9.0.0"), true);
});
