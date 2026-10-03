// redesign-flows (3.71.0): the building blocks the Update flow and the
// Inbox share that ui.js does not have yet — a flow stepper, a live step
// list, a one-of segmented switch, a dismissible "this replaces" note, a
// done mark and a page stylesheet loader. Same API shape as ui.js, so they
// can move there as they are (named in the helper's report).

import { h } from "../dom.js";

/**
 * A flow's stepper (flows/update.html `.fx-steps`): one bar per step, the
 * done ones green, the current one in the primary colour and bold.
 * @param {string[]} labels
 * @returns {{el: HTMLElement, set: (step: number) => void}}
 */
export function stepper(labels) {
  const items = labels.map((l) => h("li", null, h("span", null, l)));
  const el = h(
    "ol",
    {
      class: "fl-steps",
      style: `--n:${labels.length}`,
      "aria-label": "Steps",
    },
    ...items,
  );
  return {
    el,
    set: (step) =>
      items.forEach((li, i) => {
        const s = i + 1 < step ? "done" : i + 1 === step ? "now" : "";
        if (s) li.dataset.s = s;
        else delete li.dataset.s;
        if (s === "now") li.setAttribute("aria-current", "step");
        else li.removeAttribute("aria-current");
      }),
  };
}

/**
 * @typedef {{title: string, desc: string, state: "wait" | "run" | "ok" |
 *   "bad" | "skip", time?: string, note?: string}} CheckRow
 */

/**
 * A live list of steps (flows/update.html `.fx-check`): a status ring
 * (waiting, spinning, ticked, failed, skipped), the step and its one-line
 * description, and how long it took.
 * @param {CheckRow[]} rows
 */
export function checkList(rows) {
  return h(
    "ul",
    { class: "fl-check", "aria-label": "Progress" },
    ...rows.map((r) =>
      h(
        "li",
        { "data-s": r.state },
        h("span", {
          class: "fl-check__st",
          "aria-label":
            r.state === "ok"
              ? "done"
              : r.state === "run"
                ? "running"
                : r.state === "bad"
                  ? "failed"
                  : r.state === "skip"
                    ? "skipped"
                    : "waiting",
        }),
        h(
          "span",
          null,
          r.title,
          h("small", null, r.note ? `${r.desc} · ${r.note}` : r.desc),
        ),
        h("time", null, r.time ?? ""),
      ),
    ),
  );
}

/**
 * A one-of segmented switch ("Worst first" / "Newest").
 * @param {{label: string, items: {value: string, label: string,
 *   hint?: string}[], value: string, onChange: (v: string) => void}} spec
 * @returns {{el: HTMLElement, set: (v: string) => void,
 *   buttons: HTMLElement[]}}
 */
export function segSwitch(spec) {
  let cur = spec.value;
  const buttons = spec.items.map((it) => {
    const b = h(
      "button",
      { type: "button", "data-value": it.value, title: it.hint ?? it.label },
      it.label,
    );
    b.addEventListener("click", () => {
      if (cur === it.value) return;
      cur = it.value;
      paint();
      spec.onChange(cur);
    });
    return b;
  });
  const paint = () =>
    buttons.forEach((b, i) =>
      b.setAttribute("aria-pressed", String(spec.items[i].value === cur)),
    );
  paint();
  return {
    el: h(
      "div",
      { class: "fl-seg", role: "group", "aria-label": spec.label },
      ...buttons,
    ),
    set: (v) => {
      cur = v;
      paint();
    },
    buttons,
  };
}

/**
 * A dismissible explanation (FLOWS.md §1.5 "ⓘ This replaces…"), gone for
 * good in this browser once closed.
 * @param {string} key the browser's memory of the dismissal
 * @param {(Node | string)[]} words
 * @returns {{el: HTMLElement, close: HTMLElement} | null} null once dismissed
 */
export function explainNote(key, words) {
  try {
    if (localStorage.getItem(key)) return null;
  } catch {
    /* no storage: always shown */
  }
  const close = h(
    "button",
    {
      type: "button",
      class: "fl-explain__x",
      "aria-label": "Hide this explanation",
      title: "Hide this explanation for good",
    },
    "✕",
  );
  const el = h(
    "div",
    { class: "fl-explain", role: "note" },
    h("span", { "aria-hidden": "true" }, "ⓘ"),
    h("span", null, ...words),
    close,
  );
  close.addEventListener("click", () => {
    el.remove();
    try {
      localStorage.setItem(key, "1");
    } catch {
      /* fine: it comes back next time */
    }
  });
  return { el, close };
}

/** The big tick of a finished flow or an empty Inbox. */
export function doneMark(tone = "ok") {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 64 64");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2.4");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("fl-done__mark", `fl-done__mark--${tone}`);
  const c = document.createElementNS(ns, "circle");
  c.setAttribute("cx", "32");
  c.setAttribute("cy", "32");
  c.setAttribute("r", "27");
  c.setAttribute("opacity", ".25");
  const p = document.createElementNS(ns, "path");
  p.setAttribute(
    "d",
    tone === "ok" ? "m20 33 8 8 16-18" : "M24 24l16 16M40 24 24 40",
  );
  svg.append(c, p);
  return svg;
}
