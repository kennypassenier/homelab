// TUI parity (G17, `homelab checks answer <id> ok|nok [note]`): an
// "Answer" button beside a manual check. It opens the answer-check action's
// form with the check chosen; the press is one job in the queue, and Claude
// drives the same form with homelab ui open answer-check.

import { act, onAct } from "./act.js";
import { openAction } from "./actiondialog.js";
import { answeredJobs } from "./checks.js";
import { h } from "./dom.js";

/**
 * @param {string} id the check's id
 * @param {() => void} [after] read the list again once the job is sent
 */
export function answerButton(id, after) {
  const b = h(
    "button",
    { type: "button", class: "kp-button kp-button--ghost", "data-check": id },
    "Answer…",
  );
  b.addEventListener("click", async () => {
    const c = await openAction("_host", "answer-check", {
      preset: { check: id },
    });
    if (c && after) void c.closed.then(after);
  });
  return h("td", null, b);
}

/**
 * fix-checks-refresh (Kenny, 2026-09-29): a checks table reads its list
 * again as soon as an answer's job has finished, from the live channel's
 * job events, whoever answered (this tab, another, Claude, the TUI's
 * queue). The answers already done when the page came are not news.
 * @param {() => void} reload
 * @returns {() => void} stop
 */
export function onAnswered(reload) {
  /** @type {Set<number>} */
  const seen = new Set();
  let primed = act.jobsRead;
  if (primed) answeredJobs(act.jobs, seen);
  return onAct("jobs", () => {
    if (!primed) {
      if (!act.jobsRead) return;
      primed = true;
      answeredJobs(act.jobs, seen);
      return;
    }
    if (answeredJobs(act.jobs, seen).length) reload();
  });
}
