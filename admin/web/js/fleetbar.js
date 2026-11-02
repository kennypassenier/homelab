// The Stacks list's selection bar and what its buttons do: the kp selection
// bar over table and cards, a Deploy that waits ten seconds behind an undo
// toast (or asks first, in the batch dialog), Back up now, and Update….

import { openBatch, reopenBatch } from "./actiondialog.js";
import { send } from "./act.js";
import { h } from "./dom.js";
import { drivable, viaForm } from "./drivable.js";
import { driven } from "./drivehooks.js";
import { BATCH } from "./fleetcontrols.js";
import { updateHrefFor } from "./updateflow.js";
import {
  setSelectionBusy,
  setSelectionResult,
  undoToast,
} from "/static/kp/dist/kp-themes.min.js";

/** How long a Deploy waits behind its undo toast. */
export const UNDO_MS = 10000;

/** @param {number} n @param {string} one */
export const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/**
 * @param {string} action
 * @param {string} label
 * @param {string} cls
 * @param {string} title
 */
function batchButton(action, label, cls, title) {
  return drivable(
    viaForm(
      h(
        "button",
        {
          type: "button",
          class: cls,
          "data-kp-selection-action": action,
          "data-batch": action,
          title,
        },
        label,
      ),
      "batch",
    ),
    BATCH,
    action,
  );
}

/**
 * The selection bar, with the Deploy asks switch beside the Deploy it governs.
 * @param {HTMLElement} safetyEl
 */
export function createSelectionBar(safetyEl) {
  return h(
    "div",
    {
      class: "kp-selection fl-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "fl-body",
      "data-kp-placement": "floating-bottom",
      "data-kp-undo-ms": String(UNDO_MS),
      id: "fl-bar",
      "aria-label": "Selected stacks",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      batchButton(
        "deploy",
        "Deploy",
        "kp-button kp-button--primary",
        "Deploy every ticked stack, one after the other, each backed up first",
      ),
      batchButton(
        "backup",
        "Back up now",
        "kp-button",
        "Back up every ticked stack now",
      ),
      batchButton(
        "update",
        "Update",
        "kp-button",
        "Update the ticked stacks in the Update flow: see what changes, then back up, update and verify",
      ),
      h(
        "div",
        { class: "fl-asks" },
        h("span", { class: "fl-asks__label" }, "Deploy asks"),
        safetyEl,
      ),
    ),
  );
}

/**
 * Wire the bar's actions.
 * @param {{bar: HTMLElement, toastBed: HTMLElement, tbody: HTMLElement,
 *   st: {rows: any[], asks: string},
 *   ctx: {navigate: (href: string) => void}}} env
 */
export function wireBatch(env) {
  const { bar, toastBed, tbody, st, ctx } = env;

  /** @param {string} action @param {string[]} keys */
  const sendBatch = async (action, keys) => {
    const r = await send(
      "POST",
      "/data/actions/batch",
      { action, stacks: keys, args: {} },
      `${action} ${plural(keys.length, "stack")}`,
    );
    if (!r.ok) {
      setSelectionBusy(bar, null);
      setSelectionResult(bar, {
        done: 0,
        refused: keys.map((key) => ({ key, reason: r.error.why })),
      });
      throw new Error(r.error.why);
    }
    setSelectionBusy(bar, null);
    void reopenBatch(r.body.batch);
  };

  /**
   * The state word of ticked rows while a deploy waits.
   * @param {string[]} keys @param {string | null} word
   */
  function markWord(keys, word) {
    for (const k of keys) {
      const cell = tbody.querySelector(
        `tr[data-kp-row-key="${CSS.escape(k)}"] .kp-state-word`,
      );
      if (cell instanceof HTMLElement) {
        const r = st.rows.find((x) => x.name === k);
        cell.textContent = word ?? r?.state.label ?? "";
      }
    }
  }

  bar.addEventListener("kp-selection-action", (e) => {
    const { action, button, keys } = /** @type {CustomEvent} */ (e).detail;
    if (action === "update") {
      ctx.navigate(updateHrefFor(keys));
      return;
    }
    // Live view replays and "Asks first" use the batch dialog: its preview
    // lists each stack's command and its guard before anything starts.
    if (action === "backup" || st.asks === "confirm" || driven()) {
      void openBatch(action, keys);
      return;
    }
    // Deploy with undo: the rows say deploying at once, the toast counts
    // ten seconds, and the deploy is sent when the clock ends.
    markWord(keys, "deploying");
    undoToast(`Deploying ${plural(keys.length, "stack")}`, {
      region: toastBed,
      ms: UNDO_MS,
      clock: "ring",
      commit: () => sendBatch("deploy", keys),
      undo: () => markWord(keys, null),
      failed: "The host refused the deploy",
    });
    void button;
  });
}
