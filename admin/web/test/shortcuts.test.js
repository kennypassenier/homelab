// feat-overview-8, feat-kpadopt-2: the keys. kp-themes' engine handles the
// chords and fills the help sheet; what is ours is where each letter goes,
// the stack hub's tab keys, and that no key needs AltGr.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  GO,
  altDigit,
  appShortcuts,
  goTarget,
  tabTarget,
} from "../js/shortcuts.js";
import { route } from "../js/router.js";
import { ALTGR_KEYS } from "/static/kp/js/shortcuts.js";

test("g then a letter names a page; an unknown letter names none", () => {
  assert.equal(goTarget("h"), "/host");
  // feat-shell-1: the areas' own letters (g i = the Inbox).
  assert.equal(goTarget("i"), "/inbox");
  assert.equal(goTarget("p"), "/activity?view=planned");
  assert.equal(goTarget("z"), null);
});

test("a stack's tabs open by number and by t p / t n", () => {
  // feat-shell-3: the hub's tabs, Overview · Logs · Apps · Backups ·
  // History · Settings.
  const logs = route("/stacks/media/logs");
  assert.equal(tabTarget(logs, { n: 1 }), "/stacks/media");
  assert.equal(tabTarget(logs, { n: 6 }), "/stacks/media/settings");
  assert.equal(tabTarget(logs, { n: 2 }), null); // already there
  assert.equal(tabTarget(logs, { n: 9 }), null);
  assert.equal(tabTarget(logs, { step: -1 }), "/stacks/media");
  assert.equal(tabTarget(logs, { step: 1 }), "/stacks/media/apps");
  const last = route("/stacks/media/settings");
  assert.equal(tabTarget(last, { step: 1 }), null);
  // Tab keys mean nothing off a stack's page.
  assert.equal(tabTarget(route("/"), { n: 1 }), null);
});

test("altDigit: Alt+digit by the physical key; AltGr, Ctrl and a bare digit are never ours", () => {
  const e = (/** @type {Record<string, unknown>} */ o) => ({
    key: "",
    code: "",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...o,
  });
  // On AZERTY the unshifted digit row types "&": the code still says Digit1.
  assert.equal(altDigit(e({ key: "&", code: "Digit1", altKey: true })), 1);
  assert.equal(altDigit(e({ key: "3", code: "Digit3", altKey: true })), 3);
  // AltGr reports Ctrl+Alt: a press that needs it is not ours.
  assert.equal(
    altDigit(e({ key: "[", code: "Digit5", altKey: true, ctrlKey: true })),
    null,
  );
  assert.equal(altDigit(e({ key: "1", code: "Digit1" })), null);
  assert.equal(altDigit(e({ key: "x", code: "KeyX", altKey: true })), null);
});

test("the list handles every go-to key, none needs AltGr, and tab keys are dimmed off a stack", () => {
  /** @type {string[]} */
  const went = [];
  let at = route("/");
  const list = appShortcuts({
    navigate: (href) => went.push(href),
    route: () => at,
    focusSearch: () => went.push("search"),
  });
  const keys = list.map((s) => s.keys);
  for (const k of Object.keys(GO)) assert.ok(keys.includes(`g ${k}`), k);
  assert.ok(keys.includes("/"));
  for (const s of list)
    for (const step of s.keys.split(" "))
      assert.ok(!ALTGR_KEYS.includes(step), `${s.keys} needs AltGr`);
  const find = (/** @type {string} */ k) =>
    /** @type {NonNullable<(typeof list)[number]>} */ (
      list.find((s) => s.keys === k)
    );
  find("g s").run(/** @type {any} */ ({}));
  assert.deepEqual(went, ["/stacks"]);
  assert.equal(find("Alt 1").when?.(), false);
  assert.ok(find("Alt 1").reason);
  at = route("/stacks/media/logs");
  assert.equal(find("Alt 1").when?.(), true);
  find("Alt 1").run(/** @type {any} */ ({}));
  find("t n").run(/** @type {any} */ ({}));
  assert.deepEqual(went.slice(1), ["/stacks/media", "/stacks/media/apps"]);
});
