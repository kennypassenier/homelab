// Binds a kp selection bar once it is in the document. The bar finds the
// table or list it serves by id (`data-kp-selection-for`), so it can only be
// attached after the page put its elements in the document, which is after
// the module that builds them returned. `rules()` is read again on every
// `refresh()` (a rule that depends on the page's state).

import {
  attachSelectionBars,
  setSelectionRules,
} from "/static/kp/js/selection-bar.js";

/**
 * @param {HTMLElement} root the element that holds the bar and its list
 * @param {HTMLElement} bar
 * @param {() => Record<string, (items: {key: string, label: string}[]) => string | null>} rules
 * @param {() => void} [after] runs once the bar is attached (its own parts exist)
 * @param {(clear: HTMLElement) => void} [markClear] marks the bar's "untick all" as its declared Live view control
 */
export function bindSelection(root, bar, rules, after, markClear) {
  let stopped = false;
  let attached = false;
  /** @type {() => void} */
  let detach = () => {};
  const go = () => {
    if (stopped || attached) return;
    if (!bar.isConnected) {
      requestAnimationFrame(go);
      return;
    }
    attached = true;
    detach = attachSelectionBars(root);
    setSelectionRules(bar, rules());
    const clear = bar.querySelector(".kp-selection__clear");
    if (markClear && clear instanceof HTMLElement && !clear.dataset.drive)
      markClear(clear);
    after?.();
  };
  queueMicrotask(go);
  return {
    refresh: () => {
      if (attached) setSelectionRules(bar, rules());
    },
    stop: () => {
      stopped = true;
      detach();
    },
  };
}
