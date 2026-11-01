// ux-inbox-2: Snooze on an Inbox row is a short pick, so it is a menu on the
// button (In 1 hour, In 4 hours, Tomorrow 07:00, Pick a time), not a modal.
// A choice snoozes at once and leaves an Undo toast; "Pick a time" opens the
// full dialog (needsyou.js) for a date, or for a snooze without the
// reminder. The moments come from kp-themes' own `snoozeChoices`, the same
// the dialog's picker uses.

import { snoozeKeyOf } from "./inboxlist.js";
import { SNOOZE_MAX, endSnooze, snoozeRow } from "./rowsnooze.js";
import { rowMenu, toastRegion } from "./ui.js";
import { snoozeChoices, undoToast } from "/static/kp/dist/kp-themes.min.js";

/** The choices the menu lists, in order. */
export const MENU_CHOICES = ["1h", "4h", "tomorrow"];
/** The morning "Tomorrow" means. */
export const MORNING = "07:00";
/** How long a snooze can be taken back [ms]. */
export const SNOOZE_UNDO_MS = 6000;

/**
 * @typedef {{key: string, snoozeKey?: string, title: string, href?: string}} SnoozeTarget
 */

/**
 * Snooze a row until `atMs` and offer Undo.
 * @param {SnoozeTarget} row
 * @param {number} atMs
 * @param {string} moment the moment in words, for the toast
 * @param {HTMLElement} page
 * @returns {Promise<void>}
 */
async function snoozeWithUndo(row, atMs, moment, page) {
  const res = await snoozeRow(row, Math.round(atMs / 1000), true);
  if (!res.ok) {
    undoToast(`Not snoozed: ${res.error.why}`, {
      region: toastRegion(page),
      ms: 0,
    });
    return;
  }
  undoToast(`Snoozed until ${moment}: ${row.title}`, {
    region: toastRegion(page),
    mode: "revert",
    ms: SNOOZE_UNDO_MS,
    max: 3,
    undo: async () => {
      const back = await endSnooze(snoozeKeyOf(row));
      if (!back.ok) throw new Error(back.error.why);
    },
  });
}

/**
 * Open the snooze menu on a row's Snooze button.
 * @param {{anchor: HTMLElement, row: SnoozeTarget, page: HTMLElement,
 *   openPicker: () => void,
 *   mark: (part: HTMLElement, key: string) => void}} spec
 */
export function openSnoozeMenu(spec) {
  const choices = snoozeChoices({
    now: Date.now(),
    choices: MENU_CHOICES,
    morning: MORNING,
    max: SNOOZE_MAX,
  }).filter((c) => c.at != null);
  rowMenu({
    anchor: spec.anchor,
    label: `Snooze ${spec.row.title}`,
    title: "Snooze",
    items: [
      ...choices.map((c) => ({
        name: `snooze-${c.key}`,
        label: c.label,
        hint: c.moment,
        mark: (/** @type {HTMLElement} */ part) => spec.mark(part, c.key),
        run: () =>
          void snoozeWithUndo(spec.row, c.at ?? 0, c.moment, spec.page),
      })),
      {
        name: "snooze-pick",
        label: "Pick a time",
        hint: "Another date, or without the reminder",
        mark: (part) => spec.mark(part, "pick-time"),
        run: spec.openPicker,
      },
    ],
  });
}
