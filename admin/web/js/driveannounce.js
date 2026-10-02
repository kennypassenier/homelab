// Live view: announce, plan and pause (Kenny, 2026-09-29, form "Live view
// aankondigen"). Before Claude's step changes the screen, a tab in Live view
// shows "Next: <step>" with the server's countdown and marks the element
// the step acts on; the server holds the step for the countdown, so every
// tab waits the same. Pause holds the step until Continue; Stop ends
// Claude's sequence. The plan Claude sent first is listed beside the page.
//
// The bar sits over the page (fixed), so its coming and going moves
// nothing; inside a driven dialog (a modal dialog hides the page) it is the
// dialog's banner, which is always there and keeps every control's place.

import { send } from "./act.js";
import { notify } from "./actui.js";
import { h } from "./dom.js";
import {
  announceView,
  fieldName,
  leftNow,
  planList,
  targetOf,
} from "./driveview.js";

/**
 * @typedef {import("./driveview.js").DriveState} DriveState
 * @typedef {import("./driveview.js").DriveStep} DriveStep
 * @typedef {{el: HTMLElement, text: HTMLElement,
 *   paint: (v: ReturnType<typeof announceView>, idle: string) => void}} Bar
 * @typedef {{dialog?: HTMLDialogElement | null,
 *   input?: (name: string, id?: string) => HTMLElement | null,
 *   button?: (name: string) => HTMLElement | null,
 *   row?: (op: string, target?: string) => HTMLElement | null}} Ctl
 */

/**
 * @param {"pause" | "continue" | "stop"} what
 * @param {number | null} seq the `seq` this button was last drawn against
 *   (fix-185): a press that arrives once the state has moved on (a delayed
 *   or duplicated click) is refused rather than acted on against whatever
 *   is driving now.
 */
async function press(what, seq) {
  const body = seq == null ? { do: what } : { do: what, seq };
  const r = await send("POST", "/data/drive/control", body, what);
  if (!r.ok) notify(`${r.error.why}.`, "warning");
}

/**
 * One announcement bar. `inline`: the dialog's banner, always shown; its
 * countdown and buttons are hidden without taking their place away.
 * @param {boolean} inline
 * @param {HTMLElement[]} [extra] buttons after Stop (Leave live view)
 * @returns {Bar}
 */
export function makeBar(inline, extra = []) {
  const text = h("span", { class: "drive-announce__text" });
  const status = h("span", { class: "drive-announce__status" });
  const counter = h("span", { class: "drive-announce__counter" });
  const count = h("span", {
    class: "drive-announce__count",
    "aria-hidden": "true",
  });
  const progress = /** @type {HTMLProgressElement} */ (
    h("progress", {
      class: "kp-progress drive-announce__progress",
      max: "1",
      value: "0",
      "aria-label": "Countdown to Claude's next step",
    })
  );
  const pause = h(
    "button",
    { type: "button", class: "kp-button kp-button--secondary" },
    "Pause",
  );
  const resume = h(
    "button",
    { type: "button", class: "kp-button kp-button--primary" },
    "Continue",
  );
  const stop = h(
    "button",
    { type: "button", class: "kp-button kp-button--destructive" },
    "Stop",
  );
  // fix-185: the seq the bar last painted — captured below on every
  // `paint`, so a click always sends the round it was actually drawn for.
  let lastSeq = /** @type {number | null} */ (null);
  pause.addEventListener("click", () => void press("pause", lastSeq));
  resume.addEventListener("click", () => void press("continue", lastSeq));
  stop.addEventListener("click", () => void press("stop", lastSeq));
  const live = h(
    "span",
    { class: "drive-announce__live" },
    count,
    progress,
    h("span", { class: "drive-announce__toggle" }, pause, resume),
    stop,
  );
  const el = h(
    "div",
    {
      class: `kp-alert kp-alert--info drive-announce${inline ? " drive-announce--inline drive-banner" : ""}`,
      role: "status",
      "aria-live": "polite",
    },
    h(
      "span",
      { class: "drive-announce__words" },
      text,
      // The second line is always there, so a pause or a plan never
      // moves the step's words.
      h("span", { class: "drive-announce__meta" }, status, counter),
    ),
    live,
    ...extra,
  );
  if (!inline) el.hidden = true;

  /** @param {HTMLElement} x @param {boolean} on */
  const shown = (x, on) => x.classList.toggle("drive-announce--off", !on);

  /** @type {Bar["paint"]} */
  const paint = (v, idle) => {
    // fix-185: remember the round these buttons now represent, even while
    // idle (v null) — the last one seen is still the right one to refuse a
    // button pressed a moment before Claude starts again.
    if (v) lastSeq = v.seq;
    if (!inline) el.hidden = !v;
    el.dataset.paused = String(!!v?.paused);
    text.textContent = v ? v.text : idle;
    status.textContent = v?.status ?? "";
    shown(status, !!v?.status);
    counter.textContent = v?.counter ?? "";
    shown(counter, !!v?.counter);
    count.textContent = v?.count || "0";
    shown(count, !!v?.countdown);
    progress.value = v?.fraction ?? 0;
    shown(progress, !!v?.countdown);
    shown(live, !!v);
    shown(pause, !!v && !v.paused);
    shown(resume, !!v && v.paused);
  };
  return { el, text, paint };
}

/**
 * The plan beside the page.
 * @returns {{el: HTMLElement, paint: (s: DriveState | null, on: boolean) => void}}
 */
export function makePlan() {
  const list = h("ol", { class: "drive-plan__list" });
  const changed = h(
    "span",
    {
      class: "kp-badge kp-badge--warning drive-plan__changed",
      title: "Claude took a step that was not the plan's next",
    },
    "changed",
  );
  const counter = h("span", { class: "drive-plan__counter" });
  const el = h(
    "aside",
    { class: "drive-plan", "aria-label": "Claude's plan", hidden: "" },
    h(
      "p",
      { class: "drive-plan__head" },
      h("strong", null, "Claude's plan"),
      counter,
      changed,
    ),
    list,
  );
  /** @type {string} */
  let drawn = "";
  /** @param {DriveState | null} s @param {boolean} on */
  const paint = (s, on) => {
    const p = on ? planList(s) : null;
    el.hidden = !p;
    if (!p) return;
    counter.textContent = p.counter;
    changed.classList.toggle("drive-announce--off", !p.changed);
    const key = JSON.stringify(p.items);
    if (key === drawn) return;
    drawn = key;
    list.replaceChildren(
      ...p.items.map((x) =>
        h(
          "li",
          x.mark === "current"
            ? {
                class: "drive-plan__item drive-plan__item--current",
                "aria-current": "step",
              }
            : { class: `drive-plan__item drive-plan__item--${x.mark}` },
          x.text,
        ),
      ),
    );
    list.querySelector(".drive-plan__item--current")?.scrollIntoView({
      block: "nearest",
    });
  };
  return { el, paint };
}

/**
 * The element `step` will act on, on this page now.
 * @param {DriveStep} step
 * @param {Ctl | null} ctl the driven dialog's controller
 * @param {DriveState | null} s
 * @returns {HTMLElement | null}
 */
export function findTarget(step, ctl, s) {
  const t = targetOf(step);
  /** @param {string} sel */
  const q = (sel) =>
    /** @type {HTMLElement | null} */ (document.querySelector(sel));
  switch (t.kind) {
    case "link": {
      const href = CSS.escape(t.path);
      /** @type {HTMLElement[]} */
      const links = [
        ...document.querySelectorAll(`#nav a[href="${href}"]`),
        ...document.querySelectorAll(`#page a[href="${href}"]`),
        ...(t.path.startsWith("/stacks/")
          ? document.querySelectorAll('#nav a[href^="/stacks"]')
          : []),
      ].map((x) => /** @type {HTMLElement} */ (x));
      return links.find(visible) ?? onScreen(links[0] ?? null);
    }
    case "action":
      return q(`#page [data-action="${CSS.escape(t.action)}"]`);
    case "field": {
      const input =
        ctl?.input?.(fieldName(s, t.id), t.id) ?? document.getElementById(t.id);
      return /** @type {HTMLElement | null} */ (
        input?.closest(".kp-field") ?? input ?? null
      );
    }
    case "button":
      return ctl?.button?.(t.button) ?? null;
    case "row":
      return ctl?.row?.(t.op, t.target) ?? null;
    case "select":
      return q("table.fleet");
    case "close":
      return /** @type {HTMLElement | null} */ (
        ctl?.dialog?.querySelector(".kp-dialog__close") ?? null
      );
    default:
      return null;
  }
}

/** @param {Element} el */
const visible = (el) => el.getClientRects().length > 0;

/**
 * The element itself when it is on screen, else what shows it: a link in a
 * closed menu is marked on the menu's own button.
 * @param {HTMLElement | null} el
 * @returns {HTMLElement | null}
 */
function onScreen(el) {
  let x = el;
  while (x && !visible(x)) x = x.parentElement;
  if (!x || x === el) return x;
  // The menu's own button first: the bar's brand link comes before it.
  return /** @type {HTMLElement} */ (
    x.querySelector(":scope > button") ?? x.querySelector(":scope > a") ?? x
  );
}

/** @type {HTMLElement | null} */
let marked = null;

/**
 * Mark the step's target until the step is taken (`unmark`).
 * @param {HTMLElement | null} el
 * @returns {HTMLElement | null} what was marked (the Claude cursor aims
 *   there): the target, or the button of the closed menu that holds it
 */
export function mark(el) {
  unmark();
  el = onScreen(el);
  if (!el) return null;
  marked = el;
  el.classList.add("drive-target");
  el.scrollIntoView({ block: "nearest", inline: "nearest" });
  return el;
}

export function unmark() {
  marked?.classList.remove("drive-target");
  marked = null;
}

/**
 * The countdown's clock: what is left, counted from when the state came.
 * @returns {{set: (s: DriveState | null) => void, left: () => number}}
 */
export function countdown() {
  /** @type {DriveState | null} */
  let s = null;
  let at = 0;
  return {
    set(v) {
      s = v;
      at = Date.now();
    },
    left: () => leftNow(s, Date.now() - at),
  };
}

export { announceView };
