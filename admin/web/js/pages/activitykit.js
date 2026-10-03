// redesign-activity (3.71.0): the few building blocks the Activity and
// Console pages need that ui.js does not have yet. Same API shape as
// ui.js, so they can move there when the pages are merged:
//
//   segmented   a one-of-N view switch (the header's Window: 1 / 7 / 14 /
//               30 days), `aria-pressed` buttons in one pill
//   keysRow     the compact key row under a page ("/ search · j k …")
//   ensureStyle a page's own stylesheet, linked once
//   kbd         one key chip

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

/**
 * A page's own stylesheet, linked once (css/pages/<page>.css).
 * @param {string} href
 */
export function ensureStyle(href) {
  if (document.querySelector(`link[data-page-style="${href}"]`)) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = href;
  l.dataset.pageStyle = href;
  document.head.append(l);
}

/** One key, as a chip. @param {string} k */
export const kbd = (k) => h("kbd", { class: "nx-kbd" }, k);

/**
 * A one-of-N segmented control (DESIGN_LANGUAGE §12 "view & state": a time
 * range, a Table / Cards switch). Each button says what it shows.
 * @param {{label: string, options: {value: string, label: string,
 *   hint?: string}[], value: string, onChange: (v: string) => void,
 *   decorate?: (b: HTMLButtonElement, value: string) => void}} spec
 * @returns {{el: HTMLElement, set: (v: string) => void}}
 */
export function segmented(spec) {
  /** @type {HTMLButtonElement[]} */
  const buttons = [];
  const el = h("div", {
    class: "ac-seg",
    role: "group",
    "aria-label": spec.label,
  });
  const set = (/** @type {string} */ v) => {
    for (const b of buttons)
      b.setAttribute("aria-pressed", String(b.dataset.value === v));
  };
  for (const o of spec.options) {
    const b = /** @type {HTMLButtonElement} */ (
      h(
        "button",
        {
          type: "button",
          class: "ac-seg__btn",
          "data-value": o.value,
          title: o.hint ?? o.label,
        },
        o.label,
      )
    );
    b.addEventListener("click", () => {
      set(o.value);
      spec.onChange(o.value);
    });
    spec.decorate?.(b, o.value);
    buttons.push(b);
    el.append(b);
  }
  set(spec.value);
  return { el, set };
}

/**
 * The page's key row (DESIGN_LANGUAGE §10 "Hints"): each key beside what
 * it does.
 * @param {[string, string][]} pairs
 */
export function keysRow(pairs) {
  return h(
    "p",
    { class: "ac-keys", "aria-label": "Keyboard shortcuts" },
    ...pairs.map(([k, what]) => h("span", null, kbd(k), ` ${what}`)),
  );
}
