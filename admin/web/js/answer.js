// TUI parity (G17, `homelab checks answer <id> ok|nok [note]`): an
// "Answer" button beside a manual check. It opens the answer-check action's
// form with the check chosen; the press is one job in the queue, and Claude
// drives the same form with homelab ui open answer-check.

import { openAction } from "./actiondialog.js";
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
