// The Update flow's undo window is 7 days, and kp's countdown spells a length
// as hours only ("167:59:59"; gap 124 in ~/Projects/kp-gaps/homelab.md, queued
// for the next kp-themes release: a days unit). Until kp has one, a time left
// of more than a day reads in days here ("6 d 23:59", renewed every half
// minute) and, from the last day on, is kp's own countdown again - the element
// is swapped for it, so the final 24 hours keep kp's seconds, spoken words and
// end event. Nothing else of kp's countdown is re-implemented.

import { h } from "./dom.js";
import { attachCountdowns } from "/static/kp/js/countdown.js";

/** A day, in milliseconds. */
export const DAY_MS = 86_400_000;

/** How often the days-and-minutes line is read again. */
const RENEW_MS = 30_000;

/** @param {number} n */
const two = (n) => String(n).padStart(2, "0");

/**
 * A time left in days and minutes, rounded down: "6 d 23:59", "0 d 00:05".
 * @param {number} ms
 */
export function daysLeft(ms) {
  const minutes = Math.max(0, Math.floor(ms / 60_000));
  const rest = minutes % 1440;
  return `${Math.floor(minutes / 1440)} d ${two(Math.floor(rest / 60))}:${two(rest % 60)}`;
}

/**
 * A countdown to `untilMs` that reads in days while it is more than a day
 * away. `el` goes where the countdown stands; `stop` ends its timers (call it
 * when the card is drawn again or left).
 * @param {number} untilMs
 * @param {string} name what the countdown is for ("The undo window closes in")
 * @param {{now?: () => number}} [options]
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function longCountdown(untilMs, name, { now = () => Date.now() } = {}) {
  const el = h("div", { class: "uf-clock" });
  /** @type {ReturnType<typeof setInterval> | null} */
  let timer = null;
  let detach = () => {};
  const stopTimer = () => {
    if (timer != null) clearInterval(timer);
    timer = null;
  };
  const toKp = () => {
    stopTimer();
    el.replaceChildren(
      h("div", {
        class: "kp-countdown",
        "data-kp-countdown-until": String(untilMs),
        "data-kp-countdown-name": name,
        "data-kp-countdown-controls": "none",
      }),
    );
    detach = attachCountdowns(el);
  };
  const paint = () => {
    const left = untilMs - now();
    if (left <= DAY_MS) return toKp();
    el.replaceChildren(
      h(
        "p",
        { class: "uf-clock__words", role: "timer" },
        h("span", { class: "uf-clock__name" }, name),
        h("b", { class: "uf-clock__time" }, daysLeft(left)),
      ),
    );
  };
  paint();
  if (!el.querySelector(".kp-countdown")) timer = setInterval(paint, RENEW_MS);
  return {
    el,
    stop: () => {
      stopTimer();
      detach();
    },
  };
}
