// ux-inbox-3: one Inbox row marked seen, with the same Undo window as the
// bulk Mark seen (inboxcards.js). The row stays where it is, dimmed and
// inert, until the window closes, so the list does not jump under the
// pointer; Undo gives it back, a closed window takes it out. The caller
// holds the list still meanwhile (needsyou.js `holding`).

import { loadNotices, send } from "./act.js";
import { drivable, declare } from "./drivable.js";
import { h } from "./dom.js";
import { toastRegion } from "./ui.js";
import { undoToast } from "/static/kp/dist/kp-themes.min.js";

/** How long the row can be taken back [ms]. */
export const SEEN_UNDO_MS = 6000;

const SEEN_UNDO = declare({
  id: "inbox-seen-undo",
  page: "needs-you",
  opens: "run",
  what: "take a single row's Mark seen back before its clock ends",
  shows: "for 6 seconds after a row's Mark seen",
});

/**
 * Dim the row and stop its buttons while the window is open.
 * @param {HTMLElement} row
 * @returns {() => void} give the row back
 */
function pend(row) {
  row.dataset.ibSeen = "pending";
  row.querySelector(".kp-row-actions")?.setAttribute("inert", "");
  const note = h(
    "span",
    { class: "ib-seen-note", role: "note" },
    "Marked seen. It leaves the list when the Undo window closes.",
  );
  row.querySelector(".kp-action-list__main")?.append(note);
  return () => {
    delete row.dataset.ibSeen;
    row.querySelector(".kp-row-actions")?.removeAttribute("inert");
    note.remove();
  };
}

/**
 * Mark the notices behind one row seen after an Undo window.
 * @param {{row: HTMLElement, title: string, ids: number[], page: HTMLElement,
 *   takeOut: (row: HTMLElement) => Promise<void>,
 *   ms?: number}} spec
 * @returns {Promise<"committed" | "undone" | "failed" | "left">}
 */
export async function markSeenWithUndo(spec) {
  const { row, title, ids } = spec;
  const release = pend(row);
  const shown = undoToast(`Marked seen: ${title}`, {
    region: toastRegion(spec.page),
    ms: spec.ms ?? SEEN_UNDO_MS,
    max: 3,
    commit: async () => {
      const res = await send(
        "POST",
        "/data/notifications/read",
        { ids, read: true },
        "marking it seen",
      );
      if (!res.ok) throw new Error(res.error.why);
      // Read again before the row folds away, so the redraw that follows
      // finds it gone from both.
      await loadNotices();
    },
    undo: release,
    restore: release,
  });
  for (const b of shown.toast.querySelectorAll("button"))
    if ((b.textContent ?? "").trim() === "Undo") drivable(b, SEEN_UNDO);
  const outcome = await shown.done;
  if (outcome === "committed") await spec.takeOut(row);
  else release();
  return outcome;
}
