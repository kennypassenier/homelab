// What a host.toml row's editor holds, read and written as text: the glue
// between the rows (settingsform.js) and the Save bar / Live view.

import { editorKind } from "../../settingsrows.js";
import { MIRROR } from "../../settingsform.js";

/** @typedef {import("./ctx.js").HostField} HostField */

/**
 * What a row's editor holds, as text for `valueFromMirror`.
 * @param {Element} row @param {HostField} f
 */
export function textOf(row, f) {
  const m = /** @type {HTMLInputElement | null} */ (
    row.querySelector(`[${MIRROR}]`)
  );
  if (m) return m.value;
  const kind = editorKind(f);
  if (kind === "switch")
    return /** @type {HTMLInputElement} */ (
      row.querySelector(".kp-switch__input")
    ).checked
      ? "on"
      : "off";
  const c = /** @type {HTMLInputElement | null} */ (
    row.querySelector("input.kp-field__input, textarea, select")
  );
  return c?.value ?? "";
}

/**
 * Put a row's editor to the text a mirror would hold.
 * @param {Element} row @param {HostField} f @param {string} text
 */
export function setRowText(row, f, text) {
  const m = /** @type {HTMLInputElement | null} */ (
    row.querySelector(`[${MIRROR}]`)
  );
  if (m) {
    m.value = text;
    m.dispatchEvent(new Event("input", { bubbles: true }));
    m.dispatchEvent(new Event("change", { bubbles: true }));
    return;
  }
  if (editorKind(f) === "switch") {
    const box = /** @type {HTMLInputElement} */ (
      row.querySelector(".kp-switch__input")
    );
    box.checked = text === "on";
    box.dispatchEvent(new Event("change", { bubbles: true }));
    return;
  }
  const c = /** @type {HTMLInputElement | null} */ (
    row.querySelector("input.kp-field__input, textarea")
  );
  if (c) {
    c.value = text;
    c.dispatchEvent(new Event("input", { bubbles: true }));
    c.dispatchEvent(new Event("change", { bubbles: true }));
  }
}

/**
 * The text a staged value is in a row's editor.
 * @param {HostField} f @param {unknown} v
 */
export function wantText(f, v) {
  const kind = editorKind(f);
  if (v == null) return kind === "switch" ? "off" : "";
  if (kind === "switch") return v === true ? "on" : "off";
  if (kind === "hour") return `${String(v).padStart(2, "0")}:00`;
  if (Array.isArray(v)) return v.join(",");
  return String(v);
}
