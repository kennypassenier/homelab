// ux-inbox-5: which stacks of a batch will run. Every row of the batch
// dialog has a tick; a stack whose preview shows it cannot run (no manifest,
// no preview) stands unticked and disabled with one plain sentence, and the
// run button counts only the ticked ones. Pure helpers plus the tick itself;
// actiondialog.js draws the dialog.

import { h } from "./dom.js";

/** Words that belong to the machine, not to the person reading. */
const PATH = /(?:\/[\w.@+-]+){2,}/g;
const OS_ERROR = /\s*\((?:os error \d+|[^)]*errno[^)]*)\)/gi;

/**
 * One plain sentence for a stack that cannot run, from the server's raw
 * words (a path, "No such file or directory (os error 2)"): no paths, no
 * error numbers, no file names.
 * @param {string | null | undefined} raw what the preview said
 * @param {string} label the action, "Deploy"
 * @returns {string}
 */
export function plainWhyNot(raw, label) {
  const text = (raw ?? "").replace(OS_ERROR, "").replace(PATH, "").trim();
  const verb = label.toLowerCase();
  if (/no such file|not found|missing|does not exist/i.test(raw ?? ""))
    return `Left out: this stack has no manifest in the repository, so there is nothing to ${verb}.`;
  if (/permission|denied/i.test(raw ?? ""))
    return `Left out: the dashboard may not read this stack's manifest, so it cannot ${verb} it.`;
  return text
    ? `Left out: ${verb} cannot run on this stack yet.`
    : `Left out: the dashboard could not work out how to ${verb} this stack.`;
}

/**
 * The tick of one batch row.
 * @param {string} stack
 * @param {{checked: boolean, disabled: boolean}} state
 * @returns {HTMLInputElement}
 */
export function tickEl(stack, state) {
  const box = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "checkbox",
      class: "kp-checkbox batch-preview__tick",
      "aria-label": `Include ${stack}`,
      "data-tick": stack,
    })
  );
  box.checked = state.checked;
  box.disabled = state.disabled;
  return box;
}

/**
 * The stacks of a batch that will run, in the batch's own order.
 * @param {string[]} stacks
 */
export function selection(stacks) {
  const chosen = new Set(stacks);
  return {
    /** @param {string} s */
    has: (s) => chosen.has(s),
    /** @param {string} s @param {boolean} on */
    set(s, on) {
      if (on) chosen.add(s);
      else chosen.delete(s);
    },
    list: () => stacks.filter((s) => chosen.has(s)),
    count: () => chosen.size,
  };
}

/**
 * The run button's words for a count: "Deploy 4 stacks".
 * @param {string} label @param {number} n
 */
export const runLabel = (label, n) =>
  `${label} ${n === 1 ? "1 stack" : `${n} stacks`}`;

/**
 * The line under the list: what was left out, or why nothing can run.
 * @param {number} total @param {number} left
 */
export function leftOutText(total, left) {
  if (left === 0) return "";
  if (left === total)
    return "None of these stacks can run, so there is nothing to start.";
  return `${left} of ${total} stacks cannot run and are left out.`;
}
