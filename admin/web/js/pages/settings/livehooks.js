// Live view: the `host-settings` hooks the replay works the page with.

import { register } from "../../drivehooks.js";
import { MIRROR } from "../../settingsform.js";
import { setRowText, wantText } from "./rowtext.js";

/**
 * @param {import("./ctx.js").Ctx} ctx
 * @returns {() => void} unregister
 */
export function registerLiveHooks(ctx) {
  let lastSaved = "";
  /** @param {string} key */
  const rowOf = (key) =>
    ctx.groupBox.querySelector(`[data-key="${CSS.escape(key)}"]`);
  return register("host-settings", {
    ready: () => ctx.page !== null && ctx.detachBar !== null,
    openKey: (/** @type {string} */ key) => ctx.focusKey(key),
    rowButton: (/** @type {string} */ key) => {
      const row = rowOf(key);
      const d = row?.closest("details");
      if (d) d.open = true;
      return /** @type {HTMLElement | null} */ (
        row?.querySelector("[data-st-name]") ?? null
      );
    },
    reviewButton: () =>
      ctx.saveBtn && !(/** @type {HTMLButtonElement} */ (ctx.saveBtn).disabled)
        ? ctx.saveBtn
        : null,
    setStaged: (
      /** @type {Record<string, unknown>} */ want,
      /** @type {string[]} */ typedKeys,
    ) => {
      if (!ctx.page) return;
      for (const f of ctx.page.fields) {
        const row = rowOf(f.key);
        if (!row) continue;
        if (f.key in want) {
          setRowText(row, f, wantText(f, want[f.key]));
        } else if (row.hasAttribute("data-kp-changed")) {
          const m = row.querySelector(`[${MIRROR}]`);
          setRowText(row, f, m?.getAttribute("value") ?? "");
        }
      }
      for (const k of typedKeys) ctx.typed.add(k);
    },
    openReview: () =>
      /** @type {HTMLElement | null} */ (
        ctx.barSlot.querySelector(".kp-unsaved__count")
      )?.click(),
    showSaved: (/** @type {any} */ answer) => {
      const key = JSON.stringify(answer);
      if (key === lastSaved) return;
      lastSaved = key;
      ctx.saved(answer);
    },
  });
}
