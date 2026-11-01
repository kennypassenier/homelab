// The kp wizard of a New stack page (ux-stacknew-2): the steps' markup, the
// Back / Cancel / Next row, the wizard handle and the screen that replaces
// the wizard once the commit went through. The preset and empty routes
// (newstack.js) and the bundle route (importstack.js) both draw with it, so
// the three look and move alike and end on the same review.

import { committedView } from "./newreview.js";
import { h } from "./dom.js";
import {
  FINISH_EVENT,
  STEP_EVENT,
  attachWizards,
  wizard,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @param {HTMLElement} holder where the wizard goes
 * @param {{id: string,
 *   steps: {id: string, label: string, body: Node[]}[],
 *   navigate: (href: string) => void,
 *   onFinish: () => void}} o
 */
export function wizardShell(holder, o) {
  const next = /** @type {HTMLButtonElement} */ (
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        "data-kp-wizard-next": "",
        "data-kp-finish": "Commit",
        "data-kp-next": "Next",
        id: "new-next",
      },
      "Next",
    )
  );
  const back = /** @type {HTMLButtonElement} */ (
    h(
      "button",
      { type: "button", class: "kp-button", "data-kp-wizard-back": "" },
      "Back",
    )
  );
  const cancel = /** @type {HTMLAnchorElement} */ (
    h(
      "a",
      {
        class: "kp-button kp-button--ghost",
        href: "/stacks",
        id: "new-cancel",
      },
      "Cancel",
    )
  );
  cancel.addEventListener("click", (ev) => {
    ev.preventDefault();
    o.navigate("/stacks");
  });
  const wiz = h(
    "div",
    {
      class: "kp-wizard ns-wizard",
      "data-kp-wizard": "",
      "data-kp-validate": "false",
      "data-form": o.id,
    },
    h(
      "ol",
      { class: "kp-wizard__steps", "data-kp-wizard-steps": "" },
      ...o.steps.map((s) =>
        h("li", { "data-kp-step-label": "", "data-step": s.id }, s.label),
      ),
    ),
    ...o.steps.map((s, i) =>
      h(
        "section",
        {
          "data-kp-step": "",
          "data-step": s.id,
          "aria-label": s.label,
          class: "kp-card ns-step",
          ...(i > 0 ? { hidden: "" } : {}),
        },
        ...s.body,
      ),
    ),
    h("div", { class: "kp-wizard__actions ns-actions" }, back, cancel, next),
  );
  holder.replaceChildren(wiz);
  const detach = attachWizards(holder, { focusStep: false });
  const handle = wizard(wiz);
  let stopJob = () => {};
  /** @type {() => void} */
  let done = () => {};
  const closed = new Promise((r) => (done = () => r(undefined)));
  wiz.addEventListener(FINISH_EVENT, () => o.onFinish());
  // Nothing to go back to on the first step (Kenny's Restore finding).
  back.hidden = true;
  wiz.addEventListener(STEP_EVENT, (e) => {
    back.hidden = /** @type {CustomEvent} */ (e).detail.step === 0;
  });
  return {
    wiz,
    next,
    back,
    cancel,
    handle,
    closed,
    /** What the commit answered, in the wizard's place.
     * @param {any} answer @param {string} name */
    showCommitted: (answer, name) => {
      if (holder.dataset.committed) return;
      holder.dataset.committed = "1";
      const v = committedView(answer, name, o.navigate);
      stopJob = v.stop;
      handle?.element.remove();
      holder.replaceChildren(h("div", { class: "ns-done" }, ...v.nodes));
    },
    stop: () => {
      stopJob();
      detach();
      done();
    },
  };
}
