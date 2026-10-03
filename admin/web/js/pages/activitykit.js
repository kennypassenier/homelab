// redesign-activity (3.71.0): the two pieces the Activity and Console
// pages share that no kit has: one job, live, in a dialog, and one key
// chip. The segmented switch, the key row and the page stylesheet come
// from hostkit.js (senior review, finding 6), which the shared-kit helper
// moves into ui.js:
//
//   openJobDialog one job's live panel in a dialog
//   kbd           one key chip

import { act, actionLabel } from "../act.js";
import { openDialog } from "../actui.js";
import { h } from "../dom.js";
import { mountJobPanel } from "../jobpanel.js";

/**
 * One job, live, in a dialog: its facts, progress and its log. The log
 * scrolls inside the panel's fixed height; the dialog never grows as lines
 * arrive (invariant 47).
 * @param {number} job
 * @param {{onClose?: () => void}} [opts]
 */
export function openJobDialog(job, opts = {}) {
  const j = act.jobs.find((x) => x.job === job);
  const panel = mountJobPanel(job, { compact: true });
  const d = openDialog({
    title: j
      ? `${actionLabel(j.action)} · ${j.stack === "_host" ? "the whole host" : j.stack} · job ${job}`
      : `Job ${job}`,
    description:
      "This job, live: its steps, how long it has run and every line the host printed for it.",
    body: [panel.element],
    id: "job-dialog",
    wide: true,
  });
  void d.closed.then(() => {
    panel.stop();
    opts.onClose?.();
  });
  return d;
}

/** One key, as a chip. @param {string} k */
export const kbd = (k) => h("kbd", { class: "nx-kbd" }, k);
