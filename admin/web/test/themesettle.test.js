// kpa-53: a theme kp-themes no longer ships, left in a browser's storage,
// is replaced at start by kp's own fallback, on the page and in storage, so
// the picker and the storage agree. No theme name is part of the rule: the
// names below come from kp's own list.
import { test } from "node:test";
import assert from "node:assert/strict";
import { install } from "./support/minidom.mjs";

const doc = install();
const { settleStoredTheme } = await import("../js/themesettle.js");
const kp = await import("/static/kp/js/theme-core.js");

/** A storage with one value. @param {string | null} value */
const storage = (value) => {
  /** @type {Map<string, string>} */
  const m = new Map(value == null ? [] : [[kp.STORAGE_KEY, value]]);
  return {
    getItem: (/** @type {string} */ k) => m.get(k) ?? null,
    setItem: (/** @type {string} */ k, /** @type {string} */ v) =>
      void m.set(k, v),
    removeItem: (/** @type {string} */ k) => void m.delete(k),
    clear: () => m.clear(),
    key: () => null,
    length: 0,
  };
};

test("kpa-53: a stored theme kp does not know is replaced by kp's fallback, page and storage", () => {
  const gone = "a-theme-that-was-removed";
  assert.equal(kp.isTheme(gone), false);
  const store = storage(gone);
  const root = doc.documentElement;
  root.setAttribute("data-theme", gone);
  const next = settleStoredTheme(store, root);
  assert.equal(next, kp.DEFAULT_THEME);
  assert.equal(root.getAttribute("data-theme"), kp.DEFAULT_THEME);
  assert.equal(store.getItem(kp.STORAGE_KEY), kp.DEFAULT_THEME);
});

test("kpa-53: a stored theme kp knows, or none, is left alone", () => {
  const root = doc.documentElement;
  const known =
    kp.THEMES.find((t) => t.name !== kp.DEFAULT_THEME)?.name ??
    kp.DEFAULT_THEME;
  root.setAttribute("data-theme", known);
  const store = storage(known);
  assert.equal(settleStoredTheme(store, root), null);
  assert.equal(root.getAttribute("data-theme"), known);
  assert.equal(store.getItem(kp.STORAGE_KEY), known);
  assert.equal(settleStoredTheme(storage(null), root), null);
  assert.equal(settleStoredTheme(storage(""), root), null);
  assert.equal(settleStoredTheme(null, root), null);
});
